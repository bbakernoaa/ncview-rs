//! Read-only GRIB2 data source backed by the pure-Rust `grib` decoder.

use std::collections::HashSet;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{SecondsFormat, TimeDelta, TimeZone, Utc};
use grib::{Grib2SubmessageDecoder, LatLons};
use ndarray::Array2;

use super::grib2_catalog::TableCatalog;
use super::grib2_identity::{FieldIdentity, aerosol_type_code_for};
use super::grib2_types::{Grib2MessageHeader, Grib2SourceLocation};
use super::slice::{CoordinateGrid, Slice2D, SliceRequest, Validity};
use super::{
    AxisRole, DataSource, DatasetFormat, DatasetMetadata, Dimension, PointCoordinates, Variable,
};
use crate::error::{NcvError, Result};

#[derive(Debug, Clone)]
struct MessageDescriptor {
    variable: Variable,
    header: Grib2MessageHeader,
    rows: usize,
    cols: usize,
    time_label: String,
}

pub struct Grib2Source {
    path: PathBuf,
    bytes: Vec<u8>,
    parsed_bytes: Vec<u8>,
    metadata: DatasetMetadata,
    messages: Vec<MessageDescriptor>,
    decoded_cache: Mutex<std::collections::HashMap<usize, (Vec<f64>, CoordinateGrid)>>,
}

impl Grib2Source {
    pub fn open(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).map_err(|source| NcvError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::open_bytes(path, bytes)
    }

