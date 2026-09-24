//! VirtualiZarr, Kerchunk, and Icechunk reference manifest data source.

use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Arc,
};

use grib::{Grib2SubmessageDecoder, LatLons};
use serde_json::Value;

use super::{
    AxisRole, DataSource, DatasetFormat, DatasetMetadata, Dimension, Variable,
    grib2::normalized_grib_bytes,
    slice::{Slice2D, SliceRequest, Validity},
};
use crate::error::{NcvError, Result};
use crate::storage::location::SourceLocation;

/// A parsed reference chunk location in a manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChunkReference {
    /// Remote or local file byte range: `(uri_or_path, offset, length)`.
    ByteRange {
        uri: String,
        offset: u64,
        length: u64,
    },
    /// Inline byte array.
    Inline(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct ManifestVariableMetadata {
    pub name: String,
    pub dimensions: Vec<String>,
    pub shape: Vec<usize>,
    pub chunks: Vec<usize>,
    pub dtype: String,
    pub fill_value: Option<f64>,
    pub is_grib: bool,
    pub units: Option<String>,
    pub long_name: Option<String>,
    pub standard_name: Option<String>,
}

pub struct ManifestSource {
    path: String,
    base_dir: PathBuf,
    metadata: DatasetMetadata,
    variable_meta: BTreeMap<String, ManifestVariableMetadata>,
    chunk_refs: BTreeMap<String, ChunkReference>,
    is_remote: bool,
}

impl ManifestSource {
    /// Open a local VirtualiZarr, Kerchunk, or Icechunk manifest JSON file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).map_err(|source| NcvError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let base_dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        Self::from_json_str(&content, &path.to_string_lossy(), base_dir, false)
    }

    /// Parse a manifest document from a JSON string.
    pub fn from_json_str(
        json_str: &str,
        path_str: &str,
        base_dir: PathBuf,
        is_remote: bool,
    ) -> Result<Self> {
        let value: Value = serde_json::from_str(json_str).map_err(|error| NcvError::Grib2 {
            path: PathBuf::from(path_str),
            reason: format!("parse manifest JSON: {error}"),
        })?;

        let mut refs_map = serde_json::Map::new();

        if let Some(refs) = value.get("refs").and_then(Value::as_object) {
            refs_map = refs.clone();
        } else if let Some(virtual_chunks) = value.get("virtual_chunks").and_then(Value::as_object)
        {
            // Icechunk / VirtualiZarr store representation
            for (key, chunk_val) in virtual_chunks {
                refs_map.insert(key.clone(), chunk_val.clone());
            }
        } else if let Some(manifest) = value.get("manifest").and_then(Value::as_object) {
            if let Some(refs) = manifest.get("refs").and_then(Value::as_object) {
                refs_map = refs.clone();
            } else if let Some(chunks) = manifest.get("virtual_chunks").and_then(Value::as_object) {
                refs_map = chunks.clone();
            }
        }

        let mut variable_meta = BTreeMap::new();
        let mut chunk_refs = BTreeMap::new();
        let mut dimensions_map = BTreeMap::<String, usize>::new();

        // Gather .zarray and .zattrs to define variables
        for (key, val) in &refs_map {
            if let Some(var_name) = key.strip_suffix("/.zarray") {
                let zarray = val;
                let zattrs = refs_map.get(&format!("{var_name}/.zattrs"));

                let shape = zarray
                    .get("shape")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(Value::as_u64)
                            .map(|v| v as usize)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                let chunks = zarray
                    .get("chunks")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(Value::as_u64)
                            .map(|v| v as usize)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                let dtype = zarray
                    .get("dtype")
                    .and_then(Value::as_str)
                    .unwrap_or("<f4")
                    .to_string();

                let fill_value = zarray.get("fill_value").and_then(Value::as_f64);

                let is_grib = zarray
                    .get("filters")
                    .and_then(Value::as_array)
                    .map(|filters| {
                        filters.iter().any(|f| {
                            f.get("id").and_then(Value::as_str) == Some("grib")
                                || f.get("id").and_then(Value::as_str) == Some("grib2")
                        })
                    })
                    .unwrap_or(false)
                    || zattrs.and_then(|a| a.get("grib2_discipline")).is_some();

                let dim_names = zattrs
                    .and_then(|a| a.get("_ARRAY_DIMENSIONS"))
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(Value::as_str)
                            .map(String::from)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| (0..shape.len()).map(|i| format!("dim_{i}")).collect());

                let units = zattrs
                    .and_then(|a| a.get("units"))
                    .and_then(Value::as_str)
                    .map(String::from);
                let long_name = zattrs
                    .and_then(|a| a.get("long_name").or_else(|| a.get("grib2_description")))
                    .and_then(Value::as_str)
                    .map(String::from);
                let standard_name = zattrs
                    .and_then(|a| a.get("standard_name"))
                    .and_then(Value::as_str)
                    .map(String::from);

                for (dim_name, &dim_len) in dim_names.iter().zip(shape.iter()) {
                    dimensions_map
                        .entry(dim_name.clone())
                        .and_modify(|existing| *existing = (*existing).max(dim_len))
                        .or_insert(dim_len);
                }

                variable_meta.insert(
                    var_name.to_string(),
                    ManifestVariableMetadata {
                        name: var_name.to_string(),
                        dimensions: dim_names,
                        shape,
                        chunks,
                        dtype,
                        fill_value,
                        is_grib,
                        units,
                        long_name,
                        standard_name,
                    },
                );
            } else if !key.ends_with("/.zarray")
                && !key.ends_with("/.zattrs")
                && !key.ends_with("/.zgroup")
                && key != ".zgroup"
                && key != ".zattrs"
            {
                // Parse chunk reference
                if let Some(chunk_ref) = parse_chunk_ref(val) {
                    chunk_refs.insert(key.clone(), chunk_ref);
                }
            }
        }

        let dimensions = dimensions_map
            .into_iter()
            .map(|(name, length)| Dimension {
                role: infer_axis_role(&name),
                name,
                length,
            })
            .collect::<Vec<_>>();

        let variables = variable_meta
            .values()
            .map(|meta| Variable {
                name: meta.name.clone(),
                dimensions: meta.dimensions.clone(),
                numeric: true,
                units: meta.units.clone(),
                long_name: meta.long_name.clone(),
                standard_name: meta.standard_name.clone(),
            })
            .collect::<Vec<_>>();

        let metadata = DatasetMetadata {
            path: path_str.to_string(),
            format: DatasetFormat::VirtualManifest,
            dimensions,
            variables,
        };

        Ok(Self {
            path: path_str.to_string(),
            base_dir,
            metadata,
            variable_meta,
            chunk_refs,
            is_remote,
        })
    }

    /// Read raw bytes for a chunk reference.
    fn read_chunk_bytes(&self, chunk_ref: &ChunkReference) -> Result<Vec<u8>> {
        match chunk_ref {
            ChunkReference::Inline(bytes) => Ok(bytes.clone()),
            ChunkReference::ByteRange {
                uri,
                offset,
                length,
            } => {
                if uri.contains("://") && !uri.starts_with("file://") {
                    // Remote object fetch
                    let location = SourceLocation::parse(uri)?;
                    let store = crate::storage::object_store::build_provider_store(&location)?;
                    let range = crate::storage::object_store::ByteRange::new(
                        *offset,
                        offset + length,
                        offset + length,
                    )?;
                    let runtime = crate::storage::StorageRuntime::spawn()?;
                    let remote = crate::storage::object_store::RemoteStore::new(
                        location.clone(),
                        store.clone(),
                    );
                    let identity =
                        crate::storage::receive(&runtime, async move { remote.head().await })??;
                    let remote = Arc::new(crate::storage::object_store::RemoteStore::new(
                        location, store,
                    ));
                    let bytes = crate::storage::receive(&runtime, async move {
                        remote.read_range(&identity, range).await
                    })??;
                    Ok(bytes.to_vec())
                } else {
                    // Local file fetch
                    let file_path = if let Some(stripped) = uri.strip_prefix("file://") {
                        PathBuf::from(stripped)
                    } else {
                        let p = Path::new(uri);
                        if p.is_absolute() {
                            p.to_path_buf()
                        } else {
                            self.base_dir.join(p)
                        }
                    };
                    let mut file = File::open(&file_path).map_err(|source| NcvError::Io {
                        path: file_path.clone(),
                        source,
                    })?;
                    file.seek(SeekFrom::Start(*offset))
                        .map_err(|source| NcvError::Io {
                            path: file_path.clone(),
                            source,
                        })?;
                    let mut buffer = vec![0_u8; *length as usize];
                    file.read_exact(&mut buffer)
                        .map_err(|source| NcvError::Io {
                            path: file_path,
                            source,
                        })?;
                    Ok(buffer)
                }
            }
        }
    }
}

