use std::hint::black_box;
use std::sync::Arc;

use bytes::Bytes;
use criterion::{Criterion, criterion_group, criterion_main};
use ncview_rs::storage::{
    location::SourceLocation,
    object_store::{ByteRange, RangeBlock, RemoteStore},
    range_cache::{CacheCategory, RangeCache, coalesce_adjacent},
};
use object_store::{ObjectStoreExt, PutPayload, memory::InMemory, path::Path as ObjectPath};

fn benchmark_cache(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("benchmark runtime");
    let store: Arc<dyn object_store::ObjectStore> = Arc::new(InMemory::new());
    let source = SourceLocation::parse("s3://bench/data.bin").expect("source");
    let remote = RemoteStore::new(source, Arc::clone(&store));
    let identity = rt.block_on(async {
        store
            .put(
                &ObjectPath::from("data.bin"),
                PutPayload::from(vec![7_u8; 1024 * 1024]),
            )
            .await
            .expect("fixture");
        remote.head().await.expect("identity")
    });

    c.bench_function("remote_range_cache_lookup", |b| {
        let mut cache = RangeCache::new(2 * 1024 * 1024);
        let range = ByteRange::new(0, 1024 * 1024, identity.size()).expect("range");
        cache
            .insert(
                CacheCategory::Range,
                RangeBlock::new(&identity, range, Bytes::from(vec![7_u8; 1024 * 1024]))
                    .expect("block"),
            )
            .expect("cache insert");
        b.iter(|| black_box(cache.get(&identity, range).map(|block| block.bytes().len())));
    });

    c.bench_function("remote_range_fetch", |b| {
        let range = ByteRange::new(0, 1024 * 1024, identity.size()).expect("range");
        b.iter(|| {
            let bytes = rt
                .block_on(remote.read_range(&identity, range))
                .expect("range read");
            black_box(bytes.len());
        });
    });

    c.bench_function("adjacent_range_coalescing", |b| {
        b.iter(|| {
            let ranges = vec![
                ByteRange::new(0, 256 * 1024, identity.size()).expect("range"),
                ByteRange::new(256 * 1024, 512 * 1024, identity.size()).expect("range"),
                ByteRange::new(768 * 1024, 1024 * 1024, identity.size()).expect("range"),
            ];
            black_box(coalesce_adjacent(ranges, 1024 * 1024).expect("coalesce"));
        });
    });
}

criterion_group!(remote_streaming, benchmark_cache);
criterion_main!(remote_streaming);
