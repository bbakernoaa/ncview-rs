use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use ncview_rs::{
    app::{AppState, Command, Generation},
    data::{
        self,
        slice::{Bounds, Slice2D, Validity},
    },
    error::NcvError,
    storage::location::SourceLocation,
};
use ndarray::{Array2, arr2};
use object_store::{
    ObjectStoreExt, PutPayload,
    memory::InMemory,
    path::Path as ObjectPath,
    throttle::{ThrottleConfig, ThrottledStore},
};

#[test]
fn loading_progress_is_published_before_a_delayed_remote_read() {
    let bytes = include_bytes!("fixtures/regular.nc4");
    let inner = InMemory::new();
    let setup = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    setup.block_on(async {
        inner
            .put(
                &ObjectPath::from("regular.nc4"),
                PutPayload::from(bytes.as_slice()),
            )
            .await
            .unwrap();
    });
    let store = Arc::new(ThrottledStore::new(
        inner,
        ThrottleConfig {
            wait_get_per_call: Duration::from_millis(250),
            ..ThrottleConfig::default()
        },
    ));
    let source = SourceLocation::parse("s3://bucket/regular.nc4").unwrap();
    let (progress_tx, progress_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = data::remote::open_remote_with_store_progress(source, store, &|message| {
            progress_tx.send(message.to_owned()).is_ok()
        });
        let _ = result_tx.send(result);
    });

    assert_eq!(
        progress_rx
            .recv_timeout(Duration::from_millis(100))
            .unwrap(),
        "connecting to remote object"
    );
    let opened = result_rx
        .recv_timeout(Duration::from_secs(3))
        .unwrap()
        .unwrap();
    assert_eq!(opened.metadata().format, data::DatasetFormat::NetCdf4);
}

#[test]
fn cancellation_callback_stops_remote_open_before_provider_io() {
    let source = SourceLocation::parse("gs://bucket/missing.nc4").unwrap();
    let store = Arc::new(InMemory::new());
    let progress = |_message: &str| false;
    let error = match data::remote::open_remote_with_store_progress(source, store, &progress) {
        Ok(_) => panic!("cancelled open unexpectedly succeeded"),
        Err(error) => error,
    };
    assert!(matches!(error, NcvError::WorkerStopped));
}

#[test]
fn missing_remote_object_reports_safe_head_context() {
    let source = SourceLocation::parse("az://container/missing.nc4").unwrap();
    let store = Arc::new(InMemory::new());
    let error = match data::remote::open_remote_with_store(source, store) {
        Ok(_) => panic!("missing object unexpectedly opened"),
        Err(error) => error,
    };
    let message = error.to_string();
    assert!(message.contains("HEAD"));
    assert!(message.contains("az://container/missing.nc4"));
}

#[test]
fn unsupported_remote_format_is_rejected_after_bounded_sniff() {
    let store = Arc::new(InMemory::new());
    let setup = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    setup.block_on(async {
        store
            .put(
                &ObjectPath::from("unknown.bin"),
                PutPayload::from(&b"not-a-scientific-object"[..]),
            )
            .await
            .unwrap();
    });
    let source = SourceLocation::parse("s3://bucket/unknown.bin").unwrap();
    let error = match data::remote::open_remote_with_store(source, store) {
        Ok(_) => panic!("unsupported object unexpectedly opened"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("neither a supported GRIB2 object")
    );
}

#[test]
fn rapid_time_level_zoom_pan_and_resize_navigation_rejects_stale_results() {
    let mut state = AppState::default();
    let previous = Slice2D::new(
        arr2(&[[1.0, 2.0], [3.0, 4.0]]),
        Array2::from_elem((2, 2), Validity::Finite),
        Bounds::new(0, 2, 0, 2).unwrap(),
    )
    .unwrap();
    state.set_slice(previous.clone());
    state.view.full_bounds = Some(Bounds::new(0, 2, 0, 2).unwrap());
    state.view.time_length = 4;
    state.view.depth_length = 3;
    state.view.generation = Generation(10);
    state.view.plot_generation = Generation(20);

    for command in [
        Command::MoveTime(1),
        Command::MoveDepth(1),
        Command::Zoom(Bounds::new(0, 1, 0, 1).unwrap()),
        Command::Pan { rows: 1, cols: 1 },
        Command::ResetZoom,
        Command::Resize {
            width: 120,
            height: 40,
        },
    ] {
        let stale = state.view.generation;
        state.reduce(command);
        let current = state.next_generation();
        assert!(current > stale);
        assert!(!state.accept_slice(stale, previous.clone()));
        assert_eq!(state.view.slice.as_ref().unwrap().values[[0, 0]], 1.0);
    }

    let stale_plot = state.view.plot_generation;
    state.next_plot_generation();
    assert!(!state.accept_plot(
        stale_plot,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        "stale plot".into(),
    ));
}