impl DataSource for ManifestSource {
    fn metadata(&self) -> &DatasetMetadata {
        &self.metadata
    }

    fn is_remote(&self) -> bool {
        self.is_remote
    }

    fn read_slice(&self, request: &SliceRequest) -> Result<Slice2D> {
        let meta = self
            .variable_meta
            .get(&request.variable)
            .ok_or_else(|| NcvError::Grib2 {
                path: PathBuf::from(&self.path),
                reason: format!("variable {} not found in manifest", request.variable),
            })?;

        let bounds = request.bounds;
        let target_rows = bounds.row_end - bounds.row_start;
        let target_cols = bounds.col_end - bounds.col_start;
        let fill_val = meta.fill_value.unwrap_or(f64::NAN);
        let mut array = ndarray::Array2::<f64>::from_elem((target_rows, target_cols), fill_val);

        // Derive spatial chunk sizes
        let chunk_shape = if meta.chunks.is_empty() {
            meta.shape.clone()
        } else {
            meta.chunks.clone()
        };

        let chunk_cols = match meta.dimensions.len() {
            1 => chunk_shape.first().copied().unwrap_or(target_cols),
            _ => chunk_shape.last().copied().unwrap_or(target_cols),
        }
        .max(1);

        let chunk_rows = match meta.dimensions.len() {
            1 => 1,
            2 => chunk_shape.first().copied().unwrap_or(target_rows),
            _ => chunk_shape
                .get(meta.dimensions.len() - 2)
                .copied()
                .unwrap_or(target_rows),
        }
        .max(1);

        let c_time_size = chunk_shape.first().copied().unwrap_or(1).max(1);
        let c_depth_size = chunk_shape.get(1).copied().unwrap_or(1).max(1);

        let first_c_row = bounds.row_start / chunk_rows;
        let last_c_row = bounds.row_end.saturating_sub(1) / chunk_rows;
        let first_c_col = bounds.col_start / chunk_cols;
        let last_c_col = bounds.col_end.saturating_sub(1) / chunk_cols;

        for c_row in first_c_row..=last_c_row {
            for c_col in first_c_col..=last_c_col {
                let chunk_indices = match meta.dimensions.len() {
                    1 => vec![c_col],
                    2 => vec![c_row, c_col],
                    3 => vec![request.time / c_time_size, c_row, c_col],
                    _ => vec![
                        request.time / c_time_size,
                        request.depth / c_depth_size,
                        c_row,
                        c_col,
                    ],
                };

                let chunk_key = resolve_chunk_key(&meta.name, &chunk_indices, &self.chunk_refs);
                let Some(chunk_key) = chunk_key else {
                    continue;
                };

                let chunk_ref = &self.chunk_refs[&chunk_key];
                let bytes = match self.read_chunk_bytes(chunk_ref) {
                    Ok(b) => b,
                    Err(_) => continue,
                };

                if meta.is_grib || bytes.starts_with(b"GRIB") {
                    let parsed_bytes = normalized_grib_bytes(&bytes);
                    let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        grib::from_bytes(&parsed_bytes)
                    }))
                    .map_err(|_| NcvError::Grib2 {
                        path: PathBuf::from(&self.path),
                        reason: "GRIB2 decoder panicked".into(),
                    })?
                    .map_err(|error| NcvError::Grib2 {
                        path: PathBuf::from(&self.path),
                        reason: error.to_string(),
                    })?;

                    let (_, submessage) = parsed.iter().next().ok_or_else(|| NcvError::Grib2 {
                        path: PathBuf::from(&self.path),
                        reason: "GRIB2 message contains no submessage".into(),
                    })?;

                    let (cols, rows) =
                        submessage.grid_shape().map_err(|error| NcvError::Grib2 {
                            path: PathBuf::from(&self.path),
                            reason: error.to_string(),
                        })?;

                    let flip_rows = submessage
                        .latlons()
                        .ok()
                        .and_then(|iter| {
                            let mut iter = iter.into_iter();
                            let first = iter.next()?;
                            let second = iter.next()?;
                            Some(second.0 > first.0)
                        })
                        .unwrap_or(false);

                    let decoder = Grib2SubmessageDecoder::from(submessage).map_err(|error| {
                        NcvError::Grib2 {
                            path: PathBuf::from(&self.path),
                            reason: error.to_string(),
                        }
                    })?;

                    let values = decoder
                        .dispatch()
                        .map_err(|error| NcvError::Grib2 {
                            path: PathBuf::from(&self.path),
                            reason: error.to_string(),
                        })?
                        .map(f64::from)
                        .collect::<Vec<_>>();

                    let r_start = (c_row * chunk_rows).max(bounds.row_start);
                    let r_end = ((c_row + 1) * chunk_rows).min(bounds.row_end);
                    let c_start = (c_col * chunk_cols).max(bounds.col_start);
                    let c_end = ((c_col + 1) * chunk_cols).min(bounds.col_end);

                    for r in r_start..r_end {
                        let out_row = r - bounds.row_start;
                        let src_row = if flip_rows {
                            rows.saturating_sub(1).saturating_sub(r)
                        } else {
                            r
                        };
                        for c in c_start..c_end {
                            let out_col = c - bounds.col_start;
                            let src_col = c;
                            let linear_index = src_row * cols + src_col;
                            if linear_index < values.len() {
                                array[(out_row, out_col)] = values[linear_index];
                            }
                        }
                    }
                } else {
                    let decoded_values = decode_numeric_chunk(&bytes, &meta.dtype)?;
                    let r_start = (c_row * chunk_rows).max(bounds.row_start);
                    let r_end = ((c_row + 1) * chunk_rows).min(bounds.row_end);
                    let c_start = (c_col * chunk_cols).max(bounds.col_start);
                    let c_end = ((c_col + 1) * chunk_cols).min(bounds.col_end);

                    for r in r_start..r_end {
                        let out_row = r - bounds.row_start;
                        let row_in_chunk = r - c_row * chunk_rows;
                        for c in c_start..c_end {
                            let out_col = c - bounds.col_start;
                            let col_in_chunk = c - c_col * chunk_cols;
                            let idx = row_in_chunk * chunk_cols + col_in_chunk;
                            if idx < decoded_values.len() {
                                array[(out_row, out_col)] = decoded_values[idx];
                            }
                        }
                    }
                }
            }
        }

        let validity = ndarray::Array2::from_shape_fn((target_rows, target_cols), |(r, c)| {
            let val = array[(r, c)];
            if val.is_finite() && meta.fill_value.is_none_or(|fill| (val - fill).abs() > 1e-9) {
                Validity::Finite
            } else {
                Validity::Fill
            }
        });
        Slice2D::new(array, validity, bounds)
    }
}

