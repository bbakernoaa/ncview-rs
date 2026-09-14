//! Range-backed GRIB2 access using exact message objects.

use std::{
    collections::HashSet,
    path::Path,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

use bytes::Bytes;

use super::{
    AxisRole, DataSource, DatasetFormat, DatasetMetadata, Dimension, PointCoordinates, Variable,
    grib2::Grib2Source,
    grib2_index::{IndexRecord, parse_index},
    slice::{Slice2D, SliceRequest},
};
use crate::{
    error::{NcvError, Result},
    storage::{
        StorageRuntime,
        location::SourceLocation,
        object_store::{ByteRange, ObjectIdentity, RemoteStore},
    },
};

const MAX_INDEX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_UNINDEXED_OBJECT_BYTES: u64 = 64 * 1024 * 1024;

struct RemoteMessage {
    public_name: String,
    decoder_name: Option<String>,
    range: Option<ByteRange>,
    source: Mutex<Option<Arc<Grib2Source>>>,
    indexed_time_label: Option<String>,
}

pub struct RemoteGrib2Source {
    source: SourceLocation,
    metadata: DatasetMetadata,
    messages: Vec<RemoteMessage>,
    identity: String,
    remote: Option<Arc<RemoteStore>>,
    object_identity: Option<ObjectIdentity>,
    runtime: Option<Arc<StorageRuntime>>,
}

pub fn open_remote(
    remote: Arc<RemoteStore>,
    identity: ObjectIdentity,
    runtime: Arc<StorageRuntime>,
) -> Result<Box<dyn DataSource>> {
    open_remote_with_progress(remote, identity, runtime, &|_| true)
}

pub fn open_remote_with_progress(
    remote: Arc<RemoteStore>,
    identity: ObjectIdentity,
    runtime: Arc<StorageRuntime>,
    progress: &dyn Fn(&str) -> bool,
) -> Result<Box<dyn DataSource>> {
    report(progress, "checking colocated GRIB2 index")?;
    let records = read_optional_index(&remote, &identity, &runtime, progress)?;
    if let Some(records) = records {
        return build_indexed_source(remote, &identity, runtime, records, progress);
    }
    if identity.size() > MAX_UNINDEXED_OBJECT_BYTES {
        return Err(NcvError::remote_failure(
            remote.source().clone(),
            "GRIB2 discovery",
            "no colocated .idx sidecar was found and the object exceeds the bounded scan limit",
        ));
    }
    report(progress, "fetching bounded GRIB2 object")?;
    let range = ByteRange::new(0, identity.size(), identity.size())?;
    let remote_for_read = Arc::clone(&remote);
    let identity_for_read = identity.clone();
    let objects = vec![receive(&runtime, async move {
        remote_for_read.read_range(&identity_for_read, range).await
    })??];
    report(progress, "decoding GRIB2 metadata")?;
    build_source(remote.source(), identity.cache_token(), objects)
}

fn build_indexed_source(
    remote: Arc<RemoteStore>,
    identity: &ObjectIdentity,
    runtime: Arc<StorageRuntime>,
    records: Vec<IndexRecord>,
    progress: &dyn Fn(&str) -> bool,
) -> Result<Box<dyn DataSource>> {
    let message_ranges = records
        .iter()
        .map(|record| {
            let end = record
                .offset
                .checked_add(record.length)
                .ok_or_else(|| NcvError::InvalidRange("GRIB2 message range overflow".into()))?;
            ByteRange::new(record.offset, end, identity.size())
        })
        .collect::<Result<Vec<_>>>()?;
    let first_range = message_ranges.first().copied().ok_or_else(|| {
        NcvError::remote_failure(
            remote.source().clone(),
            "GRIB2 discovery",
            "the index contains no message records",
        )
    })?;
    report(
        progress,
        &format!(
            "fetching first GRIB2 message metadata ({} bytes)",
            first_range.len()
        ),
    )?;
    let first_bytes = fetch_message(&remote, identity, &runtime, first_range)?;
    let first_decoder = Grib2Source::open_bytes(
        Path::new(remote.source().safe_display()),
        first_bytes.to_vec(),
    )
    .map_err(|error| {
        NcvError::remote_failure(remote.source().clone(), "GRIB2 decode", &error.to_string())
    })?;
    let prototype = first_decoder
        .metadata()
        .variables
        .first()
        .cloned()
        .ok_or_else(|| {
            NcvError::remote_failure(
                remote.source().clone(),
                "GRIB2 metadata",
                "the first indexed message contains no data variable",
            )
        })?;
    let first_time_label = first_decoder.time_label_for_variable(&prototype.name, 0);
    let rows = first_decoder
        .metadata()
        .dimensions
        .iter()
        .find(|dimension| dimension.role == AxisRole::Latitude)
        .map_or(0, |dimension| dimension.length);
    let cols = first_decoder
        .metadata()
        .dimensions
        .iter()
        .find(|dimension| dimension.role == AxisRole::Longitude)
        .map_or(0, |dimension| dimension.length);
    let first_decoder = Arc::new(first_decoder);
    let mut used_names = HashSet::new();
    let mut variables = Vec::with_capacity(records.len());
    let mut messages = Vec::with_capacity(records.len());
    for (position, (record, range)) in records.iter().zip(message_ranges).enumerate() {
        let mut variable = if position == 0 {
            prototype.clone()
        } else {
            variable_from_index(record, &prototype)
        };
        let decoder_name = variable.name.clone();
        let public_name = unique_public_name(&decoder_name, position, &mut used_names);
        variable.name = public_name.clone();
        variables.push(variable);
        messages.push(RemoteMessage {
            public_name,
            decoder_name: (position == 0).then(|| decoder_name.clone()),
            range: Some(range),
            source: Mutex::new(if position == 0 {
                Some(Arc::clone(&first_decoder))
            } else {
                None
            }),
            indexed_time_label: if position == 0 {
                first_time_label.clone()
            } else {
                index_time_label(record)
            },
        });
    }
    Ok(Box::new(RemoteGrib2Source {
        source: remote.source().clone(),
        metadata: DatasetMetadata {
            path: remote.source().safe_display().to_owned(),
            format: DatasetFormat::Grib2,
            dimensions: vec![
                Dimension {
                    name: "latitude".into(),
                    length: rows,
                    role: AxisRole::Latitude,
                },
                Dimension {
                    name: "longitude".into(),
                    length: cols,
                    role: AxisRole::Longitude,
                },
            ],
            variables,
        },
        messages,
        identity: identity.cache_token().to_owned(),
        remote: Some(remote),
        object_identity: Some(identity.clone()),
        runtime: Some(runtime),
    }))
}

fn read_optional_index(
    remote: &Arc<RemoteStore>,
    identity: &ObjectIdentity,
    runtime: &Arc<StorageRuntime>,
    progress: &dyn Fn(&str) -> bool,
) -> Result<Option<Vec<IndexRecord>>> {
    let sidecar_source = remote.source().with_object_suffix(".idx")?;
    let sidecar = Arc::new(remote.with_source(sidecar_source.clone()));
    let sidecar_identity = match receive(runtime, {
        let sidecar = Arc::clone(&sidecar);
        async move { sidecar.head().await }
    }) {
        Ok(Ok(identity)) => identity,
        Ok(Err(error)) if is_not_found(&error) => {
            return Ok(None);
        }
        Ok(Err(error)) | Err(error) => return Err(error),
    };
    report(progress, "fetching GRIB2 index")?;
    if sidecar_identity.size() > MAX_INDEX_BYTES {
        return Err(NcvError::remote_failure(
            sidecar_source,
            "GRIB2 index",
            "colocated .idx sidecar exceeds the bounded index limit",
        ));
    }
    let range = ByteRange::new(0, sidecar_identity.size(), sidecar_identity.size())?;
    let bytes = receive(runtime, {
        let sidecar = Arc::clone(&sidecar);
        let sidecar_identity_for_read = sidecar_identity.clone();
        async move { sidecar.read_range(&sidecar_identity_for_read, range).await }
    })??;
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        NcvError::remote_failure(
            remote.source().clone(),
            "GRIB2 index",
            "colocated .idx sidecar is not UTF-8",
        )
    })?;
    parse_index(
        Path::new(remote.source().safe_display()),
        text,
        identity.size(),
    )
    .map_err(|error| {
        NcvError::remote_failure(remote.source().clone(), "GRIB2 index", &error.to_string())
    })
    .map(Some)
}