    /// Open one complete GRIB2 object already obtained by a caller.
    ///
    /// Remote adapters use this only for an exact message range, never for an
    /// implicit unbounded object download.
    pub(crate) fn open_bytes(path: &Path, bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() < 16 || &bytes[..4] != b"GRIB" {
            return Err(NcvError::UnsupportedFormat {
                path: path.to_path_buf(),
                reason: "expected a GRIB2 file beginning with the GRIB indicator".into(),
            });
        }

        let parsed_bytes = normalized_grib_bytes(&bytes);
        let parsed = catch_unwind(AssertUnwindSafe(|| grib::from_bytes(&parsed_bytes)))
            .map_err(|_| NcvError::Grib2 {
                path: path.to_path_buf(),
                reason: unsupported_template_reason(&bytes),
            })?
            .map_err(|error| NcvError::Grib2 {
                path: path.to_path_buf(),
                reason: error.to_string(),
            })?;
        if parsed.is_empty() {
            return Err(NcvError::InvalidDataset {
                path: path.to_path_buf(),
                reason: "GRIB2 file contains no data messages".into(),
            });
        }

        let mut messages = Vec::with_capacity(parsed.len());
        let mut used_names = HashSet::new();
        let catalog = TableCatalog;
        for (ordinal, (_, message)) in parsed.iter().enumerate() {
            let shape = message.grid_shape().map_err(|error| NcvError::Grib2 {
                path: path.to_path_buf(),
                reason: format!("message {ordinal}: grid definition: {error}"),
            })?;
            let (cols, rows) = shape;
            if rows == 0 || cols == 0 {
                return Err(NcvError::InvalidDataset {
                    path: path.to_path_buf(),
                    reason: format!("message {ordinal}: empty grid {cols}x{rows}"),
                });
            }
            let section1 = message.section1().map_err(|error| NcvError::Grib2 {
                path: path.to_path_buf(),
                reason: format!("message {ordinal}: identification section: {error}"),
            })?;
            let _section4 = message.section4().map_err(|error| NcvError::Grib2 {
                path: path.to_path_buf(),
                reason: format!("message {ordinal}: product section: {error}"),
            })?;
            let product_template = raw_product_template(&bytes, message.4.body.offset)
                .unwrap_or_else(|| message.prod_def().prod_tmpl_num());
            let product_definition_payload = raw_section_payload(&bytes, message.4.body)
                .unwrap_or_else(|| message.prod_def().iter().copied().collect::<Vec<_>>());
            let parameter_category = product_definition_payload.get(4).copied();
            let parameter_number = product_definition_payload.get(5).copied();
            let mut resolution =
                vec![catalog.resolve("0.0", u64::from(message.indicator().discipline))];
            let parameter_label =
                parameter_category
                    .zip(parameter_number)
                    .and_then(|(category, number)| {
                        let (category_resolution, parameter_resolution) = catalog
                            .resolve_parameter(message.indicator().discipline, category, number);
                        resolution.push(category_resolution);
                        resolution.push(parameter_resolution);
                        catalog.parameter_label(message.indicator().discipline, category, number)
                    });
            if let Some(aerosol_code) =
                aerosol_type_code_for(product_template, &product_definition_payload)
            {
                resolution.push(catalog.resolve("4.233", u64::from(aerosol_code)));
            }
            let description = parameter_label.unwrap_or_else(|| message.describe());
            // The timeline represents when the field is valid, not when the
            // GRIB message was produced. Read the forecast offset from the
            // original Section 4 payload because `parsed` uses a normalized
            // template number for newer templates such as 4.48. Calling the
            // decoder's temporal helper after that normalization can read the
            // wrong offset and produce dates centuries in the future.
            let time_label = valid_time_label(
                product_template,
                &product_definition_payload,
                &section1.payload.ref_time,
            )
            .or_else(|| {
                supported_product_template(product_template)
                    .then(|| catch_unwind(AssertUnwindSafe(|| message.temporal_info())).ok())
                    .flatten()
                    .and_then(|temporal| temporal.forecast_time_target.or(temporal.ref_time))
                    .map(|time| time.to_rfc3339_opts(SecondsFormat::Secs, true))
            })
            .unwrap_or_else(|| iso_datetime(&section1.payload.ref_time));
            let header = Grib2MessageHeader {
                location: Grib2SourceLocation {
                    path: path.display().to_string(),
                    message: ordinal,
                    offset: Some(message.0.body.offset as u64),
                },
                discipline: message.indicator().discipline,
                centre: section1.payload.centre_id,
                subcentre: section1.payload.subcentre_id,
                master_table_version: section1.payload.master_table_version,
                local_table_version: section1.payload.local_table_version,
                grid_template: message.grid_def().grid_tmpl_num(),
                product_template,
                data_representation_template: message.repr_def().repr_tmpl_num(),
                grid_points: message.grid_def().num_points() as usize,
                product_definition_payload,
                parameter_category,
                parameter_number,
                description: description.clone(),
                resolution,
            };
            let identity = FieldIdentity::from_header(&header);
            let surface_info = supported_product_template(product_template)
                .then(|| message.prod_def().fixed_surfaces())
                .flatten()
                .and_then(|(first, second)| surface_qualifier(&first, &second));
            let preferred_name = surface_info.as_ref().map_or_else(
                || identity.human_name.clone(),
                |(qualifier, _)| format!("{}_{}", identity.human_name, qualifier),
            );
            let variable_name =
                unique_variable_name(&preferred_name, &time_label, ordinal, &mut used_names);
            let long_name = surface_info.map_or(identity.display_label.clone(), |(_, level)| {
                format!("{} ({level})", identity.display_label)
            });
            messages.push(MessageDescriptor {
                variable: Variable {
                    name: variable_name,
                    dimensions: vec!["latitude".into(), "longitude".into()],
                    numeric: true,
                    units: None,
                    long_name: Some(long_name),
                    standard_name: None,
                },
                header,
                rows,
                cols,
                time_label,
            });
        }

        let max_rows = messages
            .iter()
            .map(|message| message.rows)
            .max()
            .unwrap_or(0);
        let max_cols = messages
            .iter()
            .map(|message| message.cols)
            .max()
            .unwrap_or(0);
        let metadata = DatasetMetadata {
            path: path.display().to_string(),
            format: DatasetFormat::Grib2,
            dimensions: vec![
                Dimension {
                    name: "latitude".into(),
                    length: max_rows,
                    role: AxisRole::Latitude,
                },
                Dimension {
                    name: "longitude".into(),
                    length: max_cols,
                    role: AxisRole::Longitude,
                },
            ],
            variables: messages
                .iter()
                .map(|message| message.variable.clone())
                .collect(),
        };
        Ok(Self {
            path: path.to_path_buf(),
            bytes,
            parsed_bytes,
            metadata,
            messages,
            decoded_cache: Mutex::new(std::collections::HashMap::new()),
        })
    }