fn resolve_chunk_key(
    var_name: &str,
    chunk_indices: &[usize],
    chunk_refs: &BTreeMap<String, ChunkReference>,
) -> Option<String> {
    let dot_key = format!(
        "{}/{}",
        var_name,
        chunk_indices
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(".")
    );
    if chunk_refs.contains_key(&dot_key) {
        return Some(dot_key);
    }

    let slash_key = format!(
        "{}/{}",
        var_name,
        chunk_indices
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("/")
    );
    if chunk_refs.contains_key(&slash_key) {
        return Some(slash_key);
    }

    let underscore_key = format!(
        "{}/{}",
        var_name,
        chunk_indices
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("_")
    );
    if chunk_refs.contains_key(&underscore_key) {
        return Some(underscore_key);
    }

    if chunk_indices.iter().all(|&i| i == 0) {
        let single_key = format!("{var_name}/0");
        if chunk_refs.contains_key(&single_key) {
            return Some(single_key);
        }
    }

    None
}

#[allow(
    clippy::manual_slice_size_calculation,
    clippy::chunks_exact_to_as_chunks
)]
fn decode_numeric_chunk(bytes: &[u8], dtype: &str) -> Result<Vec<f64>> {
    let dt = dtype.trim();
    if dt.contains("f4") || dt.contains("float32") || dt.ends_with("f") {
        if dt.starts_with('>') {
            Ok(bytes
                .chunks_exact(4)
                .map(|b| f32::from_be_bytes(b.try_into().unwrap()) as f64)
                .collect())
        } else {
            Ok(bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()) as f64)
                .collect())
        }
    } else if dt.contains("f8") || dt.contains("float64") || dt.ends_with("d") {
        if dt.starts_with('>') {
            Ok(bytes
                .chunks_exact(8)
                .map(|b| f64::from_be_bytes(b.try_into().unwrap()))
                .collect())
        } else {
            Ok(bytes
                .chunks_exact(8)
                .map(|b| f64::from_le_bytes(b.try_into().unwrap()))
                .collect())
        }
    } else if dt.contains("i2") || dt.contains("int16") {
        if dt.starts_with('>') {
            Ok(bytes
                .chunks_exact(2)
                .map(|b| i16::from_be_bytes(b.try_into().unwrap()) as f64)
                .collect())
        } else {
            Ok(bytes
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes(b.try_into().unwrap()) as f64)
                .collect())
        }
    } else if dt.contains("u2") || dt.contains("uint16") {
        if dt.starts_with('>') {
            Ok(bytes
                .chunks_exact(2)
                .map(|b| u16::from_be_bytes(b.try_into().unwrap()) as f64)
                .collect())
        } else {
            Ok(bytes
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes(b.try_into().unwrap()) as f64)
                .collect())
        }
    } else if dt.contains("i4") || dt.contains("int32") || dt.ends_with("i") {
        if dt.starts_with('>') {
            Ok(bytes
                .chunks_exact(4)
                .map(|b| i32::from_be_bytes(b.try_into().unwrap()) as f64)
                .collect())
        } else {
            Ok(bytes
                .chunks_exact(4)
                .map(|b| i32::from_le_bytes(b.try_into().unwrap()) as f64)
                .collect())
        }
    } else if dt.contains("u4") || dt.contains("uint32") {
        if dt.starts_with('>') {
            Ok(bytes
                .chunks_exact(4)
                .map(|b| u32::from_be_bytes(b.try_into().unwrap()) as f64)
                .collect())
        } else {
            Ok(bytes
                .chunks_exact(4)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as f64)
                .collect())
        }
    } else if dt.contains("i1") || dt.contains("int8") {
        Ok(bytes.iter().map(|&b| (b as i8) as f64).collect())
    } else if dt.contains("u1") || dt.contains("uint8") {
        Ok(bytes.iter().map(|&b| b as f64).collect())
    } else if dt.contains("i8") || dt.contains("int64") {
        if dt.starts_with('>') {
            Ok(bytes
                .chunks_exact(8)
                .map(|b| i64::from_be_bytes(b.try_into().unwrap()) as f64)
                .collect())
        } else {
            Ok(bytes
                .chunks_exact(8)
                .map(|b| i64::from_le_bytes(b.try_into().unwrap()) as f64)
                .collect())
        }
    } else if dt.contains("u8") || dt.contains("uint64") {
        if dt.starts_with('>') {
            Ok(bytes
                .chunks_exact(8)
                .map(|b| u64::from_be_bytes(b.try_into().unwrap()) as f64)
                .collect())
        } else {
            Ok(bytes
                .chunks_exact(8)
                .map(|b| u64::from_le_bytes(b.try_into().unwrap()) as f64)
                .collect())
        }
    } else {
        Err(NcvError::Grib2 {
            path: PathBuf::from("manifest"),
            reason: format!("unsupported array data type '{dtype}'"),
        })
    }
}

