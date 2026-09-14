use std::{fs, path::Path, sync::Arc};

use bytes::Bytes;
use ncview_rs::{
    data,
    storage::{StorageRuntime, location::SourceLocation, object_store::RemoteStore},
};
use object_store::{ObjectStoreExt, PutPayload, memory::InMemory, path::Path as ObjectPath};

#[test]
fn indexed_remote_grib_reads_one_complete_message_range() {
    let source_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("gefs.chem.t12z.a2d_0p25.f000.grib2");
    if !source_path.exists() {
        // Large scientific inputs are intentionally excluded from the repository. The
        // credential-free adapter contract is still exercised by the range tests in that case.
        return;
    }
    let source_bytes = fs::read(&source_path).expect("repository GRIB fixture");
    let message_length = fs::read_to_string(source_path.with_extension("grib2.idx"))
        .expect("repository GRIB index")
        .lines()
        .nth(1)
        .and_then(|line| line.split(':').nth(1))
        .and_then(|offset| offset.parse::<usize>().ok())
        .expect("second index offset");
    let message = Bytes::copy_from_slice(&source_bytes[..message_length]);
    let source = SourceLocation::parse("s3://bucket/data.grib2").unwrap();
    let store = Arc::new(InMemory::new());
    let setup = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    setup.block_on(async {
        store
            .put(&ObjectPath::from("data.grib2"), message.clone().into())
            .await
            .unwrap();
        store
            .put(&ObjectPath::from("data.grib2.idx"), "1:0:fixture\n".into())
            .await
            .unwrap();
    });
    let remote = Arc::new(RemoteStore::new(source.clone(), store));
    let identity = setup.block_on(async { remote.head().await }).unwrap();
    let runtime = Arc::new(StorageRuntime::spawn().unwrap());
    let opened = data::remote_grib2::open_remote(remote.clone(), identity, runtime).unwrap();
    assert!(opened.source_identity().is_some());
    assert_eq!(opened.metadata().format, data::DatasetFormat::Grib2);
    assert!(!opened.metadata().variables.is_empty());
    assert_eq!(message.len(), message_length);
    assert!(remote.stats().requested_bytes < source_bytes.len() as u64);

    let local = data::open(&source_path).expect("local GRIB2 fixture");
    let local_variable = local
        .metadata()
        .variables
        .first()
        .expect("local GRIB2 variable");
    let remote_variable = opened
        .metadata()
        .variables
        .first()
        .expect("remote GRIB2 variable");
    assert_eq!(remote_variable.long_name, local_variable.long_name);
    assert_eq!(
        opened.time_label_for_variable(&remote_variable.name, 0),
        local.time_label(0)
    );
    let rows = local
        .metadata()
        .dimensions
        .iter()
        .find(|dimension| dimension.role == data::AxisRole::Latitude)
        .map(|dimension| dimension.length)
        .expect("local latitude dimension");
    let cols = local
        .metadata()
        .dimensions
        .iter()
        .find(|dimension| dimension.role == data::AxisRole::Longitude)
        .map(|dimension| dimension.length)
        .expect("local longitude dimension");
    let bounds = data::slice::Bounds::new(0, rows.min(2), 0, cols.min(2)).unwrap();
    let local_slice = local
        .read_slice(&data::slice::SliceRequest {
            variable: local_variable.name.clone(),
            time: 0,
            depth: 0,
            bounds,
        })
        .unwrap();
    let remote_slice = opened
        .read_slice(&data::slice::SliceRequest {
            variable: remote_variable.name.clone(),
            time: 0,
            depth: 0,
            bounds,
        })
        .unwrap();
    assert_eq!(remote_slice.values, local_slice.values);
    assert_eq!(remote_slice.validity, local_slice.validity);
}

