use criterion::{Criterion, criterion_group, criterion_main};
use ncview_rs::{
    analysis::{mapping::screen_to_source, projection::ProjectionIndex},
    app::{AppState, Command},
    data::{DatasetFormat, DatasetMetadata, fixtures::regular_values, slice::Bounds},
    render::{
        colors::{Palette, ScaleMode, ScientificColorMap, normalize},
        landmask::Detail,
        map_background,
        raster::{rgb_raster, rgb_raster_with_options_for_view},
    },
};
use ratatui::layout::Rect;
use ratatui::{Terminal, backend::TestBackend};
use std::path::Path;
use std::{hint::black_box, sync::Arc};

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
        bencher.iter(|| black_box(map_background::render(160, 80, None, Detail::Global)))
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

fn raw_dimension_slice(criterion: &mut Criterion) {
    let Ok(path) = std::env::var("NCVIEW_RAW_DIMENSION_BENCH_FILE") else {
        return;
    };
    let Ok(source) = ncview_rs::data::open(Path::new(&path)) else {
        eprintln!("Skipping raw-dimension benchmark: cannot open {path}");
        return;
    };
    let metadata = source.metadata();
    let dimensions = &metadata.dimensions;
    let candidate = metadata.variables.iter().find_map(|variable| {
        if !variable.numeric || variable.dimensions.len() < 2 {
            return None;
        }
        let shape = variable
            .dimensions
            .iter()
            .map(|name| dimensions.iter().find(|dimension| dimension.name == *name))
            .collect::<Option<Vec<_>>>()?;
        let row = shape.iter().position(|dimension| dimension.length == 300)?;
        let col = shape
            .iter()
            .position(|dimension| dimension.length == 600 && dimension.name != shape[row].name)?;
        Some((variable.name.clone(), shape, row, col))
    });
    let Some((variable, shape, row, col)) = candidate else {
        eprintln!(
            "Skipping raw-dimension benchmark: no numeric 300-by-600 variable found in {path}"
        );
        return;
    };
    let row_name = shape[row].name.clone();
    let col_name = shape[col].name.clone();
    let fixed = shape
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != row && *index != col)
        .map(|(_, dimension)| (dimension.name.clone(), 0))
        .collect::<Vec<_>>();
    let request = ncview_rs::data::slice::SliceRequest {
        variable,
        time: 0,
        depth: 0,
        bounds: Bounds::new(0, 300, 0, 600).expect("constant bounds are valid"),
    };
    criterion.bench_function("raw_300x600_fixed_dimension_slice", |bencher| {
        bencher.iter(|| {
            black_box(
                source
                    .read_slice_on_axes(&request, Some(&row_name), Some(&col_name), &fixed)
                    .expect("benchmark slice read succeeds"),
            )
        })
    });
}

fn palette_picker_interaction(criterion: &mut Criterion) {
    let mut state = AppState::default();
    state.view.palette_catalog = (0..64)
        .map(|index| {
            Palette::Custom(Arc::new(ScientificColorMap {
                name: format!("Benchmark{index:02}"),
                colors: vec![[10, 20, 30], [120, 140, 160], [240, 230, 220]],
            }))
        })
        .collect();
    let metadata = DatasetMetadata {
        path: "benchmark".into(),
        format: DatasetFormat::NetCdf4,
        dimensions: Vec::new(),
        variables: Vec::new(),
    };
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("test terminal builds");
    criterion.bench_function("palette_picker_64_open_focus_preview", |bencher| {
        bencher.iter(|| {
            state.reduce(Command::OpenPalettePicker);
            state.reduce(Command::MovePalettePicker(32));
            terminal
                .draw(|frame| {
                    ncview_rs::ui::popup::render(
                        frame,
                        frame.area(),
                        &state.view,
                        &metadata,
                        &[],
                        "",
                        None,
                    );
                })
                .expect("picker render succeeds");
            let selected = state
                .view
                .palette_picker
                .as_ref()
                .map(|draft| &draft.focused_palette);
            black_box(selected);
            state.reduce(Command::CancelPalettePicker);
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().without_plots();
    targets = startup_scaffold, slice_and_raster, interactive_paths, raw_dimension_slice,
        palette_picker_interaction
}
criterion_main!(benches);