fn parse_chunk_ref(val: &Value) -> Option<ChunkReference> {
    if let Some(arr) = val.as_array() {
        if arr.len() >= 3 {
            let uri = arr[0].as_str()?.to_string();
            let offset = arr[1].as_u64()?;
            let length = arr[2].as_u64()?;
            return Some(ChunkReference::ByteRange {
                uri,
                offset,
                length,
            });
        }
    } else if let Some(obj) = val.as_object() {
        let uri = obj
            .get("path")
            .or_else(|| obj.get("url"))
            .and_then(Value::as_str)?
            .to_string();
        let offset = obj.get("offset").and_then(Value::as_u64)?;
        let length = obj.get("length").and_then(Value::as_u64)?;
        return Some(ChunkReference::ByteRange {
            uri,
            offset,
            length,
        });
    } else if let Some(s) = val.as_str() {
        if let Some(b64) = s.strip_prefix("base64:") {
            if let Ok(bytes) = base64_simd::STANDARD.decode_to_vec(b64) {
                return Some(ChunkReference::Inline(bytes));
            }
        } else {
            return Some(ChunkReference::Inline(s.as_bytes().to_vec()));
        }
    }
    None
}

fn infer_axis_role(name: &str) -> AxisRole {
    let lower = name.to_ascii_lowercase();
    if lower.contains("lat") || lower == "y" || lower.contains("north") {
        AxisRole::Latitude
    } else if lower.contains("lon") || lower == "x" || lower.contains("east") {
        AxisRole::Longitude
    } else if lower.contains("time")
        || lower == "t"
        || lower.contains("step")
        || lower.contains("grib")
    {
        AxisRole::Time
    } else if lower.contains("depth")
        || lower.contains("lev")
        || lower == "z"
        || lower.contains("height")
    {
        AxisRole::Depth
    } else {
        AxisRole::Other
    }
}

/// Check if a local path is a VirtualiZarr / Kerchunk / Icechunk JSON manifest file.
pub fn is_manifest_file(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "json" && ext != "manifest" {
        return false;
    }
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut buffer = Vec::new();
    if file.read_to_end(&mut buffer).is_err() {
        return false;
    }
    let text = String::from_utf8_lossy(&buffer);
    text.contains("\"refs\"")
        || text.contains("\"virtual_chunks\"")
        || text.contains("\"ncv_manifest_profile\"")
        || text.contains("\"virtualizarr")
        || text.contains("\"icechunk\"")
}