#[test]
fn indexed_remote_grib_metadata_does_not_fetch_all_messages() {
    let source_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("gefs.chem.t12z.a2d_0p25.f000.grib2");
    if !source_path.exists() {
        return;
    }
    let source_bytes = fs::read(&source_path).expect("repository GRIB fixture");
    let index =
        fs::read_to_string(source_path.with_extension("grib2.idx")).expect("repository GRIB index");
    let source = SourceLocation::parse("s3://bucket/full-data.grib2").unwrap();
    let store = Arc::new(InMemory::new());
    let setup = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    setup.block_on(async {
        store
            .put(
                &ObjectPath::from("full-data.grib2"),
                PutPayload::from(source_bytes.clone()),
            )
            .await
            .unwrap();
        store
            .put(
                &ObjectPath::from("full-data.grib2.idx"),
                PutPayload::from(index.into_bytes()),
            )
            .await
            .unwrap();
    });
    let remote = Arc::new(RemoteStore::new(
        source,
        Arc::clone(&store) as Arc<dyn object_store::ObjectStore>,
    ));
    let identity = setup.block_on(async { remote.head().await }).unwrap();
    let runtime = Arc::new(StorageRuntime::spawn().unwrap());
    let opened = data::remote_grib2::open_remote(remote.clone(), identity, runtime).unwrap();

    // Opening should fetch the sidecar and one complete message for the
    // spatial prototype. Every later message is represented by a descriptor
    // and fetched only when that variable is selected.
    assert!(opened.metadata().variables.len() > 1);
    assert!(remote.stats().requested_bytes < source_bytes.len() as u64 / 2);
}

#[test]
fn indexed_remote_grib_preserves_aerosol_identity_and_values() {
    let source_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("gefs.chem.t12z.a2d_0p25.f000.grib2");
    if !source_path.exists() {
        return;
    }

    let source_bytes = fs::read(&source_path).expect("repository GRIB fixture");
    let index =
        fs::read_to_string(source_path.with_extension("grib2.idx")).expect("repository GRIB index");
    let offsets = index
        .lines()
        .take(5)
        .map(|line| {
            line.split(':')
                .nth(1)
                .and_then(|offset| offset.parse::<usize>().ok())
                .expect("GRIB index offset")
        })
        .collect::<Vec<_>>();
    assert_eq!(offsets.len(), 5);

    // Messages 1 and 4 are both AOTK fields with different wavelength
    // qualifiers. Keep only those exact messages in the remote object so the
    // test exercises sidecar identity without downloading the full fixture.
    let first = &source_bytes[offsets[0]..offsets[1]];
    let fourth = &source_bytes[offsets[3]..offsets[4]];
    let mut selected = Vec::with_capacity(first.len() + fourth.len());
    selected.extend_from_slice(first);
    selected.extend_from_slice(fourth);
    let sidecar_line = |line: &str, ordinal: usize, offset: usize| {
        let description = line.splitn(3, ':').nth(2).unwrap_or_default();
        format!("{ordinal}:{offset}:{description}")
    };
    let sidecar = format!(
        "{}\n{}\n",
        sidecar_line(index.lines().next().unwrap(), 1, 0),
        sidecar_line(index.lines().nth(3).unwrap(), 2, first.len())
    );

    let store = Arc::new(InMemory::new());
    let setup = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    setup.block_on(async {
        store
            .put(
                &ObjectPath::from("selected.grib2"),
                PutPayload::from(selected),
            )
            .await
            .unwrap();
        store
            .put(
                &ObjectPath::from("selected.grib2.idx"),
                PutPayload::from(sidecar.into_bytes()),
            )
            .await
            .unwrap();
    });

    let source = SourceLocation::parse("s3://bucket/selected.grib2").unwrap();
    let remote = Arc::new(RemoteStore::new(
        source,
        Arc::clone(&store) as Arc<dyn object_store::ObjectStore>,
    ));
    let identity = setup.block_on(async { remote.head().await }).unwrap();
    let runtime = Arc::new(StorageRuntime::spawn().unwrap());
    let opened = data::remote_grib2::open_remote(remote.clone(), identity, runtime).unwrap();
    let local = data::open(&source_path).expect("local GRIB2 fixture");

    assert_eq!(opened.metadata().variables.len(), 2);
    let remote_variables = &opened.metadata().variables;
    assert_ne!(remote_variables[0].name, remote_variables[1].name);
    assert_ne!(remote_variables[0].long_name, remote_variables[1].long_name);
    for variable in remote_variables {
        let label = variable.long_name.as_deref().unwrap_or_default();
        assert!(label.to_ascii_lowercase().contains("aerosol"));
        assert!(label.to_ascii_lowercase().contains("nm"));
    }

    let local_variables = &local.metadata().variables;
    for (remote_variable, local_index) in remote_variables.iter().zip([0_usize, 3]) {
        assert_eq!(
            remote_variable.long_name,
            local_variables[local_index].long_name
        );
        assert_eq!(
            opened.time_label_for_variable(&remote_variable.name, 0),
            local.time_label_for_variable(&local_variables[local_index].name, 0)
        );
        let rows = local
            .metadata()
            .dimensions
            .iter()
            .find(|dimension| dimension.role == data::AxisRole::Latitude)
            .map_or(2, |dimension| dimension.length.min(2));
        let cols = local
            .metadata()
            .dimensions
            .iter()
            .find(|dimension| dimension.role == data::AxisRole::Longitude)
            .map_or(2, |dimension| dimension.length.min(2));
        let bounds = data::slice::Bounds::new(0, rows, 0, cols).unwrap();
        let local_slice = local
            .read_slice(&data::slice::SliceRequest {
                variable: local_variables[local_index].name.clone(),
                time: 0,
                depth: 0,
                bounds,
            })
            .unwrap();
        let remote_slice = opened
            .read_slice(&data::slice::SliceRequest {
                variable: remote_variable.name.clone(),
                time: 0,
                depth: 0,
                bounds,
            })
            .unwrap();
        assert_eq!(remote_slice.values, local_slice.values);
        assert_eq!(remote_slice.validity, local_slice.validity);
    }
    assert!(remote.stats().requested_bytes < source_bytes.len() as u64);
}