fn is_not_found(error: &NcvError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    ["not found", "notfound", "no such file", "no data in memory"]
        .iter()
        .any(|marker| message.contains(marker))
}

fn report(progress: &dyn Fn(&str) -> bool, message: &str) -> Result<()> {
    if progress(message) {
        Ok(())
    } else {
        Err(NcvError::WorkerStopped)
    }
}

fn build_source(
    source: &SourceLocation,
    identity: &str,
    objects: Vec<Bytes>,
) -> Result<Box<dyn DataSource>> {
    let mut messages = Vec::new();
    let mut variables = Vec::new();
    let mut used_names = HashSet::new();
    let mut max_rows = 0usize;
    let mut max_cols = 0usize;
    for bytes in objects {
        let decoder = Arc::new(
            Grib2Source::open_bytes(Path::new(source.safe_display()), bytes.to_vec()).map_err(
                |error| {
                    NcvError::remote_failure(source.clone(), "GRIB2 decode", &error.to_string())
                },
            )?,
        );
        max_rows = max_rows.max(
            decoder
                .metadata()
                .dimensions
                .iter()
                .find(|dimension| dimension.role == AxisRole::Latitude)
                .map_or(0, |dimension| dimension.length),
        );
        max_cols = max_cols.max(
            decoder
                .metadata()
                .dimensions
                .iter()
                .find(|dimension| dimension.role == AxisRole::Longitude)
                .map_or(0, |dimension| dimension.length),
        );
        for variable in &decoder.metadata().variables {
            let decoder_name = variable.name.clone();
            let public_name = unique_public_name(&decoder_name, messages.len(), &mut used_names);
            let mut public_variable = variable.clone();
            public_variable.name = public_name.clone();
            variables.push(public_variable);
            messages.push(RemoteMessage {
                public_name,
                decoder_name: Some(decoder_name),
                range: None,
                source: Mutex::new(Some(Arc::clone(&decoder))),
                indexed_time_label: decoder.time_label(messages.len()),
            });
        }
    }
    if messages.is_empty() {
        return Err(NcvError::InvalidDataset {
            path: source.safe_display().into(),
            reason: "GRIB2 object contains no data messages".into(),
        });
    }
    Ok(Box::new(RemoteGrib2Source {
        source: source.clone(),
        metadata: DatasetMetadata {
            path: source.safe_display().to_owned(),
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
            variables,
        },
        messages,
        identity: identity.to_owned(),
        remote: None,
        object_identity: None,
        runtime: None,
    }))
}