    fn message(&self, variable: &str) -> Result<&MessageDescriptor> {
        self.messages
            .iter()
            .find(|message| message.variable.name == variable)
            .ok_or_else(|| NcvError::UnsupportedVariable {
                variable: variable.into(),
                reason: "GRIB2 message was not found".into(),
            })
    }

    fn decode(&self, descriptor: &MessageDescriptor) -> Result<(Vec<f64>, CoordinateGrid)> {
        let message_idx = descriptor.header.location.message;
        if let Ok(cache) = self.decoded_cache.lock()
            && let Some(cached) = cache.get(&message_idx)
        {
            return Ok(cached.clone());
        }

        let parsed = catch_unwind(AssertUnwindSafe(|| grib::from_bytes(&self.parsed_bytes)))
            .map_err(|_| NcvError::Grib2 {
                path: self.path.clone(),
                reason: unsupported_template_reason(&self.bytes),
            })?
            .map_err(|error| NcvError::Grib2 {
                path: self.path.clone(),
                reason: error.to_string(),
            })?;
        let (_, message) = parsed
            .iter()
            .nth(message_idx)
            .ok_or_else(|| NcvError::Grib2 {
                path: self.path.clone(),
                reason: format!("message {} disappeared while decoding", message_idx),
            })?;
        let mut latlons = message
            .latlons()
            .map_err(|error| NcvError::Grib2 {
                path: self.path.clone(),
                reason: format!("message coordinates: {error}"),
            })?
            .map(|(lat, lon)| (f64::from(lat), f64::from(lon)))
            .collect::<Vec<_>>();
        let decoder = Grib2SubmessageDecoder::from(message).map_err(|error| NcvError::Grib2 {
            path: self.path.clone(),
            reason: format!("message values: {error}"),
        })?;
        let mut values = decoder
            .dispatch()
            .map_err(|error| NcvError::Grib2 {
                path: self.path.clone(),
                reason: format!("message packing: {error}"),
            })?
            .map(f64::from)
            .collect::<Vec<_>>();
        let expected = descriptor
            .rows
            .checked_mul(descriptor.cols)
            .ok_or_else(|| NcvError::InvalidSlice("GRIB2 grid element count overflow".into()))?;
        if values.len() != expected || latlons.len() != expected {
            return Err(NcvError::Grib2 {
                path: self.path.clone(),
                reason: format!(
                    "message grid has {expected} points but decoded {} values and {} coordinates",
                    values.len(),
                    latlons.len()
                ),
            });
        }
        normalize_grib2_longitude_order(
            &mut values,
            &mut latlons,
            descriptor.rows,
            descriptor.cols,
        );
        let latitude = Array2::from_shape_vec(
            (descriptor.rows, descriptor.cols),
            latlons.iter().map(|(lat, _)| *lat).collect(),
        )
        .map_err(|error| NcvError::Grib2 {
            path: self.path.clone(),
            reason: format!("latitude grid: {error}"),
        })?;
        let longitude = Array2::from_shape_vec(
            (descriptor.rows, descriptor.cols),
            latlons.iter().map(|(_, lon)| *lon).collect(),
        )
        .map_err(|error| NcvError::Grib2 {
            path: self.path.clone(),
            reason: format!("longitude grid: {error}"),
        })?;
        let result = (
            values,
            CoordinateGrid {
                latitude: Some(latitude),
                longitude: Some(longitude),
            },
        );
        if let Ok(mut cache) = self.decoded_cache.lock() {
            cache.insert(message_idx, result.clone());
        }
        Ok(result)
    }
}

