use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

use bytes::Bytes;
use ncview_rs::{
    error::NcvError,
    storage::{
        location::SourceLocation,
        object_store::{ByteRange, RangeBlock, RangePolicy, RemoteStore},
        operation::{
            CancellationToken, LatestGeneration, OperationKind, OperationPhase, RemoteOperation,
            retry_delay,
        },
        range_cache::{CacheCategory, RangeCache, coalesce_adjacent},
    },
};
use object_store::{ObjectStoreExt, PutPayload, memory::InMemory, path::Path as ObjectPath};

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn validates_half_open_ranges_against_object_size() {
    assert!(ByteRange::new(0, 0, 0).is_ok());
    assert!(ByteRange::new(2, 6, 5).is_err());
    assert!(ByteRange::new(4, 3, 5).is_err());
    assert!(ByteRange::new(0, 6, 5).is_err());
}

#[test]
fn reads_exact_bytes_with_identity_metadata() {
    let rt = runtime();
    rt.block_on(async {
        let store = Arc::new(InMemory::new());
        store
            .put(
                &ObjectPath::from("data.bin"),
                PutPayload::from(&b"0123456789"[..]),
            )
            .await
            .unwrap();
        let source = SourceLocation::parse("s3://bucket/data.bin").unwrap();
        let remote = RemoteStore::new(source, store);
        let identity = remote.head().await.unwrap();
        let bytes = remote
            .read_range(&identity, ByteRange::new(2, 7, identity.size()).unwrap())
            .await
            .unwrap();
        assert_eq!(&bytes[..], b"23456");
        let stats = remote.stats();
        assert_eq!(stats.requests, 1);
        assert_eq!(stats.requested_bytes, 5);
        assert_eq!(stats.received_bytes, 5);
        assert_eq!(stats.retries, 0);
    });
}

#[test]
fn changed_object_identity_rejects_old_ranges() {
    let rt = runtime();
    rt.block_on(async {
        let store = Arc::new(InMemory::new());
        let path = ObjectPath::from("data.bin");
        store
            .put(&path, PutPayload::from(&b"first"[..]))
            .await
            .unwrap();
        let source = SourceLocation::parse("s3://bucket/data.bin").unwrap();
        let remote = RemoteStore::new(
            source,
            Arc::clone(&store) as Arc<dyn object_store::ObjectStore>,
        );
        let identity = remote.head().await.unwrap();
        store
            .put(&path, PutPayload::from(&b"second"[..]))
            .await
            .unwrap();
        let error = remote
            .read_range(&identity, ByteRange::new(0, 5, identity.size()).unwrap())
            .await
            .unwrap_err();
        assert!(matches!(error, NcvError::RemoteOperation { .. }));
    });
}

#[test]
fn cache_tracks_bytes_evicts_lru_and_invalidates_by_identity() {
    let rt = runtime();
    rt.block_on(async {
        let store = Arc::new(InMemory::new());
        store
            .put(
                &ObjectPath::from("data.bin"),
                PutPayload::from(&b"0123456789abcdef"[..]),
            )
            .await
            .unwrap();
        let source = SourceLocation::parse("s3://bucket/data.bin").unwrap();
        let remote = RemoteStore::new(source.clone(), store);
        let identity = remote.head().await.unwrap();
        let mut cache = RangeCache::new(8);
        let first_range = ByteRange::new(0, 4, identity.size()).unwrap();
        let second_range = ByteRange::new(4, 8, identity.size()).unwrap();
        let third_range = ByteRange::new(8, 12, identity.size()).unwrap();
        cache
            .insert(
                CacheCategory::Range,
                RangeBlock::new(&identity, first_range, Bytes::from_static(b"0123")).unwrap(),
            )
            .unwrap();
        cache
            .insert(
                CacheCategory::Range,
                RangeBlock::new(&identity, second_range, Bytes::from_static(b"4567")).unwrap(),
            )
            .unwrap();
        assert_eq!(cache.byte_usage(), 8);
        let usage = cache.working_set_usage();
        assert_eq!(usage.range_bytes, 8);
        assert_eq!(usage.decoded_bytes, 0);
        assert_eq!(usage.rendered_bytes, 0);
        assert_eq!(usage.total_bytes, 8);
        assert_eq!(
            &cache.get(&identity, first_range).unwrap().bytes()[..],
            b"0123"
        );
        cache
            .insert(
                CacheCategory::Range,
                RangeBlock::new(&identity, third_range, Bytes::from_static(b"89ab")).unwrap(),
            )
            .unwrap();
        assert!(cache.get(&identity, second_range).is_none());
        assert!(cache.get(&identity, first_range).is_some());
        assert_eq!(cache.byte_usage(), 8);
        cache.invalidate_identity(identity.cache_token());
        assert_eq!(cache.byte_usage(), 0);
        assert!(cache.get(&identity, first_range).is_none());
        assert_eq!(source.object_key(), "data.bin");
    });
}

