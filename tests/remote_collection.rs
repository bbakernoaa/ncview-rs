use ncview_rs::{
    data::{
        AxisRole, DataSource, DatasetFormat, DatasetMetadata, Dimension, PointCoordinates,
        Variable,
        slice::{Slice2D, SliceRequest},
        virtual_dataset::VirtualDatasetManifest,
    },
    error::{NcvError, Result},
};

struct CollectionSource {
    metadata: DatasetMetadata,
    identity: Option<String>,
    labels: Vec<String>,
}

impl DataSource for CollectionSource {
    fn metadata(&self) -> &DatasetMetadata {
        &self.metadata
    }

    fn read_slice(&self, _request: &SliceRequest) -> Result<Slice2D> {
        Err(NcvError::WorkerStopped)
    }

    fn source_identity(&self) -> Option<&str> {
        self.identity.as_deref()
    }

    fn time_label_for_variable(&self, _variable: &str, index: usize) -> Option<String> {
        self.labels.get(index).cloned()
    }

    fn point_coordinates(&self, _variable: &str, row: usize, col: usize) -> PointCoordinates {
        PointCoordinates {
            latitude: Some(row as f64),
            longitude: Some(col as f64),
        }
    }
}

fn metadata(path: &str, format: DatasetFormat, time_length: usize) -> DatasetMetadata {
    DatasetMetadata {
        path: path.into(),
        format,
        dimensions: vec![
            Dimension {
                name: "time".into(),
                length: time_length,
                role: AxisRole::Time,
            },
            Dimension {
                name: "latitude".into(),
                length: 2,
                role: AxisRole::Latitude,
            },
            Dimension {
                name: "longitude".into(),
                length: 2,
                role: AxisRole::Longitude,
            },
        ],
        variables: vec![Variable {
            name: "temperature".into(),
            dimensions: vec!["time".into(), "latitude".into(), "longitude".into()],
            numeric: true,
            units: Some("K".into()),
            long_name: Some("temperature".into()),
            standard_name: Some("air_temperature".into()),
        }],
    }
}

#[test]
fn mixed_local_remote_sources_keep_identity_and_deterministic_frame_mapping() {
    let local = CollectionSource {
        metadata: metadata("local.nc", DatasetFormat::NetCdf4, 2),
        identity: None,
        labels: vec!["2026-09-14T00:00:00Z".into(), "2026-09-14T03:00:00Z".into()],
    };
    let remote = CollectionSource {
        metadata: metadata("s3://bucket/part.nc", DatasetFormat::NetCdf4, 1),
        identity: Some("s3/bucket/part.nc/etag-1".into()),
        labels: vec!["2026-09-14T06:00:00Z".into()],
    };
    let sources: Vec<&dyn DataSource> = vec![&local, &remote];
    let manifest = VirtualDatasetManifest::from_sources(&sources);

    assert_eq!(manifest.sources[0].identity, None);
    assert_eq!(
        manifest.sources[1].identity.as_deref(),
        Some("s3/bucket/part.nc/etag-1")
    );
    let variable = manifest.variable("temperature").unwrap();
    assert!(variable.compatible);
    assert_eq!(
        variable.frames,
        vec![
            ncview_rs::data::virtual_dataset::VirtualFrame {
                source_index: 0,
                local_index: 0,
                source_identity: None,
                label: Some("2026-09-14T00:00:00Z".into()),
            },
            ncview_rs::data::virtual_dataset::VirtualFrame {
                source_index: 0,
                local_index: 1,
                source_identity: None,
                label: Some("2026-09-14T03:00:00Z".into()),
            },
            ncview_rs::data::virtual_dataset::VirtualFrame {
                source_index: 1,
                local_index: 0,
                source_identity: Some("s3/bucket/part.nc/etag-1".into()),
                label: Some("2026-09-14T06:00:00Z".into()),
            },
        ]
    );

    let grib_first = CollectionSource {
        metadata: metadata("forecast-00.grib2", DatasetFormat::Grib2, 1),
        identity: Some("etag-grib-00".into()),
        labels: vec!["2026-09-14T00:00:00Z".into()],
    };
    let grib_second = CollectionSource {
        metadata: metadata("forecast-03.grib2", DatasetFormat::Grib2, 1),
        identity: Some("etag-grib-03".into()),
        labels: vec!["2026-09-14T03:00:00Z".into()],
    };
    let grib_sources: Vec<&dyn DataSource> = vec![&grib_first, &grib_second];
    let grib_manifest = VirtualDatasetManifest::from_sources(&grib_sources);
    assert!(grib_manifest.variable("temperature").unwrap().compatible);
    assert_eq!(
        grib_manifest
            .frames_for_variable("temperature")
            .unwrap()
            .iter()
            .map(|frame| (frame.source_index, frame.local_index))
            .collect::<Vec<_>>(),
        vec![(0, 0), (1, 0)]
    );
}

#[test]
fn collection_reports_missing_incompatible_and_duplicate_partitions() {
    let first = CollectionSource {
        metadata: metadata("first.nc", DatasetFormat::NetCdf4, 1),
        identity: None,
        labels: vec!["same".into()],
    };
    let mut missing_metadata = metadata("missing.nc", DatasetFormat::NetCdf4, 1);
    missing_metadata.variables.clear();
    let missing = CollectionSource {
        metadata: missing_metadata,
        identity: Some("etag-missing".into()),
        labels: vec!["same".into()],
    };
    let mut incompatible_metadata = metadata("incompatible.grib2", DatasetFormat::Grib2, 1);
    incompatible_metadata.dimensions[1].length = 3;
    let incompatible = CollectionSource {
        metadata: incompatible_metadata,
        identity: Some("etag-incompatible".into()),
        labels: vec!["other".into()],
    };
    let duplicate = CollectionSource {
        metadata: metadata("duplicate.nc", DatasetFormat::NetCdf4, 1),
        identity: Some("etag-duplicate".into()),
        labels: vec!["same".into()],
    };
    let sources: Vec<&dyn DataSource> = vec![&first, &missing, &incompatible, &duplicate];
    let manifest = VirtualDatasetManifest::from_sources(&sources);

    let variable = manifest.variable("temperature").unwrap();
    assert!(!variable.compatible);
    assert!(
        manifest
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("missing from source 1"))
    );
    assert!(
        manifest
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("duplicate time label"))
    );
    assert!(
        manifest
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.contains("format") || diagnostic.contains("dimension"))
    );
}