fn fetch_message(
    remote: &Arc<RemoteStore>,
    identity: &ObjectIdentity,
    runtime: &Arc<StorageRuntime>,
    range: ByteRange,
) -> Result<Bytes> {
    fetch_message_cancelled(remote, identity, runtime, range, None)
}

fn fetch_message_cancelled(
    remote: &Arc<RemoteStore>,
    identity: &ObjectIdentity,
    runtime: &Arc<StorageRuntime>,
    range: ByteRange,
    cancelled: Option<Arc<AtomicBool>>,
) -> Result<Bytes> {
    let remote = Arc::clone(remote);
    let identity = identity.clone();
    let future = async move { remote.read_range(&identity, range).await };
    match cancelled {
        Some(cancelled) => Ok(runtime
            .submit_cancellable(future, cancelled)?
            .recv()
            .map_err(|_| NcvError::WorkerStopped)?
            .ok_or(NcvError::WorkerStopped)??),
        None => receive(runtime, future)?,
    }
}

fn unique_public_name(preferred: &str, ordinal: usize, used_names: &mut HashSet<String>) -> String {
    if used_names.insert(preferred.to_owned()) {
        return preferred.to_owned();
    }
    let mut name = format!("{preferred}_message_{:04}", ordinal + 1);
    while !used_names.insert(name.clone()) {
        name.push('_');
    }
    name
}