#[test]
fn cache_evicts_lower_priority_working_sets_before_metadata() {
    let rt = runtime();
    rt.block_on(async {
        let store = Arc::new(InMemory::new());
        store
            .put(
                &ObjectPath::from("data.bin"),
                PutPayload::from(&b"0123456789ab"[..]),
            )
            .await
            .unwrap();
        let source = SourceLocation::parse("s3://bucket/data.bin").unwrap();
        let remote = RemoteStore::new(source, store);
        let identity = remote.head().await.unwrap();
        let mut cache = RangeCache::new(8);
        let metadata_range = ByteRange::new(0, 4, 12).unwrap();
        let rendered_range = ByteRange::new(4, 8, 12).unwrap();
        let replacement_range = ByteRange::new(8, 12, 12).unwrap();
        cache
            .insert(
                CacheCategory::Metadata,
                RangeBlock::new(&identity, metadata_range, Bytes::from_static(b"meta")).unwrap(),
            )
            .unwrap();
        cache
            .insert(
                CacheCategory::Rendered,
                RangeBlock::new(&identity, rendered_range, Bytes::from_static(b"view")).unwrap(),
            )
            .unwrap();
        cache
            .insert(
                CacheCategory::Range,
                RangeBlock::new(&identity, replacement_range, Bytes::from_static(b"next")).unwrap(),
            )
            .unwrap();
        assert!(cache.get(&identity, metadata_range).is_some());
        assert!(cache.get(&identity, rendered_range).is_none());
    });
}

#[test]
fn coalesces_only_adjacent_ranges_within_request_bound() {
    let ranges = vec![
        ByteRange::new(0, 2, 20).unwrap(),
        ByteRange::new(2, 5, 20).unwrap(),
        ByteRange::new(8, 9, 20).unwrap(),
    ];
    let merged = coalesce_adjacent(ranges, 6).unwrap();
    assert_eq!(
        merged,
        vec![
            ByteRange::new(0, 5, 20).unwrap(),
            ByteRange::new(8, 9, 20).unwrap()
        ]
    );
}

#[test]
fn storage_runtime_polls_jobs_off_the_calling_thread() {
    let runtime = ncview_rs::storage::StorageRuntime::spawn().unwrap();
    let result = runtime.submit(async { 2_u32 + 3 }).unwrap().recv().unwrap();
    assert_eq!(result, 5);
}

#[test]
fn storage_runtime_drops_cancelled_provider_futures() {
    let runtime = ncview_rs::storage::StorageRuntime::spawn().unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let receiver = runtime
        .submit_cancellable(
            async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                7_u32
            },
            Arc::clone(&cancelled),
        )
        .unwrap();
    std::thread::sleep(Duration::from_millis(20));
    cancelled.store(true, Ordering::Release);
    assert_eq!(receiver.recv().unwrap(), None);
}

#[test]
fn storage_runtime_keeps_provider_jobs_within_the_concurrency_bound() {
    let runtime = ncview_rs::storage::StorageRuntime::spawn().unwrap();
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let receivers = (0..8)
        .map(|_| {
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            runtime
                .submit(async move {
                    let current = active.fetch_add(1, Ordering::AcqRel) + 1;
                    maximum.fetch_max(current, Ordering::AcqRel);
                    tokio::task::yield_now().await;
                    active.fetch_sub(1, Ordering::AcqRel);
                    current
                })
                .unwrap()
        })
        .collect::<Vec<_>>();
    for receiver in receivers {
        receiver.recv().unwrap();
    }
    assert_eq!(maximum.load(Ordering::Acquire), 1);
}