#[test]
fn malformed_colocated_index_is_rejected_with_line_context() {
    let store = Arc::new(InMemory::new());
    let setup = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    setup.block_on(async {
        store
            .put(
                &ObjectPath::from("data.grib2"),
                PutPayload::from(&b"GRIB-invalid-fixture"[..]),
            )
            .await
            .unwrap();
        store
            .put(
                &ObjectPath::from("data.grib2.idx"),
                PutPayload::from(&b"not-an-index\n"[..]),
            )
            .await
            .unwrap();
    });
    let source = SourceLocation::parse("s3://bucket/data.grib2").unwrap();
    let remote = Arc::new(RemoteStore::new(source, store));
    let identity = setup.block_on(async { remote.head().await }).unwrap();
    let runtime = Arc::new(StorageRuntime::spawn().unwrap());
    let error = match data::remote_grib2::open_remote(remote, identity, runtime) {
        Ok(_) => panic!("malformed index unexpectedly opened"),
        Err(error) => error,
    };
    assert!(error.to_string().contains(".idx line 1"));
}

#[test]
fn changed_grib2_identity_rejects_indexed_message_ranges() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("gefs.chem.t12z.a2d_0p25.f000.grib2");
    if !fixture.exists() {
        return;
    }
    let source_bytes = fs::read(&fixture).unwrap();
    let index = fs::read_to_string(fixture.with_extension("grib2.idx")).unwrap();
    let store = Arc::new(InMemory::new());
    let setup = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    setup.block_on(async {
        store
            .put(
                &ObjectPath::from("data.grib2"),
                PutPayload::from(source_bytes),
            )
            .await
            .unwrap();
        store
            .put(
                &ObjectPath::from("data.grib2.idx"),
                PutPayload::from(index.into_bytes()),
            )
            .await
            .unwrap();
    });
    let source = SourceLocation::parse("s3://bucket/data.grib2").unwrap();
    let remote = Arc::new(RemoteStore::new(
        source,
        Arc::clone(&store) as Arc<dyn object_store::ObjectStore>,
    ));
    let identity = setup.block_on(async { remote.head().await }).unwrap();
    setup.block_on(async {
        store
            .put(
                &ObjectPath::from("data.grib2"),
                PutPayload::from(vec![0_u8; identity.size() as usize + 1]),
            )
            .await
            .unwrap();
    });
    let runtime = Arc::new(StorageRuntime::spawn().unwrap());
    let error = match data::remote_grib2::open_remote(remote, identity, runtime) {
        Ok(_) => panic!("changed GRIB2 unexpectedly opened"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("precondition"));
}

#[test]
fn oversized_unindexed_grib2_refuses_before_full_object_scan() {
    let object_size = 64 * 1024 * 1024 + 1;
    let mut object = vec![0_u8; object_size];
    object[..4].copy_from_slice(b"GRIB");
    let store = Arc::new(InMemory::new());
    let setup = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    setup.block_on(async {
        store
            .put(
                &ObjectPath::from("unindexed.grib2"),
                PutPayload::from(object),
            )
            .await
            .unwrap();
    });
    let source = SourceLocation::parse("gs://bucket/unindexed.grib2").unwrap();
    let remote = Arc::new(RemoteStore::new(source, store));
    let identity = setup.block_on(async { remote.head().await }).unwrap();
    let runtime = Arc::new(StorageRuntime::spawn().unwrap());
    let error = match data::remote_grib2::open_remote(remote.clone(), identity, runtime) {
        Ok(_) => panic!("oversized unindexed object unexpectedly opened"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("bounded scan limit"));
    assert!(remote.requested_ranges().is_empty());
}