fn variable_from_index(record: &IndexRecord, prototype: &Variable) -> Variable {
    let name = index_variable_name(record).unwrap_or_else(|| prototype.name.clone());
    let mut variable = prototype.clone();
    variable.name = name.clone();
    variable.long_name = Some(name);
    variable
}

fn index_variable_name(record: &IndexRecord) -> Option<String> {
    let short_name = record.fields.get(3)?.as_str();
    let parameter = match short_name.to_ascii_uppercase().as_str() {
        "AOTK" => "aerosol_optical_thickness",
        "ASYSFK" => "asymmetry_factor",
        "SSALBK" => "single_scattering_albedo",
        "SCTAOTK" => "scattering_aerosol_optical_thickness",
        value => &slug_component(value),
    };
    let species = record
        .fields
        .iter()
        .find_map(|field| field.strip_prefix("aerosol="))
        .map(slug_component);
    let size = record
        .fields
        .iter()
        .find_map(|field| field.strip_prefix("aerosol_size "))
        .and_then(|value| format_index_interval(value, 1e6, "um"));
    let wavelength = record
        .fields
        .iter()
        .find_map(|field| field.strip_prefix("aerosol_wavelength "))
        .and_then(|value| format_index_interval(value, 1e9, "nm"));
    let mut parts = Vec::new();
    if let Some(species) = species {
        parts.push(species);
    }
    if let Some(size) = size {
        parts.push(size);
    }
    parts.push(parameter.to_owned());
    if let Some(wavelength) = wavelength {
        parts.push(wavelength);
    }
    Some(parts.join("_"))
}

fn format_index_interval(value: &str, factor: f64, unit: &str) -> Option<String> {
    let values = value.split(',').map(str::trim).collect::<Vec<_>>();
    if values.len() == 1 {
        let (operator, number) = parse_index_quantity(values[0])?;
        return Some(format!(
            "{}{}",
            interval_prefix(operator),
            format_index_quantity(number, factor, unit)
        ));
    }
    if values.len() != 2 {
        return None;
    }
    let (first_operator, first) = parse_index_quantity(values[0])?;
    let (second_operator, second) = parse_index_quantity(values[1])?;
    if matches!(first_operator, ">=" | ">") && matches!(second_operator, "<=" | "<") {
        return Some(format!(
            "{}-{}",
            format_index_quantity(first, factor, unit),
            format_index_quantity(second, factor, unit)
        ));
    }
    Some(format!(
        "{}{}-{}{}",
        interval_prefix(first_operator),
        format_index_quantity(first, factor, unit),
        interval_prefix(second_operator),
        format_index_quantity(second, factor, unit)
    ))
}

fn parse_index_quantity(value: &str) -> Option<(&str, f64)> {
    [">=", "<=", ">", "<", "="].iter().find_map(|operator| {
        value
            .strip_prefix(operator)
            .and_then(|number| number.trim().parse().ok().map(|number| (*operator, number)))
    })
}

fn interval_prefix(operator: &str) -> &'static str {
    match operator {
        "<" | "<=" => "lt",
        ">" | ">=" => "gt",
        "=" => "eq",
        _ => "",
    }
}

fn format_index_quantity(value: f64, factor: f64, unit: &str) -> String {
    let scaled = value * factor;
    let mut result = format!("{scaled:.6}");
    while result.ends_with('0') {
        result.pop();
    }
    if result.ends_with('.') {
        result.pop();
    }
    format!("{result}{unit}")
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

fn index_time_label(record: &IndexRecord) -> Option<String> {
    let date = record
        .fields
        .iter()
        .find_map(|field| field.strip_prefix("d="))?;
    if date.len() < 10
        || !date[..10]
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return None;
    }
    Some(format!(
        "{}-{}-{}T{}:00:00Z",
        &date[0..4],
        &date[4..6],
        &date[6..8],
        &date[8..10]
    ))
}