#[test]
fn operation_state_tracks_progress_cancellation_and_latest_generation() {
    let source = SourceLocation::parse("gs://bucket/data.bin").unwrap();
    let mut operation = RemoteOperation::new(
        7,
        ncview_rs::app::Generation(3),
        OperationKind::Slice,
        source,
    );
    assert_eq!(operation.phase, OperationPhase::Queued);
    operation.advance(OperationPhase::Fetching, "fetching selected range");
    operation.progress.requested_bytes = 128;
    operation.progress.received_bytes = 64;
    operation.progress.completed_units = Some(1);
    operation.progress.total_units = Some(4);
    operation.progress.cancellable = true;
    operation.cancel();
    assert_eq!(operation.phase, OperationPhase::Fetching);
    assert!(operation.cancellation.is_cancelled());
    assert_eq!(operation.progress.completed_units, Some(1));
    assert_eq!(operation.progress.total_units, Some(4));
    assert!(operation.progress.cancellable);

    let token = CancellationToken::new();
    let clone = token.clone();
    clone.cancel();
    assert!(token.is_cancelled());

    let latest = LatestGeneration::default();
    assert!(latest.accept(ncview_rs::app::Generation(3)));
    assert!(!latest.accept(ncview_rs::app::Generation(2)));
    assert!(latest.accept(ncview_rs::app::Generation(4)));

    let state = ncview_rs::storage::operation::OperationState::default();
    let tracked = state.begin(
        ncview_rs::app::Generation(5),
        OperationKind::Slice,
        SourceLocation::parse("s3://bucket/data.bin").unwrap(),
    );
    let progress = ncview_rs::storage::operation::Progress {
        message: "reading".into(),
        completed_units: Some(2),
        total_units: Some(3),
        requested_bytes: 10,
        received_bytes: 8,
        retries: 1,
        cancellable: true,
    };
    assert!(state.update(tracked.id, OperationPhase::Fetching, progress.clone()));
    assert_eq!(state.active().unwrap().progress, progress);
    assert!(!state.update(
        tracked.id.saturating_add(1),
        OperationPhase::Failed,
        progress
    ));
    assert!(state.cancel(tracked.id));
    assert_eq!(state.active().unwrap().phase, OperationPhase::Cancelled);
    assert!(state.finish(tracked.id, OperationPhase::Complete));
}

#[test]
fn rejects_short_cached_blocks_before_they_can_enter_the_working_set() {
    let rt = runtime();
    rt.block_on(async {
        let store = Arc::new(InMemory::new());
        store
            .put(
                &ObjectPath::from("data.bin"),
                PutPayload::from(&b"0123456789"[..]),
            )
            .await
            .unwrap();
        let source = SourceLocation::parse("s3://bucket/data.bin").unwrap();
        let remote = RemoteStore::new(source, store);
        let identity = remote.head().await.unwrap();
        let range = ByteRange::new(0, 4, identity.size()).unwrap();
        assert!(RangeBlock::new(&identity, range, Bytes::from_static(b"012")).is_err());
    });
}

#[test]
fn retry_backoff_is_bounded_and_classification_is_conservative() {
    let base = Duration::from_millis(50);
    let maximum = Duration::from_millis(200);
    assert_eq!(retry_delay(0, base, maximum), Duration::from_millis(50));
    assert_eq!(retry_delay(2, base, maximum), maximum);
    assert!(ncview_rs::storage::operation::is_retryable_message(
        "request timed out"
    ));
    assert!(!ncview_rs::storage::operation::is_retryable_message(
        "permission denied"
    ));
}

#[test]
fn range_policy_refuses_oversized_requests_before_provider_io() {
    let rt = runtime();
    rt.block_on(async {
        let store = Arc::new(InMemory::new());
        store
            .put(
                &ObjectPath::from("data.bin"),
                PutPayload::from(&b"0123456789"[..]),
            )
            .await
            .unwrap();
        let source = SourceLocation::parse("s3://bucket/data.bin").unwrap();
        let policy = RangePolicy {
            max_request_bytes: 2,
            ..RangePolicy::default()
        };
        let remote = RemoteStore::with_policy(source, store, policy);
        let identity = remote.head().await.unwrap();
        let error = remote
            .read_range(&identity, ByteRange::new(0, 3, identity.size()).unwrap())
            .await
            .unwrap_err();
        assert!(matches!(error, NcvError::RemoteOperation { .. }));
    });
}

#[test]
fn diagnostics_include_only_safe_transfer_metadata() {
    let rt = runtime();
    rt.block_on(async {
        let store = Arc::new(InMemory::new());
        store
            .put(
                &ObjectPath::from("data.bin"),
                PutPayload::from(&b"0123456789"[..]),
            )
            .await
            .unwrap();
        let source = SourceLocation::parse("s3://bucket/data.bin").unwrap();
        let remote = RemoteStore::new(source, store);
        let identity = remote.head().await.unwrap();
        remote
            .read_range(&identity, ByteRange::new(0, 4, identity.size()).unwrap())
            .await
            .unwrap();
        let diagnostics = remote.diagnostics(&identity, "slice");
        assert_eq!(diagnostics.operation, "slice");
        assert_eq!(diagnostics.object_size, 10);
        assert_eq!(diagnostics.requested_bytes, 4);
        assert_eq!(diagnostics.received_bytes, 4);
        assert!(!format!("{diagnostics:?}").contains("Authorization"));
    });
}
