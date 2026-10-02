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

#[path = "support/fixtures.rs"]
mod fixtures;

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

fn large_map_selection(criterion: &mut Criterion) {
    let slice = fixtures::large_regular_slice();
    let raster = rgb_raster_with_options_for_view(
        &slice,
        Palette::Viridis,
        None,
        None,
        false,
        ScaleMode::Linear,
        320,
        160,
        None,
    );
    criterion.bench_function("large_map_stable_base_raster_2048x1024", |bencher| {
        bencher.iter(|| {
            black_box(rgb_raster_with_options_for_view(
                &slice,
                Palette::Viridis,
                None,
                None,
                false,
                ScaleMode::Linear,
                320,
                160,
                None,
            ))
        })
    });
    criterion.bench_function("large_map_selection_only_raster_2048x1024", |bencher| {
        let mut selection = 0_usize;
        let mut terminal = Terminal::new(TestBackend::new(80, 40)).expect("test terminal builds");
        bencher.iter(|| {
            selection = selection.wrapping_add(1);
            let point = black_box(selection % 2);
            terminal
                .draw(|frame| {
                    frame.render_widget(
                        ratatui::widgets::Paragraph::new(if point == 0 { "◆" } else { " " }),
                        Rect::new(40, 20, 1, 1),
                    );
                })
                .expect("selection overlay renders");
        })
    });
    criterion.bench_function("large_map_rgb_payload_base64_320x160", |bencher| {
        bencher.iter(|| black_box(base64_simd::STANDARD.encode_to_string(raster.as_raw())))
    });
}

fn repeated_coordinate_read(criterion: &mut Criterion) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/curvilinear.nc4");
    let Ok(source) = ncview_rs::data::open(&path) else {
        eprintln!(
            "Skipping coordinate-cache benchmark: cannot open {}",
            path.display()
        );
        return;
    };
    let Some(variable) = source
        .metadata()
        .variables
        .iter()
        .find(|variable| variable.numeric && variable.dimensions.len() >= 2)
        .map(|variable| variable.name.clone())
    else {
        return;
    };
    let dimensions = &source.metadata().dimensions;
    let shape = source
        .metadata()
        .variables
        .iter()
        .find(|candidate| candidate.name == variable)
        .map(|candidate| {
            candidate
                .dimensions
                .iter()
                .filter_map(|name| dimensions.iter().find(|dimension| dimension.name == *name))
                .map(|dimension| dimension.length)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if shape.len() < 2 {
        return;
    }
    let rows = shape[shape.len() - 2];
    let cols = shape[shape.len() - 1];
    let request = ncview_rs::data::slice::SliceRequest {
        variable,
        time: 0,
        depth: 0,
        bounds: Bounds::new(0, rows, 0, cols).expect("fixture bounds"),
    };
    criterion.bench_function("curvilinear_coordinates_repeated_read", |bencher| {
        bencher.iter(|| black_box(source.read_slice(&request).expect("slice read")))
    });
}

fn catalog_search(criterion: &mut Criterion) {
    let variables = fixtures::variable_catalog(10_000);
    criterion.bench_function(
        "variable_catalog_search_10000_uncached_reference",
        |bencher| {
            bencher.iter(|| {
                let query = "variable_099".to_ascii_lowercase();
                let mut matched = variables
                    .iter()
                    .filter(|variable| {
                        let normalized = variable.name.to_ascii_lowercase();
                        let mut chars = normalized.chars();
                        query
                            .chars()
                            .all(|needle| chars.by_ref().any(|candidate| candidate == needle))
                    })
                    .collect::<Vec<_>>();
                matched.sort_by(|left, right| {
                    left.name
                        .to_ascii_lowercase()
                        .cmp(&right.name.to_ascii_lowercase())
                        .then_with(|| left.name.cmp(&right.name))
                });
                black_box(matched)
            })
        },
    );
    criterion.bench_function("variable_catalog_search_10000", |bencher| {
        bencher.iter(|| {
            black_box(ncview_rs::ui::sidebar::filter_variables(
                &variables,
                "variable_099",
            ))
        })
    });
    let palettes = fixtures::palette_catalog(256);
    criterion.bench_function("palette_catalog_search_256", |bencher| {
        bencher.iter(|| {
            black_box(ncview_rs::app::palette_catalog_matches(
                &palettes,
                "benchmark25",
            ))
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().without_plots();
    targets = startup_scaffold, slice_and_raster, interactive_paths, raw_dimension_slice,
        palette_picker_interaction, large_map_selection, repeated_coordinate_read,
        catalog_search
}
criterion_main!(benches);