fn receive<T>(
    runtime: &StorageRuntime,
    future: impl std::future::Future<Output = T> + Send + 'static,
) -> Result<T>
where
    T: Send + 'static,
{
    runtime
        .submit(future)?
        .recv()
        .map_err(|_| NcvError::WorkerStopped)
}

impl RemoteGrib2Source {
    fn with_decoder_cancelled<T>(
        &self,
        index: usize,
        operation: &str,
        cancelled: Option<Arc<AtomicBool>>,
        callback: impl FnOnce(&Grib2Source, &RemoteMessage) -> Result<T>,
    ) -> Result<T> {
        let message = self.messages.get(index).ok_or_else(|| {
            NcvError::remote_failure(self.source.clone(), operation, "message index is invalid")
        })?;
        let needs_fetch = message
            .source
            .lock()
            .map_err(|_| {
                NcvError::remote_failure(
                    self.source.clone(),
                    operation,
                    "message cache is poisoned",
                )
            })?
            .is_none();
        if needs_fetch {
            let remote = self.remote.as_ref().ok_or_else(|| {
                NcvError::remote_failure(
                    self.source.clone(),
                    operation,
                    "message bytes are unavailable after bounded opening",
                )
            })?;
            let identity = self.object_identity.as_ref().ok_or_else(|| {
                NcvError::remote_failure(
                    self.source.clone(),
                    operation,
                    "object identity is unavailable",
                )
            })?;
            let runtime = self.runtime.as_ref().ok_or_else(|| {
                NcvError::remote_failure(
                    self.source.clone(),
                    operation,
                    "storage runtime is unavailable",
                )
            })?;
            let range = message.range.ok_or_else(|| {
                NcvError::remote_failure(
                    self.source.clone(),
                    operation,
                    "message byte range is unavailable",
                )
            })?;
            let bytes = fetch_message_cancelled(remote, identity, runtime, range, cancelled)?;
            let decoder = Arc::new(
                Grib2Source::open_bytes(Path::new(self.source.safe_display()), bytes.to_vec())
                    .map_err(|error| {
                        NcvError::remote_failure(
                            self.source.clone(),
                            "GRIB2 decode",
                            &error.to_string(),
                        )
                    })?,
            );
            let mut cached = message.source.lock().map_err(|_| {
                NcvError::remote_failure(
                    self.source.clone(),
                    operation,
                    "message cache is poisoned",
                )
            })?;
            if cached.is_none() {
                *cached = Some(decoder);
            }
        }
        let cached = message.source.lock().map_err(|_| {
            NcvError::remote_failure(self.source.clone(), operation, "message cache is poisoned")
        })?;
        let decoder = cached.as_ref().ok_or_else(|| {
            NcvError::remote_failure(self.source.clone(), operation, "message was not decoded")
        })?;
        callback(decoder, message)
    }

    fn cached_decoder(&self, index: usize) -> Option<Arc<Grib2Source>> {
        self.messages
            .get(index)
            .and_then(|message| message.source.lock().ok()?.as_ref().map(Arc::clone))
    }
}

impl RemoteGrib2Source {
    fn read_slice_with_cancelled(
        &self,
        request: &SliceRequest,
        row_axis: Option<&str>,
        col_axis: Option<&str>,
        fixed_axes: &[(String, usize)],
        cancelled: Option<Arc<AtomicBool>>,
    ) -> Result<Slice2D> {
        let message_index = self
            .messages
            .iter()
            .position(|message| message.public_name == request.variable)
            .ok_or_else(|| NcvError::UnsupportedVariable {
                variable: request.variable.clone(),
                reason: "remote GRIB2 message was not found".into(),
            })?;
        self.with_decoder_cancelled(
            message_index,
            "GRIB2 slice",
            cancelled,
            |decoder, message| {
                let mut request = request.clone();
                request.variable = message
                    .decoder_name
                    .clone()
                    .or_else(|| {
                        decoder
                            .metadata()
                            .variables
                            .first()
                            .map(|variable| variable.name.clone())
                    })
                    .ok_or_else(|| {
                        NcvError::remote_failure(
                            self.source.clone(),
                            "GRIB2 slice",
                            "decoded message has no data variable",
                        )
                    })?;
                decoder
                    .read_slice_on_axes(&request, row_axis, col_axis, fixed_axes)
                    .map_err(|error| {
                        NcvError::remote_failure(
                            self.source.clone(),
                            "GRIB2 slice",
                            &error.to_string(),
                        )
                    })
            },
        )
    }
}

