use criterion::{Criterion, criterion_group, criterion_main};
use ncview_rs::{
    analysis::{mapping::screen_to_source, projection::ProjectionIndex},
    data::{fixtures::regular_values, slice::Bounds},
    render::{
        colors::{Palette, ScaleMode, normalize},
        landmask::Detail,
        map_background,
        raster::{rgb_raster, rgb_raster_with_options_for_view},
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
    let large_slice = regular_values(512, 512).unwrap();
    criterion.bench_function("slice_conversion", |bencher| {
        bencher.iter(|| black_box(slice.permuted_axes().unwrap()))
    });
    criterion.bench_function("rasterization", |bencher| {
        bencher.iter(|| black_box(rgb_raster(&slice, Palette::Viridis)))
    });
    criterion.bench_function("large_viewport_rasterization", |bencher| {
        bencher.iter(|| {
            black_box(rgb_raster_with_options_for_view(
                &large_slice,
                Palette::Viridis,
                None,
                None,
                true,
                ScaleMode::Linear,
                160,
                80,
                None,
            ))
        })
    });
    criterion.bench_function("map_backdrop_rendering", |bencher| {
        bencher.iter(|| {
            black_box(map_background::render(160, 80, None, Detail::Global))
        })
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

criterion_group! {
    name = benches;
    config = Criterion::default().without_plots();
    targets = startup_scaffold, slice_and_raster, interactive_paths
}
criterion_main!(benches);