/// Convert GRIB2 fields using the common global `0..360` longitude convention
/// to the viewer's `-180..180` convention.  The coordinate conversion alone
/// is not sufficient: the data columns must be rotated with their coordinates
/// or the field is displayed half a world away from the map backdrop.
fn normalize_grib2_longitude_order(
    values: &mut [f64],
    latlons: &mut [(f64, f64)],
    rows: usize,
    cols: usize,
) {
    if rows == 0
        || cols == 0
        || values.len() != rows.saturating_mul(cols)
        || latlons.len() != values.len()
    {
        return;
    }

    let raw_longitudes = latlons
        .iter()
        .map(|(_, longitude)| *longitude)
        .filter(|longitude| longitude.is_finite())
        .collect::<Vec<_>>();
    let Some(min_longitude) = raw_longitudes.iter().copied().reduce(f64::min) else {
        return;
    };
    let Some(max_longitude) = raw_longitudes.iter().copied().reduce(f64::max) else {
        return;
    };

    // `grib` normalizes some 0..360 grids to signed values without rotating
    // their source order, yielding `0..180,-180..0`. Detect that dateline
    // wrap as well as an explicitly non-negative 0..360 range.
    let source_wraps_at_dateline = latlons
        .chunks(cols)
        .any(|row| row.windows(2).any(|pair| pair[0].1 - pair[1].1 > 180.0));
    let explicit_zero_to_360 = min_longitude >= -1.0e-6 && max_longitude > 180.0 + 1.0e-6;
    if !source_wraps_at_dateline && !explicit_zero_to_360 {
        return;
    }

    let mut reordered_values = vec![0.0; values.len()];
    let mut reordered_latlons = vec![(0.0, 0.0); latlons.len()];

    let first_row_order = {
        let mut order = (0..cols).collect::<Vec<_>>();
        order.sort_by(|&left, &right| {
            normalize_grib2_longitude(latlons[left].1)
                .total_cmp(&normalize_grib2_longitude(latlons[right].1))
                .then_with(|| left.cmp(&right))
        });
        order
    };

    for row in 0..rows {
        let row_start = row * cols;
        let local_order;
        let row_order = if row == 0
            || (0..cols).all(|c| (latlons[row_start + c].1 - latlons[c].1).abs() < 1e-6)
        {
            &first_row_order[..]
        } else {
            let mut order = (0..cols).collect::<Vec<_>>();
            order.sort_by(|&left, &right| {
                normalize_grib2_longitude(latlons[row_start + left].1)
                    .total_cmp(&normalize_grib2_longitude(latlons[row_start + right].1))
                    .then_with(|| left.cmp(&right))
            });
            local_order = order;
            &local_order[..]
        };

        for (new_col, &old_col) in row_order.iter().enumerate() {
            let old_index = row_start + old_col;
            let new_index = row_start + new_col;
            let (latitude, longitude) = latlons[old_index];
            reordered_values[new_index] = values[old_index];
            reordered_latlons[new_index] = (latitude, normalize_grib2_longitude(longitude));
        }
    }
    values.copy_from_slice(&reordered_values);
    latlons.copy_from_slice(&reordered_latlons);
}

fn normalize_grib2_longitude(longitude: f64) -> f64 {
    (longitude + 180.0).rem_euclid(360.0) - 180.0
}