impl DataSource for RemoteGrib2Source {
    fn metadata(&self) -> &DatasetMetadata {
        &self.metadata
    }

    fn is_remote(&self) -> bool {
        true
    }

    fn source_identity(&self) -> Option<&str> {
        Some(&self.identity)
    }

    fn read_slice(&self, request: &SliceRequest) -> Result<Slice2D> {
        self.read_slice_on_axes(request, None, None, &[])
    }

    fn read_slice_on_axes(
        &self,
        request: &SliceRequest,
        row_axis: Option<&str>,
        col_axis: Option<&str>,
        fixed_axes: &[(String, usize)],
    ) -> Result<Slice2D> {
        self.read_slice_with_cancelled(request, row_axis, col_axis, fixed_axes, None)
    }

    fn read_slice_on_axes_cancellable(
        &self,
        request: &SliceRequest,
        row_axis: Option<&str>,
        col_axis: Option<&str>,
        fixed_axes: &[(String, usize)],
        cancelled: Arc<AtomicBool>,
    ) -> Result<Slice2D> {
        self.read_slice_with_cancelled(request, row_axis, col_axis, fixed_axes, Some(cancelled))
    }

    fn time_label(&self, index: usize) -> Option<String> {
        self.messages
            .get(index)
            .and_then(|message| message.indexed_time_label.clone())
    }

    fn time_label_for_variable(&self, variable: &str, _index: usize) -> Option<String> {
        self.messages
            .iter()
            .find(|message| message.public_name == variable)
            .and_then(|message| message.indexed_time_label.clone())
    }

    fn vertical_label(&self, variable: &str, index: usize) -> Option<String> {
        let message_index = self
            .messages
            .iter()
            .position(|message| message.public_name == variable)?;
        let decoder = self.cached_decoder(message_index)?;
        let decoder_variable = self.messages[message_index]
            .decoder_name
            .as_deref()
            .or_else(|| {
                decoder
                    .metadata()
                    .variables
                    .first()
                    .map(|variable| variable.name.as_str())
            })?;
        decoder.vertical_label(decoder_variable, index)
    }

    fn dimension_values(&self, variable: &str, dimension: &str) -> Option<Vec<f64>> {
        let message_index = self
            .messages
            .iter()
            .position(|message| message.public_name == variable)?;
        let decoder = self.cached_decoder(message_index)?;
        let decoder_variable = self.messages[message_index]
            .decoder_name
            .as_deref()
            .or_else(|| {
                decoder
                    .metadata()
                    .variables
                    .first()
                    .map(|variable| variable.name.as_str())
            })?;
        decoder.dimension_values(decoder_variable, dimension)
    }

    fn point_coordinates(&self, variable: &str, row: usize, col: usize) -> PointCoordinates {
        let Some(message_index) = self
            .messages
            .iter()
            .position(|message| message.public_name == variable)
        else {
            return PointCoordinates::default();
        };
        let Some(decoder) = self.cached_decoder(message_index) else {
            return PointCoordinates::default();
        };
        let Some(decoder_variable) = self.messages[message_index]
            .decoder_name
            .as_deref()
            .or_else(|| {
                decoder
                    .metadata()
                    .variables
                    .first()
                    .map(|variable| variable.name.as_str())
            })
        else {
            return PointCoordinates::default();
        };
        decoder.point_coordinates(decoder_variable, row, col)
    }
}
