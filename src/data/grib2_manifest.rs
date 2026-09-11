//! Kerchunk-compatible GRIB2 reference manifest generation.

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::grib2::{normalized_grib_bytes, raw_product_template, raw_section_payload};
use super::grib2_index::parse_index;
use crate::error::{NcvError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestFormat {
    Kerchunk,
    Virtualizarr,
}

impl ManifestFormat {
    pub fn profile(self) -> &'static str {
        match self {
            Self::Kerchunk => "kerchunk-grib2-v1",
            Self::Virtualizarr => "virtualizarr-kerchunk-grib2-v1",
        }
    }
}

pub fn write_manifest(
    source_path: &Path,
    index_path: &Path,
    output_path: &Path,
    format: ManifestFormat,
    source_uri: Option<&str>,
    strict: bool,
) -> Result<()> {
    let source_len = fs::metadata(source_path)
        .map_err(|source| NcvError::Io {
            path: source_path.to_path_buf(),
            source,
        })?
        .len();
    let index_text = fs::read_to_string(index_path).map_err(|source| NcvError::Io {
        path: index_path.to_path_buf(),
        source,
    })?;
    let records = parse_index(index_path, &index_text, source_len)?;
    if records.is_empty() {
        return Err(NcvError::Grib2 {
            path: index_path.to_path_buf(),
            reason: "index contains no data records".into(),
        });
    }
    let uri = source_uri
        .map(str::to_owned)
        .unwrap_or_else(|| source_path.to_string_lossy().into_owned());
    let mut refs = serde_json::Map::new();
    let mut diagnostics = Vec::new();
    let source_bytes = fs::read(source_path).map_err(|source| NcvError::Io {
        path: source_path.to_path_buf(),
        source,
    })?;
    let parsed_bytes = normalized_grib_bytes(&source_bytes);
    let parsed = catch_unwind(AssertUnwindSafe(|| grib::from_bytes(&parsed_bytes)))
        .map_err(|_| NcvError::Grib2 {
            path: source_path.to_path_buf(),
            reason:
                "GRIB2 product definition template is valid but unsupported by the bundled decoder"
                    .into(),
        })?
        .map_err(|error| NcvError::Grib2 {
            path: source_path.to_path_buf(),
            reason: error.to_string(),
        })?;
    for record in &records {
        let matching = parsed
            .iter()
            .filter(|(_, message)| message.0.body.offset as u64 == record.offset)
            .collect::<Vec<_>>();
        if matching.is_empty() {
            let message = format!(
                ".idx line {} has no corresponding GRIB2 message",
                record.line
            );
            if strict {
                return Err(NcvError::Grib2 {
                    path: index_path.to_path_buf(),
                    reason: message,
                });
            }
            diagnostics.push(message);
            continue;
        }
        let message_length = matching[0].1.indicator().total_length;
        if message_length != record.length {
            let diagnostic = format!(
                ".idx line {} range is {} bytes but GRIB2 header declares {} bytes",
                record.line, record.length, message_length
            );
            if strict {
                return Err(NcvError::Grib2 {
                    path: index_path.to_path_buf(),
                    reason: diagnostic,
                });
            }
            diagnostics.push(diagnostic);
            continue;
        }
        for (submessage_position, (_, message)) in matching.into_iter().enumerate() {
            let key = if submessage_position == 0 {
                format!("grib2_message_{:04}", record.ordinal)
            } else {
                format!(
                    "grib2_message_{:04}_submessage_{submessage_position:04}",
                    record.ordinal
                )
            };
            refs.insert(
                format!("{key}/.zarray"),
                json!({
                    "zarr_format": 2,
                    "shape": [1],
                    "chunks": [1],
                    "dtype": "<f4",
                    "compressor": null,
                    "filters": [{"id": "grib", "var": key}],
                    "order": "C",
                    "fill_value": null
                }),
            );
            refs.insert(
                format!("{key}/.zattrs"),
                json!({
                    "_ARRAY_DIMENSIONS": ["grib2_message"],
                    "source_index_line": record.line,
                    "source_offset": record.offset,
                    "source_length": record.length,
                    "grib2_submessage": submessage_position,
                    "grib2_description": message.describe(),
                    "grib2_discipline": message.indicator().discipline,
                    "grib2_grid_template": message.grid_def().grid_tmpl_num(),
                    "grib2_product_template": raw_product_template(&source_bytes, message.4.body.offset)
                        .unwrap_or_else(|| message.prod_def().prod_tmpl_num()),
                    "grib2_parameter_category": message.prod_def().parameter_category(),
                    "grib2_parameter_number": message.prod_def().parameter_number(),
                    "grib2_product_definition_hex": hex_bytes(
                        &raw_section_payload(&source_bytes, message.4.body)
                            .unwrap_or_else(|| message.prod_def().iter().copied().collect::<Vec<_>>())
                    )
                }),
            );
            refs.insert(
                format!("{key}/0"),
                json!([uri, record.offset, record.length]),
            );
        }
    }
    let root_attrs = json!({
        "ncv_manifest_profile": format.profile(),
        "ncv_source_format": "GRIB2",
        "ncv_source_uri": uri,
        "ncv_index": index_path.to_string_lossy(),
        "ncv_diagnostics": diagnostics,
    });
    refs.insert(".zgroup".into(), json!({"zarr_format": 2}));
    refs.insert(".zattrs".into(), root_attrs);
    let document = json!({"version": 1, "refs": refs});
    write_atomic(output_path, &document)
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_atomic(output_path: &Path, document: &Value) -> Result<()> {
    let parent = output_path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| NcvError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let temporary = PathBuf::from(format!("{}.tmp", output_path.display()));
    let bytes = serde_json::to_vec_pretty(document).map_err(|error| NcvError::Grib2 {
        path: output_path.to_path_buf(),
        reason: format!("serialize manifest: {error}"),
    })?;
    fs::write(&temporary, bytes).map_err(|source| NcvError::Io {
        path: temporary.clone(),
        source,
    })?;
    fs::rename(&temporary, output_path).map_err(|source| NcvError::Io {
        path: output_path.to_path_buf(),
        source,
    })
}
