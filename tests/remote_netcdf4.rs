use std::{path::Path, sync::Arc};

use object_store::{ObjectStoreExt, PutPayload, memory::InMemory, path::Path as ObjectPath};

use ncview_rs::{
    data,
    data::remote_hdf5::RemoteByteSource,
    storage::{location::SourceLocation, object_store::RemoteStore},
};

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn bounded_remote_netcdf4_uses_the_existing_decoder() {
    let bytes = include_bytes!("fixtures/regular.nc4");
    let store = Arc::new(InMemory::new());
    let rt = runtime();
    rt.block_on(async {
        store
            .put(
                &ObjectPath::from("regular.nc4"),
                PutPayload::from(bytes.as_slice()),
            )
            .await
            .unwrap();
    });
    let source = SourceLocation::parse("gs://bucket/regular.nc4").unwrap();
    let opened = data::remote::open_remote_with_store(source, store).unwrap();
    assert!(opened.is_remote());
    let capabilities = opened.remote_capabilities().unwrap();
    assert!(capabilities.range_reads);
    assert!(capabilities.bounded_fallback);
    assert!(!capabilities.chunked_reads);
    assert_eq!(opened.metadata().format, data::DatasetFormat::NetCdf4);
    assert!(
        opened
            .metadata()
            .variables
            .iter()
            .any(|variable| variable.name == "temperature")
    );
}

#[test]
fn bounded_remote_netcdf4_matches_local_metadata_and_values() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/regular.nc4");
    let local = data::open(&fixture).unwrap();
    let bytes = std::fs::read(&fixture).unwrap();
    let store = Arc::new(InMemory::new());
    let rt = runtime();
    rt.block_on(async {
        store
            .put(&ObjectPath::from("regular.nc4"), PutPayload::from(bytes))
            .await
            .unwrap();
    });
    let source = SourceLocation::parse("s3://bucket/regular.nc4").unwrap();
    let remote = data::remote::open_remote_with_store(source, store).unwrap();

    assert_eq!(remote.metadata().format, local.metadata().format);
    assert_eq!(remote.metadata().dimensions, local.metadata().dimensions);
    assert_eq!(remote.metadata().variables, local.metadata().variables);
    let request = data::slice::SliceRequest {
        variable: "temperature".into(),
        time: 0,
        depth: 0,
        bounds: data::slice::Bounds::new(0, 4, 0, 5).unwrap(),
    };
    let local_slice = local.read_slice(&request).unwrap();
    let remote_slice = remote.read_slice(&request).unwrap();
    assert_eq!(remote_slice.values, local_slice.values);
    assert_eq!(remote_slice.validity, local_slice.validity);
}

#[test]
fn paired_remote_netcdf4_fixtures_preserve_coordinates_and_masks() {
    for (fixture_name, variable_name) in [
        ("regular.nc4", "temperature"),
        ("packed-fill.nc4", "temperature"),
        ("curvilinear.nc4", "temperature"),
        ("coards-float32.nc4", "MACCity"),
    ] {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(fixture_name);
        let local = data::open(&fixture).unwrap();
        let bytes = std::fs::read(&fixture).unwrap();
        let store = Arc::new(InMemory::new());
        let rt = runtime();
        rt.block_on(async {
            store
                .put(&ObjectPath::from(fixture_name), PutPayload::from(bytes))
                .await
                .unwrap();
        });
        let source = SourceLocation::parse(&format!("s3://bucket/{fixture_name}")).unwrap();
        let remote = data::remote::open_remote_with_store(source, store).unwrap();
        assert_eq!(remote.metadata().dimensions, local.metadata().dimensions);
        assert_eq!(remote.metadata().variables, local.metadata().variables);

        let variable = local
            .metadata()
            .variables
            .iter()
            .find(|variable| variable.name == variable_name)
            .unwrap();
        let dimensions = &local.metadata().dimensions;
        let row_dimension = variable
            .dimensions
            .iter()
            .find_map(|name| {
                dimensions
                    .iter()
                    .find(|dimension| dimension.name == *name)
                    .filter(|dimension| dimension.role == data::AxisRole::Latitude)
            })
            .or_else(|| {
                variable
                    .dimensions
                    .get(variable.dimensions.len().saturating_sub(2))
                    .and_then(|name| dimensions.iter().find(|dimension| dimension.name == *name))
            })
            .unwrap();
        let col_dimension = variable
            .dimensions
            .iter()
            .find_map(|name| {
                dimensions
                    .iter()
                    .find(|dimension| dimension.name == *name)
                    .filter(|dimension| dimension.role == data::AxisRole::Longitude)
            })
            .or_else(|| {
                variable
                    .dimensions
                    .last()
                    .and_then(|name| dimensions.iter().find(|dimension| dimension.name == *name))
            })
            .unwrap();
        let bounds =
            data::slice::Bounds::new(0, row_dimension.length, 0, col_dimension.length).unwrap();
        let request = data::slice::SliceRequest {
            variable: variable_name.into(),
            time: 0,
            depth: 0,
            bounds,
        };
        let local_slice = local.read_slice(&request).unwrap();
        let remote_slice = remote.read_slice(&request).unwrap();
        assert_eq!(remote_slice.values, local_slice.values, "{fixture_name}");
        assert_eq!(
            remote_slice.validity, local_slice.validity,
            "{fixture_name}"
        );
        assert_eq!(
            remote_slice
                .coordinates
                .as_ref()
                .and_then(|grid| grid.latitude.as_ref()),
            local_slice
                .coordinates
                .as_ref()
                .and_then(|grid| grid.latitude.as_ref()),
            "{fixture_name} latitude"
        );
        assert_eq!(
            remote_slice
                .coordinates
                .as_ref()
                .and_then(|grid| grid.longitude.as_ref()),
            local_slice
                .coordinates
                .as_ref()
                .and_then(|grid| grid.longitude.as_ref()),
            "{fixture_name} longitude"
        );
    }
}