/// `grib` 0.18.4 only models a subset of Product Definition Template 4.x and
/// panics when its generated enum sees a valid newer template such as NOAA's
/// 4.48. The common section/grid/data layout is still decodable, so normalize
/// only the unsupported template discriminant for the third-party parser.
/// The original Section 4 bytes are retained separately for metadata and
/// identity generation.
pub(crate) fn normalized_grib_bytes(source: &[u8]) -> Vec<u8> {
    let mut normalized = source.to_vec();
    let mut message_offset = 0usize;
    while message_offset
        .checked_add(16)
        .is_some_and(|end| end <= source.len())
        && &source[message_offset..message_offset + 4] == b"GRIB"
    {
        let total_length = u64::from_be_bytes(
            source[message_offset + 8..message_offset + 16]
                .try_into()
                .unwrap(),
        ) as usize;
        let Some(message_end) = message_offset.checked_add(total_length) else {
            break;
        };
        if total_length < 20 || message_end > source.len() {
            break;
        }
        let mut section_offset = message_offset + 16;
        while section_offset + 5 <= message_end {
            let section_length = u32::from_be_bytes(
                source[section_offset..section_offset + 4]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let section_number = source[section_offset + 4];
            if section_length < 5 || section_offset + section_length > message_end {
                break;
            }
            if section_number == 4 && section_length >= 9 {
                let template = u16::from_be_bytes(
                    source[section_offset + 7..section_offset + 9]
                        .try_into()
                        .unwrap(),
                );
                if !matches!(template, 0 | 1 | 2 | 5 | 6) {
                    normalized[section_offset + 7..section_offset + 9]
                        .copy_from_slice(&0_u16.to_be_bytes());
                }
            }
            section_offset += section_length;
            if section_number == 8 {
                break;
            }
        }
        message_offset = message_end;
    }
    normalized
}

pub(crate) fn raw_section_payload(source: &[u8], section: &grib::SectionInfo) -> Option<Vec<u8>> {
    let start = section.offset.checked_add(5)?;
    let end = section.offset.checked_add(section.size)?;
    (end <= source.len() && start <= end).then(|| source[start..end].to_vec())
}

pub(crate) fn raw_product_template(source: &[u8], section_offset: usize) -> Option<u16> {
    let start = section_offset.checked_add(7)?;
    let bytes = source.get(start..start + 2)?;
    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn unsupported_template_reason(source: &[u8]) -> String {
    let template = source
        .windows(9)
        .find(|window| window[4] == 4)
        .map(|window| u16::from_be_bytes([window[7], window[8]]));
    match template {
        Some(template) => format!(
            "GRIB2 product definition template 4.{template} is valid but unsupported by the bundled decoder"
        ),
        None => "bundled GRIB2 decoder rejected the message without a recoverable template number"
            .into(),
    }
}

fn supported_product_template(template: u16) -> bool {
    matches!(template, 0 | 1 | 2 | 5 | 6)
}

fn valid_time_label(
    product_template: u16,
    payload: &[u8],
    reference_time: &grib::def::grib2::template::param_set::DateTime,
) -> Option<String> {
    let forecast_offset = match product_template {
        0..=15 | 32..=34 | 51 | 60..=61 | 86..=87 | 91 | 1000..=1101 => 8,
        40..=43 => 10,
        44..=47 | 85 => 21,
        48..=49 => 32,
        55..=56 | 59 | 62..=63 => 14,
        70..=73 => 13,
        76..=79 => 11,
        80..=81 => 33,
        82..=84 => 22,
        88 => 26,
        _ => return None,
    };
    let unit = *payload.get(4 + forecast_offset)?;
    let value = u32::from_be_bytes(
        payload
            .get(5 + forecast_offset..9 + forecast_offset)?
            .try_into()
            .ok()?,
    );
    let reference = Utc
        .with_ymd_and_hms(
            i32::from(reference_time.year),
            u32::from(reference_time.month),
            u32::from(reference_time.day),
            u32::from(reference_time.hour),
            u32::from(reference_time.minute),
            u32::from(reference_time.second),
        )
        .single()?;
    let value = i64::from(value);
    let delta = match unit {
        0 => TimeDelta::try_minutes(value),
        1 => TimeDelta::try_hours(value),
        2 => TimeDelta::try_days(value),
        10 => TimeDelta::try_hours(value.checked_mul(3)?),
        11 => TimeDelta::try_hours(value.checked_mul(6)?),
        12 => TimeDelta::try_hours(value.checked_mul(12)?),
        13 => TimeDelta::try_seconds(value),
        _ => None,
    }?;
    reference
        .checked_add_signed(delta)
        .map(|time| time.to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn unique_variable_name(
    preferred: &str,
    time_label: &str,
    ordinal: usize,
    used_names: &mut HashSet<String>,
) -> String {
    if used_names.insert(preferred.to_owned()) {
        return preferred.to_owned();
    }

    let time_suffix = slug_component(time_label);
    if !time_suffix.is_empty() {
        let candidate = format!("{preferred}_valid_{time_suffix}");
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
    }

    let mut variant = ordinal + 1;
    loop {
        let candidate = format!("{preferred}_variant_{variant:03}");
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
        variant += 1;
    }
}

fn surface_qualifier(
    first: &grib::FixedSurface,
    second: &grib::FixedSurface,
) -> Option<(String, String)> {
    let first_value = first.value();
    if !first_value.is_finite() {
        return None;
    }
    let (label, _, _) = first.describe();
    let slugged_label = slug_component(&label);
    let machine_label = slugged_label
        .strip_suffix("_surface")
        .unwrap_or(&slugged_label)
        .to_owned();
    let unit = first.unit().map(slug_component).unwrap_or_default();
    let display_label = label
        .strip_suffix(" surface")
        .unwrap_or(&label)
        .to_ascii_lowercase();
    let display_unit = first.unit().unwrap_or_default();
    let mut machine = format!("{}_{}{}", machine_label, compact_number(first_value), unit);
    let mut display = format!(
        "{} {}{}",
        display_label,
        compact_number(first_value),
        if display_unit.is_empty() {
            String::new()
        } else {
            format!(" {display_unit}")
        }
    );
    let second_value = second.value();
    if second_value.is_finite()
        && second.surface_type == first.surface_type
        && (second_value - first_value).abs() > f64::EPSILON
    {
        machine.push('-');
        machine.push_str(&compact_number(second_value));
        machine.push_str(&unit);
        display.push('–');
        display.push_str(&compact_number(second_value));
        if !display_unit.is_empty() {
            display.push(' ');
            display.push_str(display_unit);
        }
    }
    Some((machine, display))
}

fn compact_number(value: f64) -> String {
    let mut result = format!("{value:.6}");
    while result.ends_with('0') {
        result.pop();
    }
    if result.ends_with('.') {
        result.pop();
    }
    result
}

fn slug_component(value: &str) -> String {
    let mut result = String::new();
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            result.push(character.to_ascii_lowercase());
        } else if character == '.' {
            result.push('p');
        } else if !result.ends_with('_') {
            result.push('_');
        }
    }
    result.trim_matches('_').to_owned()
}

fn iso_datetime(value: &grib::def::grib2::template::param_set::DateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        value.year, value.month, value.day, value.hour, value.minute, value.second
    )
}

impl DataSource for Grib2Source {
    fn metadata(&self) -> &DatasetMetadata {
        &self.metadata
    }

    fn read_slice(&self, request: &SliceRequest) -> Result<Slice2D> {
        self.read_slice_on_axes(request, None, None, &[])
    }

    fn read_slice_on_axes(
        &self,
        request: &SliceRequest,
        row_axis: Option<&str>,
        col_axis: Option<&str>,
        _fixed_axes: &[(String, usize)],
    ) -> Result<Slice2D> {
        if row_axis.is_some_and(|axis| axis != "latitude")
            || col_axis.is_some_and(|axis| axis != "longitude")
        {
            return Err(NcvError::UnsupportedVariable {
                variable: request.variable.clone(),
                reason: "GRIB2 MVP supports latitude rows and longitude columns".into(),
            });
        }
        let descriptor = self.message(&request.variable)?.clone();
        let (values, coordinates) = self.decode(&descriptor)?;
        if request.bounds.row_end > descriptor.rows || request.bounds.col_end > descriptor.cols {
            return Err(NcvError::InvalidSlice(
                "slice bounds exceed GRIB2 grid shape".into(),
            ));
        }
        let out_shape = request.bounds.shape();
        let mut selected = Array2::zeros(out_shape);
        let mut validity = Array2::from_elem(out_shape, Validity::Finite);
        for row in request.bounds.row_start..request.bounds.row_end {
            let row_offset = row * descriptor.cols;
            let local_row = row - request.bounds.row_start;
            for col in request.bounds.col_start..request.bounds.col_end {
                let value = values[row_offset + col];
                let local = (local_row, col - request.bounds.col_start);
                selected[local] = value;
                validity[local] = if value.is_nan() {
                    Validity::Missing
                } else if value == f64::INFINITY {
                    Validity::PosInf
                } else if value == f64::NEG_INFINITY {
                    Validity::NegInf
                } else {
                    Validity::Finite
                };
            }
        }
        let coordinate_slice = CoordinateGrid {
            latitude: coordinates.latitude.map(|grid| {
                grid.slice(ndarray::s![
                    request.bounds.row_start..request.bounds.row_end,
                    request.bounds.col_start..request.bounds.col_end
                ])
                .to_owned()
            }),
            longitude: coordinates.longitude.map(|grid| {
                grid.slice(ndarray::s![
                    request.bounds.row_start..request.bounds.row_end,
                    request.bounds.col_start..request.bounds.col_end
                ])
                .to_owned()
            }),
        };
        Ok(Slice2D::new(selected, validity, request.bounds)?.with_coordinates(coordinate_slice))
    }

    fn time_label(&self, index: usize) -> Option<String> {
        self.messages
            .get(index)
            .map(|message| message.time_label.clone())
    }

    fn time_label_for_variable(&self, variable: &str, _index: usize) -> Option<String> {
        self.message(variable)
            .ok()
            .map(|message| message.time_label.clone())
    }

    fn point_coordinates(&self, variable: &str, row: usize, col: usize) -> PointCoordinates {
        let Ok(descriptor) = self.message(variable) else {
            return PointCoordinates::default();
        };
        let Ok((_, coordinates)) = self.decode(descriptor) else {
            return PointCoordinates::default();
        };
        PointCoordinates {
            latitude: coordinates
                .latitude
                .and_then(|grid| grid.get((row, col)).copied()),
            longitude: coordinates
                .longitude
                .and_then(|grid| grid.get((row, col)).copied()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        iso_datetime, normalize_grib2_longitude_order, normalized_grib_bytes, valid_time_label,
    };

    #[test]
    fn formats_grib_reference_time_as_iso8601() {
        let value = grib::def::grib2::template::param_set::DateTime::new(2023, 7, 9, 4, 5, 6);
        assert_eq!(iso_datetime(&value), "2023-07-09T04:05:06Z");
    }

    #[test]
    fn calculates_valid_time_for_aerosol_template() {
        let reference = grib::def::grib2::template::param_set::DateTime::new(2026, 9, 7, 12, 0, 0);
        let mut payload = vec![0_u8; 41];
        payload[36] = 1; // hours
        payload[37..41].copy_from_slice(&3_u32.to_be_bytes());
        assert_eq!(
            valid_time_label(48, &payload, &reference).as_deref(),
            Some("2026-09-07T15:00:00Z")
        );
    }

    #[test]
    fn normalizes_unknown_product_template_without_changing_section_bounds() {
        let mut source = vec![0_u8; 25];
        source[..4].copy_from_slice(b"GRIB");
        source[8..16].copy_from_slice(&25_u64.to_be_bytes());
        source[16..20].copy_from_slice(&9_u32.to_be_bytes());
        source[20] = 4;
        source[23..25].copy_from_slice(&48_u16.to_be_bytes());

        let normalized = normalized_grib_bytes(&source);
        assert_eq!(&normalized[23..25], &[0, 0]);
        assert_eq!(&normalized[..23], &source[..23]);
        assert_eq!(&normalized[16..20], &source[16..20]);
    }

    #[test]
    fn preserves_supported_product_templates() {
        let mut source = vec![0_u8; 25];
        source[..4].copy_from_slice(b"GRIB");
        source[8..16].copy_from_slice(&25_u64.to_be_bytes());
        source[16..20].copy_from_slice(&9_u32.to_be_bytes());
        source[20] = 4;
        source[23..25].copy_from_slice(&0_u16.to_be_bytes());

        assert_eq!(normalized_grib_bytes(&source), source);
    }

    #[test]
    fn rotates_zero_to_360_columns_into_signed_longitude_order() {
        let mut values = vec![0.0, 1.0, 2.0, 3.0];
        let mut latlons = vec![(0.0, 0.0), (0.0, 90.0), (0.0, 180.0), (0.0, 270.0)];

        normalize_grib2_longitude_order(&mut values, &mut latlons, 1, 4);

        assert_eq!(values, [2.0, 3.0, 0.0, 1.0]);
        assert_eq!(
            latlons,
            [(0.0, -180.0), (0.0, -90.0), (0.0, 0.0), (0.0, 90.0)]
        );
    }

    #[test]
    fn rotates_signed_coordinates_when_source_order_wraps_at_dateline() {
        let mut values = vec![0.0, 1.0, 2.0, 3.0];
        let mut latlons = vec![(0.0, 0.0), (0.0, 90.0), (0.0, -180.0), (0.0, -90.0)];

        normalize_grib2_longitude_order(&mut values, &mut latlons, 1, 4);

        assert_eq!(values, [2.0, 3.0, 0.0, 1.0]);
        assert_eq!(
            latlons,
            [(0.0, -180.0), (0.0, -90.0), (0.0, 0.0), (0.0, 90.0)]
        );
    }
}
