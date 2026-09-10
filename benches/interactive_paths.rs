use criterion::{Criterion, criterion_group, criterion_main};
use ncview_rs::{
    analysis::{mapping::screen_to_source, projection::ProjectionIndex},
    data::{fixtures::regular_values, slice::Bounds},
    render::{
        colors::{Palette, normalize},
        raster::rgb_raster,
    },
};
use ratatui::layout::Rect;
use std::hint::black_box;

fn startup_scaffold(criterion: &mut Criterion) {
    criterion.bench_function("startup_scaffold", |bencher| {
        bencher.iter(|| black_box(ncview_rs::app::AppState::default()))
    });
}

fn slice_and_raster(criterion: &mut Criterion) {
    let slice = regular_values(128, 128).unwrap();
    criterion.bench_function("slice_conversion", |bencher| {
        bencher.iter(|| black_box(slice.permuted_axes().unwrap()))
    });
    criterion.bench_function("rasterization", |bencher| {
        bencher.iter(|| black_box(rgb_raster(&slice, Palette::Viridis)))
    });
    criterion.bench_function("normalization", |bencher| {
        bencher.iter(|| black_box(normalize(0.42, 0.0, 1.0)))
    });
}

fn interactive_paths(criterion: &mut Criterion) {
    let bounds = Bounds::new(0, 128, 0, 128).unwrap();
    let rect = Rect::new(0, 0, 80, 40);
    criterion.bench_function("mapping", |bencher| {
        bencher.iter(|| black_box(screen_to_source(20, 20, rect, bounds)))
    });
    let lat = vec![0.0; 128 * 128];
    let lon = (0..128 * 128)
        .map(|index| index as f64 % 360.0 - 180.0)
        .collect::<Vec<_>>();
    let index = ProjectionIndex::build(&lat, &lon, 128);
    criterion.bench_function("kdtree_query", |bencher| {
        bencher.iter(|| black_box(index.nearest(0.0, 0.0)))
    });
}

criterion_group!(
    benches,
    startup_scaffold,
    slice_and_raster,
    interactive_paths
);
criterion_main!(benches);