#[test]
fn large_range_backed_source_reads_metadata_and_chunks_without_full_object_reads() {
    let rt = runtime();
    rt.block_on(async {
        let object_size = 16 * 1024 * 1024;
        let mut object = vec![0_u8; object_size];
        object[0..8].copy_from_slice(b"\x89HDF\r\n\x1a\n");
        object[4 * 1024 * 1024..4 * 1024 * 1024 + 4].copy_from_slice(b"CHNK");
        let store = Arc::new(InMemory::new());
        store
            .put(&ObjectPath::from("large.nc4"), PutPayload::from(object))
            .await
            .unwrap();
        let source = SourceLocation::parse("s3://bucket/large.nc4").unwrap();
        let remote = Arc::new(RemoteStore::new(source, store));
        let identity = remote.head().await.unwrap();
        let bytes = RemoteByteSource::new(Arc::clone(&remote), identity, 4096, 64 * 1024);

        assert_eq!(&bytes.read(0, 8).await.unwrap()[..], b"\x89HDF\r\n\x1a\n");
        assert_eq!(&bytes.read(1, 3).await.unwrap()[..], b"HDF");
        assert_eq!(&bytes.read(4 * 1024 * 1024, 4).await.unwrap()[..], b"CHNK");
        assert!(
            remote
                .requested_ranges()
                .iter()
                .all(|range| range.len() < object_size as u64)
        );
        assert_eq!(remote.requested_ranges().len(), 2);
        let stats = remote.stats();
        assert!(stats.requested_bytes < object_size as u64);
        assert_eq!(stats.received_bytes, stats.requested_bytes);
    });
}

#[test]
fn large_remote_netcdf4_uses_source_backed_metadata_and_payload_ranges() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/regular.nc4");
    let local = data::open(&fixture).unwrap();
    let mut bytes = std::fs::read(&fixture).unwrap();
    let large_size = 65 * 1024 * 1024;
    bytes.resize(large_size, 0);

    let store = Arc::new(InMemory::new());
    let rt = runtime();
    rt.block_on(async {
        store
            .put(
                &ObjectPath::from("large-regular.nc4"),
                PutPayload::from(bytes),
            )
            .await
            .unwrap();
    });

    let source = SourceLocation::parse("s3://bucket/large-regular.nc4").unwrap();
    let remote = Arc::new(RemoteStore::new(source, store));
    let identity = rt.block_on(remote.head()).unwrap();
    let runtime = ncview_rs::storage::StorageRuntime::spawn().unwrap();
    let opened =
        data::remote_netcdf4::open_remote(Arc::clone(&remote), identity, Arc::new(runtime))
            .unwrap();

    assert!(
        opened
            .metadata()
            .variables
            .iter()
            .any(|v| v.name == "temperature")
    );
    let request = data::slice::SliceRequest {
        variable: "temperature".into(),
        time: 0,
        depth: 0,
        bounds: data::slice::Bounds::new(0, 4, 0, 5).unwrap(),
    };
    let remote_slice = opened.read_slice(&request).unwrap();
    let local_slice = local.read_slice(&request).unwrap();
    assert_eq!(remote_slice.values, local_slice.values);
    assert_eq!(remote_slice.validity, local_slice.validity);

    let stats = remote.stats();
    assert!(stats.requested_bytes < large_size as u64);
    assert!(
        remote
            .requested_ranges()
            .iter()
            .all(|range| range.len() < large_size as u64)
    );
}
