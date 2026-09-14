//! In-memory virtual dataset indexing for multi-file views.
//!
//! This is deliberately format-neutral. It does not copy data or create a
//! persistent reference store; it records which source and local coordinate
//! back each logical frame, so the existing readers remain responsible for
//! decoding NetCDF and GRIB data.

use std::collections::{BTreeMap, BTreeSet};

use super::{AxisRole, DataSource, DatasetFormat, DatasetMetadata, Variable};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualSource {
    pub path: String,
    pub format: DatasetFormat,
    pub identity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualFrame {
    pub source_index: usize,
    pub local_index: usize,
    pub source_identity: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualVariable {
    pub name: String,
    pub source_indices: Vec<usize>,
    pub frames: Vec<VirtualFrame>,
    pub compatible: bool,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VirtualDatasetManifest {
    pub sources: Vec<VirtualSource>,
    variables: BTreeMap<String, VirtualVariable>,
    diagnostics: Vec<String>,
}

impl VirtualDatasetManifest {
    /// Build the in-memory index shared by all opened sources.
    ///
    /// Variables are matched by name. A variable can still be displayed when
    /// its schemas differ between files, but the mismatch is retained as a
    /// diagnostic instead of silently presenting the files as identical.
    pub fn from_metadata(metadata: &[&DatasetMetadata]) -> Self {
        let identities = vec![None; metadata.len()];
        Self::from_metadata_with_identities(metadata, &identities)
    }

    /// Build the collection index while retaining immutable remote source identities.
    ///
    /// Local sources return no identity token. Remote adapters return the version/ETag-derived
    /// cache token captured during `HEAD`, allowing collection diagnostics and frame mappings to
    /// distinguish two objects that happen to share the same display path.
    pub fn from_sources(sources: &[&dyn DataSource]) -> Self {
        let metadata = sources
            .iter()
            .map(|source| source.metadata())
            .collect::<Vec<_>>();
        let identities = sources
            .iter()
            .map(|source| source.source_identity())
            .collect::<Vec<_>>();
        let mut manifest = Self::from_metadata_with_identities(&metadata, &identities);
        for variable in manifest.variables.values_mut() {
            for frame in &mut variable.frames {
                frame.label = sources.get(frame.source_index).and_then(|source| {
                    source.time_label_for_variable(&variable.name, frame.local_index)
                });
            }
        }
        manifest.validate_source_semantics(sources);
        manifest
    }

    fn from_metadata_with_identities(
        metadata: &[&DatasetMetadata],
        identities: &[Option<&str>],
    ) -> Self {
        let sources = metadata
            .iter()
            .enumerate()
            .map(|(index, dataset)| VirtualSource {
                path: dataset.path.clone(),
                format: dataset.format,
                identity: identities.get(index).copied().flatten().map(str::to_owned),
            })
            .collect();

        let mut grouped = BTreeMap::<String, Vec<(usize, Variable)>>::new();
        let mut all_variable_names = BTreeSet::new();
        for (source_index, dataset) in metadata.iter().enumerate() {
            for variable in &dataset.variables {
                all_variable_names.insert(variable.name.clone());
                grouped
                    .entry(variable.name.clone())
                    .or_default()
                    .push((source_index, variable.clone()));
            }
        }

        let variables: BTreeMap<String, VirtualVariable> = grouped
            .into_iter()
            .map(|(name, entries)| {
                let source_indices = entries
                    .iter()
                    .map(|(source_index, _)| *source_index)
                    .collect::<Vec<_>>();
                let frames = entries
                    .iter()
                    .flat_map(|(source_index, variable)| {
                        let time_length =
                            leading_lengths(metadata[*source_index], variable).0.max(1);
                        (0..time_length).map(move |local_index| VirtualFrame {
                            source_index: *source_index,
                            local_index,
                            source_identity: identities
                                .get(*source_index)
                                .copied()
                                .flatten()
                                .map(str::to_owned),
                            label: None,
                        })
                    })
                    .collect();

                let reference = &entries[0].1;
                let mut diagnostics = Vec::new();
                for (source_index, candidate) in entries.iter().skip(1) {
                    let mut differences = Vec::new();
                    if metadata[entries[0].0].format != metadata[*source_index].format {
                        differences.push("format");
                    }
                    if reference.dimensions != candidate.dimensions {
                        differences.push("dimensions");
                    }
                    if !dimensions_match(metadata[*source_index], metadata[entries[0].0], candidate)
                    {
                        differences.push("dimension shapes or roles");
                    }
                    if reference.numeric != candidate.numeric {
                        differences.push("numeric type");
                    }
                    if reference.units != candidate.units {
                        differences.push("units");
                    }
                    if reference.long_name != candidate.long_name {
                        differences.push("long name");
                    }
                    if reference.standard_name != candidate.standard_name {
                        differences.push("standard name");
                    }
                    if !differences.is_empty() {
                        let diagnostic = format!(
                            "variable {name} differs from source 0 in {} (source {source_index})",
                            differences.join(", ")
                        );
                        diagnostics.push(diagnostic.clone());
                    }
                }

                for (source_index, source_metadata) in metadata.iter().enumerate() {
                    if !entries
                        .iter()
                        .any(|(entry_source_index, _)| *entry_source_index == source_index)
                    {
                        diagnostics.push(format!(
                            "variable {name} is missing from source {source_index} ({})",
                            source_metadata.path
                        ));
                    }
                }
                let compatible =
                    !entries
                        .iter()
                        .enumerate()
                        .any(|(entry_index, (source_index, candidate))| {
                            let reference_source = entries[0].0;
                            entry_index > 0
                                && (!dimensions_match(
                                    metadata[*source_index],
                                    metadata[reference_source],
                                    candidate,
                                ) || reference.dimensions != candidate.dimensions
                                    || metadata[*source_index].format
                                        != metadata[reference_source].format
                                    || reference.numeric != candidate.numeric
                                    || reference.units != candidate.units
                                    || reference.long_name != candidate.long_name
                                    || reference.standard_name != candidate.standard_name)
                        })
                        && entries.len() == metadata.len();

                let variable_diagnostics = diagnostics;
                (
                    name.clone(),
                    VirtualVariable {
                        name,
                        source_indices,
                        frames,
                        compatible,
                        diagnostics: variable_diagnostics,
                    },
                )
            })
            .collect();

        let mut diagnostics = Vec::new();
        for name in all_variable_names {
            if let Some(variable) = variables.get(&name)
                && !variable.compatible
            {
                diagnostics.extend(variable.diagnostics.iter().cloned());
            }
        }
        diagnostics.sort();
        diagnostics.dedup();
        Self {
            sources,
            variables,
            diagnostics,
        }
    }

    pub fn variable(&self, name: &str) -> Option<&VirtualVariable> {
        self.variables.get(name)
    }

    pub fn variables(&self) -> impl Iterator<Item = &VirtualVariable> {
        self.variables.values()
    }

    pub fn frames_for_variable(&self, name: &str) -> Option<&[VirtualFrame]> {
        self.variable(name)
            .map(|variable| variable.frames.as_slice())
    }

    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    fn validate_source_semantics(&mut self, sources: &[&dyn DataSource]) {
        let mut labels = BTreeMap::<(String, String), (usize, usize)>::new();
        for (source_index, source) in sources.iter().enumerate() {
            for variable in &source.metadata().variables {
                let time_length = leading_lengths(source.metadata(), variable).0.max(1);
                for local_index in 0..time_length {
                    let label = source
                        .time_label_for_variable(&variable.name, local_index)
                        .unwrap_or_else(|| format!("t={local_index}"));
                    if let Some((previous_source, previous_index)) = labels.insert(
                        (variable.name.clone(), label.clone()),
                        (source_index, local_index),
                    ) {
                        self.diagnostics.push(format!(
                            "variable {} has duplicate time label {label:?} in source {} index {} and source {source_index} index {local_index}",
                            variable.name, previous_source, previous_index
                        ));
                    }
                }
            }
        }
        let names = self.variables.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let Some(variable) = self.variables.get(&name) else {
                continue;
            };
            if !variable.compatible {
                continue;
            }
            let source_indices = variable.source_indices.clone();
            let Some(reference_source_index) = source_indices.first().copied() else {
                continue;
            };
            let Some(reference_source) = sources.get(reference_source_index) else {
                continue;
            };
            let Some(reference_variable) = reference_source
                .metadata()
                .variables
                .iter()
                .find(|candidate| candidate.name == name)
            else {
                continue;
            };
            for source_index in source_indices.iter().skip(1).copied() {
                let Some(source) = sources.get(source_index) else {
                    continue;
                };
                let Some(candidate) = source
                    .metadata()
                    .variables
                    .iter()
                    .find(|candidate| candidate.name == name)
                else {
                    continue;
                };
                for dimension in &reference_variable.dimensions {
                    let reference_values =
                        reference_source.dimension_values(name.as_str(), dimension);
                    let candidate_values = source.dimension_values(name.as_str(), dimension);
                    if let (Some(reference_values), Some(candidate_values)) =
                        (reference_values, candidate_values)
                    {
                        let same = reference_values.len() == candidate_values.len()
                            && reference_values.iter().zip(candidate_values.iter()).all(
                                |(reference, candidate)| {
                                    (reference - candidate).abs() <= 1e-12
                                        || (reference.is_nan() && candidate.is_nan())
                                },
                            );
                        if !same {
                            self.add_semantic_diagnostic(format!(
                                "variable {name} coordinate dimension {dimension} differs between source {reference_source_index} and source {source_index}"
                            ));
                        }
                    }
                }

                let (reference_rows, reference_cols) =
                    spatial_shape(reference_source.metadata(), reference_variable);
                let (candidate_rows, candidate_cols) = spatial_shape(source.metadata(), candidate);
                if (reference_rows, reference_cols) != (candidate_rows, candidate_cols) {
                    self.add_semantic_diagnostic(format!(
                        "variable {name} grid shape differs between source {reference_source_index} and source {source_index}"
                    ));
                }
                // GRIB2 coordinate lookup currently requires decoding the
                // complete packed message. Do not turn collection manifest
                // refreshes into one full value decode per variable/source;
                // dimensions and the GRIB grid metadata already establish
                // the bounded compatibility check here. NetCDF coordinate
                // values remain validated because their coordinate reads are
                // independently cached and range-addressable.
                if reference_source.metadata().format != DatasetFormat::Grib2
                    && source.metadata().format != DatasetFormat::Grib2
                {
                    for (row, col) in [
                        (0, 0),
                        (
                            reference_rows.saturating_sub(1),
                            reference_cols.saturating_sub(1),
                        ),
                    ] {
                        let reference_coordinates =
                            reference_source.point_coordinates(&name, row, col);
                        let candidate_coordinates = source.point_coordinates(&name, row, col);
                        if !coordinates_match(reference_coordinates, candidate_coordinates) {
                            self.add_semantic_diagnostic(format!(
                                "variable {name} coordinate values differ between source {reference_source_index} and source {source_index}"
                            ));
                            break;
                        }
                    }
                }
            }
        }
    }

    fn add_semantic_diagnostic(&mut self, diagnostic: String) {
        self.diagnostics.push(diagnostic.clone());
        if let Some(name) = diagnostic
            .strip_prefix("variable ")
            .and_then(|value| value.split_whitespace().next())
            && let Some(variable) = self.variables.get_mut(name)
        {
            variable.compatible = false;
            variable.diagnostics.push(diagnostic);
        }
    }
}

fn dimensions_match(
    candidate_metadata: &DatasetMetadata,
    reference_metadata: &DatasetMetadata,
    candidate: &Variable,
) -> bool {
    candidate.dimensions.iter().all(|dimension_name| {
        let candidate_dimension = candidate_metadata
            .dimensions
            .iter()
            .find(|dimension| dimension.name == *dimension_name);
        let reference_dimension = reference_metadata
            .dimensions
            .iter()
            .find(|dimension| dimension.name == *dimension_name);
        candidate_dimension.zip(reference_dimension).is_some_and(
            |(candidate_dimension, reference_dimension)| {
                candidate_dimension.role == reference_dimension.role
                    && (candidate_dimension.length == reference_dimension.length
                        || candidate_dimension.role == AxisRole::Time)
            },
        )
    })
}

fn spatial_shape(metadata: &DatasetMetadata, variable: &Variable) -> (usize, usize) {
    let rows = variable
        .dimensions
        .iter()
        .find_map(|name| {
            metadata
                .dimensions
                .iter()
                .find(|dimension| dimension.name == *name)
                .filter(|dimension| dimension.role == AxisRole::Latitude)
                .map(|dimension| dimension.length)
        })
        .unwrap_or(0);
    let cols = variable
        .dimensions
        .iter()
        .find_map(|name| {
            metadata
                .dimensions
                .iter()
                .find(|dimension| dimension.name == *name)
                .filter(|dimension| dimension.role == AxisRole::Longitude)
                .map(|dimension| dimension.length)
        })
        .unwrap_or(0);
    (rows, cols)
}

fn coordinates_match(
    reference: super::PointCoordinates,
    candidate: super::PointCoordinates,
) -> bool {
    fn same(reference: Option<f64>, candidate: Option<f64>) -> bool {
        match (reference, candidate) {
            (Some(reference), Some(candidate)) => {
                (reference - candidate).abs() <= 1e-12 || (reference.is_nan() && candidate.is_nan())
            }
            (None, None) => true,
            _ => false,
        }
    }
    same(reference.latitude, candidate.latitude) && same(reference.longitude, candidate.longitude)
}

/// Return the logical time and depth lengths for a variable.
///
/// Leading axes are inferred in the same way as the viewer's slice loader:
/// latitude/longitude roles identify the spatial plane, followed by explicit
/// time/depth roles and finally the conventional first/second-axis fallback.
pub fn leading_lengths(metadata: &DatasetMetadata, variable: &Variable) -> (usize, usize) {
    let row_index = variable
        .dimensions
        .iter()
        .position(|name| {
            metadata
                .dimensions
                .iter()
                .find(|dimension| dimension.name == *name)
                .is_some_and(|dimension| dimension.role == AxisRole::Latitude)
        })
        .unwrap_or(variable.dimensions.len().saturating_sub(2));
    let col_index = variable
        .dimensions
        .iter()
        .position(|name| {
            metadata
                .dimensions
                .iter()
                .find(|dimension| dimension.name == *name)
                .is_some_and(|dimension| dimension.role == AxisRole::Longitude)
        })
        .unwrap_or(variable.dimensions.len().saturating_sub(1));
    variable
        .dimensions
        .iter()
        .enumerate()
        .filter(|(axis, _)| *axis != row_index && *axis != col_index)
        .fold((1, 1), |(time, depth), (axis, name)| {
            let length = metadata
                .dimensions
                .iter()
                .find(|dimension| dimension.name == *name)
                .map_or(1, |dimension| dimension.length);
            let role = match metadata
                .dimensions
                .iter()
                .find(|dimension| dimension.name == *name)
                .map(|dimension| dimension.role)
            {
                Some(AxisRole::Time) => AxisRole::Time,
                Some(AxisRole::Depth) => AxisRole::Depth,
                _ if axis == row_index => AxisRole::Other,
                _ => match axis {
                    0 => AxisRole::Time,
                    1 => AxisRole::Depth,
                    _ => AxisRole::Other,
                },
            };
            match role {
                AxisRole::Time => (length, depth),
                AxisRole::Depth => (time, length),
                _ => (time, depth),
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct LabelledSource {
        metadata: DatasetMetadata,
        labels: Vec<String>,
    }

    impl DataSource for LabelledSource {
        fn metadata(&self) -> &DatasetMetadata {
            &self.metadata
        }

        fn read_slice(
            &self,
            _request: &crate::data::slice::SliceRequest,
        ) -> crate::error::Result<crate::data::slice::Slice2D> {
            Err(crate::error::NcvError::WorkerStopped)
        }

        fn time_label_for_variable(&self, _variable: &str, index: usize) -> Option<String> {
            self.labels.get(index).cloned()
        }
    }

    fn metadata(path: &str, time_length: usize, units: &str) -> DatasetMetadata {
        DatasetMetadata {
            path: path.into(),
            format: DatasetFormat::NetCdf4,
            dimensions: vec![
                super::super::Dimension {
                    name: "time".into(),
                    length: time_length,
                    role: AxisRole::Time,
                },
                super::super::Dimension {
                    name: "lat".into(),
                    length: 2,
                    role: AxisRole::Latitude,
                },
                super::super::Dimension {
                    name: "lon".into(),
                    length: 3,
                    role: AxisRole::Longitude,
                },
            ],
            variables: vec![Variable {
                name: "temperature".into(),
                dimensions: vec!["time".into(), "lat".into(), "lon".into()],
                numeric: true,
                units: Some(units.into()),
                long_name: None,
                standard_name: None,
            }],
        }
    }

    #[test]
    fn concatenates_frames_across_sources() {
        let first = metadata("first.nc", 2, "K");
        let second = metadata("second.nc", 3, "K");
        let manifest = VirtualDatasetManifest::from_metadata(&[&first, &second]);

        assert_eq!(manifest.sources.len(), 2);
        assert_eq!(
            manifest.variable("temperature").unwrap().source_indices,
            [0, 1]
        );
        assert_eq!(
            manifest.frames_for_variable("temperature").unwrap(),
            &[
                VirtualFrame {
                    source_index: 0,
                    local_index: 0,
                    source_identity: None,
                    label: None,
                },
                VirtualFrame {
                    source_index: 0,
                    local_index: 1,
                    source_identity: None,
                    label: None,
                },
                VirtualFrame {
                    source_index: 1,
                    local_index: 0,
                    source_identity: None,
                    label: None,
                },
                VirtualFrame {
                    source_index: 1,
                    local_index: 1,
                    source_identity: None,
                    label: None,
                },
                VirtualFrame {
                    source_index: 1,
                    local_index: 2,
                    source_identity: None,
                    label: None,
                },
            ]
        );
    }

    #[test]
    fn retains_schema_mismatch_diagnostics() {
        let first = metadata("first.nc", 1, "K");
        let second = metadata("second.nc", 1, "degC");
        let manifest = VirtualDatasetManifest::from_metadata(&[&first, &second]);
        let variable = manifest.variable("temperature").unwrap();

        assert!(!variable.compatible);
        assert_eq!(variable.diagnostics.len(), 1);
        assert!(variable.diagnostics[0].contains("units"));
    }

    #[test]
    fn missing_variable_is_diagnostic_and_not_marked_as_a_complete_collection() {
        let first = metadata("first.nc", 1, "K");
        let mut second = metadata("second.nc", 1, "K");
        second.variables.clear();
        let manifest = VirtualDatasetManifest::from_metadata(&[&first, &second]);
        let variable = manifest.variable("temperature").unwrap();

        assert!(!variable.compatible);
        assert_eq!(variable.frames.len(), 1);
        assert!(
            manifest
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.contains("missing from source 1"))
        );
    }

    #[test]
    fn dimension_shape_and_role_mismatches_are_diagnostic() {
        let first = metadata("first.nc", 1, "K");
        let mut second = metadata("second.nc", 1, "K");
        second.dimensions[1].length = 4;
        second.dimensions[1].role = AxisRole::Other;
        let manifest = VirtualDatasetManifest::from_metadata(&[&first, &second]);
        let variable = manifest.variable("temperature").unwrap();

        assert!(!variable.compatible);
        assert!(
            variable
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("dimension shapes or roles"))
        );
    }

    #[test]
    fn duplicate_time_labels_are_reported_for_source_mapping() {
        let first = LabelledSource {
            metadata: metadata("first.nc", 2, "K"),
            labels: vec!["2026-09-14T00:00:00Z".into(), "2026-09-14T03:00:00Z".into()],
        };
        let second = LabelledSource {
            metadata: metadata("second.nc", 2, "K"),
            labels: vec!["2026-09-14T00:00:00Z".into(), "2026-09-14T06:00:00Z".into()],
        };
        let sources: Vec<&dyn DataSource> = vec![&first, &second];
        let manifest = VirtualDatasetManifest::from_sources(&sources);

        assert!(
            manifest
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.contains("duplicate time label"))
        );
    }
}
