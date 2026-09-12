use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, Instant},
};

use clap::{CommandFactory, Parser, Subcommand};
use crossterm::{
    event, execute,
    terminal::{LeaveAlternateScreen, disable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, layout::Rect};

use ncview_rs::{
    analysis::mapping::screen_to_source,
    app::{
        AppState, AxisField, ColorScaleScope, Command, LimitField, Overlay, PlotSeries,
        TimelinePoint,
    },
    data::{
        self, AxisRole, DatasetFormat, DatasetMetadata, Variable,
        grib2_manifest::{self, ManifestFormat},
        slice::{Bounds, SliceRequest},
    },
    events::input,
    render::protocol::GraphicsRenderer,
    ui::{dashboard, layout as dashboard_layout},
};

#[derive(Debug, Parser)]
#[command(
    name = "ncv",
    version,
    about = "Terminal-native NetCDF and GRIB2 scientific data viewer"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,
    /// One or more NetCDF-4 or GRIB2 datasets to inspect. Shell globs are supported.
    #[arg(value_name = "DATASET", num_args = 0..)]
    dataset: Vec<String>,
}

#[derive(Debug, Subcommand)]
enum CliCommand {
    /// Create a Kerchunk-compatible GRIB2 reference manifest from a `.idx` sidecar.
    Manifest {
        /// Output profile. `virtualizarr` emits a VirtualiZarr-consumable Kerchunk profile.
        #[arg(long, value_parser = ["kerchunk", "virtualizarr"])]
        format: String,
        /// GRIB2 source object.
        #[arg(long)]
        input: String,
        /// Matching NOAA-style `.idx` sidecar.
        #[arg(long)]
        idx: String,
        /// Manifest destination JSON.
        #[arg(long)]
        output: String,
        /// URI to place in byte-range references instead of the local source path.
        #[arg(long)]
        source_uri: Option<String>,
        /// Treat warnings and mismatches as errors.
        #[arg(long)]
        strict: bool,
    },
}

fn setup_panic_hook() {
    let _ = color_eyre::install();
    human_panic::setup_panic!();
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(
            stdout,
            crossterm::cursor::Show,
            crossterm::event::DisableMouseCapture,
            LeaveAlternateScreen
        );
        let _ = io::Write::flush(&mut stdout);
        prev_hook(panic_info);
    }));
}

fn main() -> ExitCode {
    setup_panic_hook();
    let cli = Cli::parse();
    if let Some(CliCommand::Manifest {
        format,
        input,
        idx,
        output,
        source_uri,
        strict,
    }) = cli.command
    {
        let manifest_format = match format.as_str() {
            "kerchunk" => ManifestFormat::Kerchunk,
            "virtualizarr" => ManifestFormat::Virtualizarr,
            _ => unreachable!("clap validates manifest format"),
        };
        return match grib2_manifest::write_manifest(
            Path::new(&input),
            Path::new(&idx),
            Path::new(&output),
            manifest_format,
            source_uri.as_deref(),
            strict,
        ) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("ncv manifest: {error}");
                ExitCode::from(2)
            }
        };
    }
    if cli.dataset.is_empty() {
        let mut command = Cli::command();
        let _ = command.print_help();
        println!();
        return ExitCode::SUCCESS;
    }
    if let Err(error) = run(&cli.dataset) {
        eprintln!("ncv: {error}");
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

fn run(datasets: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let sources = datasets
        .iter()
        .map(|dataset| data::open(dataset).map_err(|error| format!("{dataset}: {error}")))
        .collect::<Result<Vec<_>, _>>()?;
    let mut active_file = 0usize;
    let initial_source = sources[active_file].as_ref();
    let (stdout_tx, stdout_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        use std::io::Write;
        while let Ok(bytes) = stdout_rx.recv() {
            let _ = handle.write_all(&bytes);
            let _ = handle.flush();
        }
    });

    struct ChannelWriter {
        tx: std::sync::mpsc::Sender<Vec<u8>>,
        buffer: Vec<u8>,
    }

    impl std::io::Write for ChannelWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.buffer.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            if !self.buffer.is_empty() {
                let bytes = std::mem::take(&mut self.buffer);
                let _ = self.tx.send(bytes);
            }
            Ok(())
        }
    }

    let channel_writer = ChannelWriter {
        tx: stdout_tx,
        buffer: Vec::new(),
    };

    let mut session = ncview_rs::events::terminal::TerminalSession::enter()?;
    let backend = CrosstermBackend::new(channel_writer);
    let mut terminal = Terminal::new(backend)?;
    let mut graphics = GraphicsRenderer::probe();
    let mut chart_graphics = graphics.secondary();
    let mut state = state_for_source(initial_source);
    select_initial_variable(&mut state, initial_source.metadata());
    configure_timeline(&mut state, &sources, active_file);
    load_selected(&mut state, &sources, active_file);
    if state.view.slice.is_none() {
        state.view.status = format!(
            "opened {} variable(s); no plottable data",
            state.variables.len()
        );
    }
    let mut dirty = true;
    let mut last_render = Instant::now();
    let frame_budget = Duration::from_millis(33); // ~30 FPS throttle max
    let mut last_playback_tick = Instant::now();

    loop {
        let source = sources[active_file].as_ref();
        if state.view.playing
            && last_playback_tick.elapsed()
                >= Duration::from_secs_f32(1.0 / state.view.playback_speed)
        {
            state.reduce(Command::TickPlayback);
            load_selected(&mut state, &sources, active_file);
            last_playback_tick = Instant::now();
            dirty = true;
        }

        if dirty && last_render.elapsed() >= frame_budget {
            terminal.draw(|frame| {
                dashboard::render_with_search_and_image(
                    frame,
                    frame.area(),
                    &state.view,
                    &display_dataset_name(&datasets[active_file], active_file, datasets.len()),
                    source.metadata(),
                    &state.variable_query,
                    state.view.variable_search_active,
                    Some(&mut graphics),
                    Some(&mut chart_graphics),
                )
            })?;
            last_render = Instant::now();
            dirty = false;
        }

        // Compute poll timeout: idle if not playing, else remaining time to next playback tick/render
        let poll_timeout = if state.view.playing {
            let playback_interval = Duration::from_secs_f32(1.0 / state.view.playback_speed);
            let elapsed = last_playback_tick.elapsed();
            playback_interval
                .saturating_sub(elapsed)
                .min(Duration::from_millis(33))
        } else if dirty {
            frame_budget.saturating_sub(last_render.elapsed())
        } else {
            Duration::from_millis(100)
        };

        if event::poll(poll_timeout)?
            && let Some(command) = input::command_from_event_with_search(
                event::read()?,
                state.view.variable_search_active,
            )
        {
            dirty = true;
            let size = terminal.size()?;
            let command = translate_mouse(
                command,
                Rect::new(0, 0, size.width, size.height),
                source.metadata(),
                &state.view,
                &state.variable_query,
                Some(&graphics),
            );
            if matches!(command, Command::Quit)
                && state.view.overlay.is_none()
                && !state.view.variable_search_active
            {
                break;
            }
            let file_delta = match command {
                Command::PreviousFile => Some(-1isize),
                Command::NextFile => Some(1isize),
                _ => None,
            };
            if let Some(delta) = file_delta {
                active_file = bounded_file_index(active_file, delta, sources.len());
                let source = sources[active_file].as_ref();
                state = state_for_source(source);
                select_initial_variable(&mut state, source.metadata());
                configure_timeline(&mut state, &sources, active_file);
                load_selected(&mut state, &sources, active_file);
                state.view.status = format!(
                    "opened file {}/{}: {}",
                    active_file + 1,
                    datasets.len(),
                    datasets[active_file]
                );
                continue;
            }
            let source = sources[active_file].as_ref();
            let dataset = &datasets[active_file];
            let activate_point = matches!(command, Command::ActivatePoint);
            let cycle_image_filter = matches!(command, Command::CycleImageFilter);
            let export_current = matches!(command, Command::ExportCurrent);
            let refresh_time_series = activate_point
                || matches!(command, Command::OpenPlot)
                || matches!(
                    command,
                    Command::MoveTime(_)
                        | Command::SetTime(_)
                        | Command::MoveDepth(_)
                        | Command::CyclePlotAxis(_)
                        | Command::SetPlotKind(_)
                        | Command::TogglePointSelection
                        | Command::SelectVariable(_)
                        | Command::SelectVariableAt(_)
                        | Command::SubmitVariableSearch
                        | Command::ExecuteCommandPalette
                        | Command::SetAxes { .. }
                );
            let reload = matches!(
                command,
                Command::SelectVariable(_)
                    | Command::SelectVariableAt(_)
                    | Command::MoveTime(_)
                    | Command::SetTime(_)
                    | Command::MoveDepth(_)
                    | Command::TickPlayback
                    | Command::SubmitVariableSearch
                    | Command::ExecuteCommandPalette
                    | Command::Zoom(_)
                    | Command::ResetZoom
                    | Command::Pan { .. }
                    | Command::SetAxes { .. }
                    | Command::AutomaticLimits
                    | Command::ToggleColorScaleScope
                    | Command::ToggleScale
            );
            let axis_submit = matches!(command, Command::ActivatePoint)
                && state.view.overlay == Some(Overlay::Axis);
            let reconfigure_timeline = matches!(
                command,
                Command::SelectVariable(_)
                    | Command::SelectVariableAt(_)
                    | Command::SubmitVariableSearch
                    | Command::ExecuteCommandPalette
                    | Command::SetAxes { .. }
            ) || axis_submit;
            let point_target = match &command {
                Command::HoverPoint { row, col, .. } | Command::SelectPoint { row, col } => {
                    Some((*row, *col))
                }
                Command::TogglePointSelection => state
                    .view
                    .hover_point
                    .as_ref()
                    .map(|point| (point.row, point.col)),
                Command::ActivatePoint => state.view.selected_point,
                _ => None,
            };
            let _ = state.reduce(command);
            if cycle_image_filter {
                if graphics.cycle_filter() {
                    state.view.status = format!("image interpolation: {}", graphics.filter_label());
                } else {
                    state.view.status =
                        "scientific rendering locked; set NCVIEW_SCIENTIFIC_RENDERING=0 to enable interpolation".into();
                }
            }
            if export_current {
                match export_current_slice(&state, dataset, source.metadata()) {
                    Ok(path) => state.view.status = format!("exported {}", path.display()),
                    Err(error) => state.view.status = format!("export failed: {error}"),
                }
            }
            if let Some((row, col)) = point_target
                && let Some(variable) = state.view.selected_variable.as_deref()
            {
                let coordinates = source.point_coordinates(variable, row, col);
                if let Some(point) = state.view.hover_point.as_mut()
                    && point.row == row
                    && point.col == col
                {
                    point.latitude = coordinates.latitude;
                    point.longitude = coordinates.longitude;
                }
                if state.view.selected_point == Some((row, col)) {
                    state.view.selected_coordinates = coordinates;
                }
            }
            if reload || axis_submit {
                if reconfigure_timeline {
                    configure_timeline(&mut state, &sources, active_file);
                }
                load_selected(&mut state, &sources, active_file);
            }
            if refresh_time_series
                && matches!(
                    state.view.overlay,
                    Some(Overlay::TimeSeries | Overlay::Plot)
                )
            {
                load_time_series(&mut state, &sources, active_file);
            }
            if let Some(point) = state.view.timeline.get(state.view.time_index) {
                active_file = point.source_index;
            }
        }
    }
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    session.restore()?;
    Ok(())
}

fn state_for_source(source: &dyn data::DataSource) -> AppState {
    AppState {
        variables: source
            .metadata()
            .variables
            .iter()
            .filter(|variable| variable.numeric && variable.dimensions.len() >= 2)
            .cloned()
            .collect(),
        ..AppState::default()
    }
}

fn display_dataset_name(path: &str, index: usize, total: usize) -> String {
    if total <= 1 {
        path.to_string()
    } else {
        format!("{path}  [{}/{}]", index + 1, total)
    }
}

fn bounded_file_index(index: usize, delta: isize, length: usize) -> usize {
    if length == 0 {
        return 0;
    }
    let next = index as isize + delta;
    next.clamp(0, length.saturating_sub(1) as isize) as usize
}

fn export_current_slice(
    state: &AppState,
    dataset: &str,
    metadata: &DatasetMetadata,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let slice = state
        .view
        .slice
        .as_ref()
        .ok_or("no slice is currently loaded")?;
    let variable_name = state
        .view
        .selected_variable
        .as_deref()
        .ok_or("no variable is currently selected")?;
    let metadata_variable = metadata
        .variables
        .iter()
        .find(|variable| variable.name == variable_name);
    let limits = state
        .view
        .limits
        .or_else(|| slice.statistics.map(|stats| (stats.min, stats.max)))
        .filter(|(min, max)| min.is_finite() && max.is_finite() && max > min)
        .ok_or("current slice has no finite color range")?;
    let raster = ncview_rs::render::raster::rgb_raster_with_options(
        slice,
        state.view.palette.clone(),
        Some(limits),
        state.view.filter_range,
        state.view.show_land_borders,
        state.view.scale_mode,
        None,
        state.view.selected_point,
    );
    let directory = env::var_os("NCVIEW_EXPORT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&directory)?;
    let dataset_stem = Path::new(dataset)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(sanitize_filename)
        .unwrap_or_else(|| "ncv".into());
    let stem = sanitize_filename(variable_name);
    let stem = format!(
        "{dataset_stem}_{stem}_t{:04}_z{:04}",
        state.view.time_index, state.view.depth_index
    );
    let svg_path = directory.join(format!("{stem}.svg"));
    let png_path = directory.join(format!("{stem}.png"));
    let metadata_path = directory.join(format!("{stem}.json"));
    ncview_rs::export::write_slice_svg(
        &svg_path,
        &raster,
        &state.view.palette,
        limits,
        state.view.scale_mode,
        variable_name,
        metadata_variable.and_then(|variable| variable.units.as_deref()),
        metadata_variable.and_then(|variable| variable.long_name.as_deref()),
        metadata_variable.and_then(|variable| variable.standard_name.as_deref()),
        state.view.time_label.as_deref(),
        state.view.depth_index,
    )?;
    ncview_rs::export::write_slice_png(
        &png_path,
        &raster,
        &state.view.palette,
        limits,
        state.view.scale_mode,
        variable_name,
        metadata_variable.and_then(|variable| variable.units.as_deref()),
        metadata_variable.and_then(|variable| variable.long_name.as_deref()),
        metadata_variable.and_then(|variable| variable.standard_name.as_deref()),
        state.view.time_label.as_deref(),
        state.view.depth_index,
    )?;
    ncview_rs::export::write_slice_metadata_json(
        &metadata_path,
        limits,
        state.view.scale_mode,
        variable_name,
        metadata_variable.and_then(|variable| variable.units.as_deref()),
        metadata_variable.and_then(|variable| variable.long_name.as_deref()),
        metadata_variable.and_then(|variable| variable.standard_name.as_deref()),
        state.view.time_label.as_deref(),
        state.view.depth_index,
    )?;
    Ok(png_path)
}

fn sanitize_filename(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "slice".into()
    } else {
        sanitized
    }
}

fn translate_mouse(
    command: Command,
    area: Rect,
    metadata: &DatasetMetadata,
    view: &ncview_rs::app::ViewModel,
    variable_query: &str,
    graphics: Option<&GraphicsRenderer>,
) -> Command {
    if let Command::BeginDrag { x, y, zoom } = command {
        if view.help_visible || view.overlay.is_some() || view.variable_search_active {
            return Command::Pointer { x, y };
        }
        let Some(canvas) = map_drawable(area, view, graphics) else {
            return Command::Pointer { x, y };
        };
        return if canvas.contains((x, y).into()) {
            Command::BeginDrag { x, y, zoom }
        } else {
            Command::Pointer { x, y }
        };
    }
    if let Command::UpdateDrag { x, y } = command {
        return if view.drag.is_some() {
            Command::UpdateDrag { x, y }
        } else {
            Command::Pointer { x, y }
        };
    }
    if let Command::MouseRelease { x, y } = command {
        let Some(drag) = view.drag else {
            return translate_mouse(
                Command::MouseClick { x, y, right: false },
                area,
                metadata,
                view,
                variable_query,
                graphics,
            );
        };
        let Some(slice) = view.slice.as_ref() else {
            return Command::CancelDrag;
        };
        let Some(canvas) = map_drawable(area, view, graphics) else {
            return Command::CancelDrag;
        };
        if let Some(current) = view.zoom_bounds
            && !drag.zoom
            && let Some((rows, cols)) = drag.pan_delta(
                canvas,
                current.row_end.saturating_sub(current.row_start),
                current.col_end.saturating_sub(current.col_start),
            )
        {
            return Command::Pan { rows, cols };
        }
        let (rows, cols) = slice.values.dim();
        if let Some(bounds) = drag.bounds(canvas, rows, cols) {
            return Command::Zoom(Bounds {
                row_start: slice.source_bounds.row_start + bounds.row_start,
                row_end: slice.source_bounds.row_start + bounds.row_end,
                col_start: slice.source_bounds.col_start + bounds.col_start,
                col_end: slice.source_bounds.col_start + bounds.col_end,
            });
        }
        return map_point_at(x, y, area, view, graphics)
            .map(|(row, col, _)| Command::SelectPoint { row, col })
            .unwrap_or(Command::CancelDrag);
    }
    let (x, y, right, clicked) = match command {
        Command::MouseClick { x, y, right } => (x, y, right, true),
        Command::Pointer { x, y } => (x, y, false, false),
        command => return command,
    };
    if right {
        return Command::ToggleHelp;
    }
    if view.variable_search_active {
        if close_button_hit(variable_browser_rect(area), x, y) {
            return Command::Quit;
        }
        return Command::Pointer { x, y };
    }
    if view.help_visible {
        if close_button_hit(help_rect(area), x, y) {
            return Command::ToggleHelp;
        }
        return Command::Pointer { x, y };
    }
    if let Some(overlay) = view.overlay
        && close_button_hit(overlay_rect(area, overlay), x, y)
    {
        return Command::Quit;
    }
    if matches!(view.overlay, Some(Overlay::Limits | Overlay::Filter)) {
        let width = area.width.saturating_mul(3) / 5;
        let height = area.height.saturating_mul(2) / 5;
        let popup_x = area.x + area.width.saturating_sub(width) / 2;
        let popup_y = area.y + area.height.saturating_sub(height) / 2;
        if x >= popup_x && x < popup_x.saturating_add(width) && y == popup_y.saturating_add(1) {
            return Command::FocusLimitField(LimitField::Min);
        }
        if x >= popup_x && x < popup_x.saturating_add(width) && y == popup_y.saturating_add(2) {
            return Command::FocusLimitField(LimitField::Max);
        }
        return Command::Pointer { x, y };
    }
    if matches!(view.overlay, Some(Overlay::Axis)) {
        let width = area.width.saturating_mul(3) / 5;
        let height = area.height.saturating_mul(2) / 5;
        let popup_x = area.x + area.width.saturating_sub(width) / 2;
        let popup_y = area.y + area.height.saturating_sub(height) / 2;
        if x >= popup_x && x < popup_x.saturating_add(width) && y == popup_y.saturating_add(1) {
            return Command::FocusAxisField(AxisField::X);
        }
        if x >= popup_x && x < popup_x.saturating_add(width) && y == popup_y.saturating_add(2) {
            return Command::FocusAxisField(AxisField::Y);
        }
        return Command::Pointer { x, y };
    }
    if matches!(view.overlay, Some(Overlay::Plot)) {
        let popup = overlay_rect(area, Overlay::Plot);
        let content_x = popup.x.saturating_add(2);
        let type_y = popup.y.saturating_add(2);
        if y == type_y && x >= content_x && x < popup.right().saturating_sub(1) {
            let relative = x.saturating_sub(content_x);
            let fifth = (popup.width.saturating_sub(4) / 5).max(1);
            return if relative < fifth {
                Command::SetPlotKind(ncview_rs::app::PlotKind::TimeSeries)
            } else if relative < fifth.saturating_mul(2) {
                Command::SetPlotKind(ncview_rs::app::PlotKind::Scatter)
            } else if relative < fifth.saturating_mul(3) {
                Command::SetPlotKind(ncview_rs::app::PlotKind::Histogram)
            } else if relative < fifth.saturating_mul(4) {
                Command::SetPlotKind(ncview_rs::app::PlotKind::Cdf)
            } else {
                Command::SetPlotKind(ncview_rs::app::PlotKind::VerticalProfile)
            };
        }
        if x >= popup.x && x < popup.right() {
            if y == popup.y.saturating_add(4) {
                return Command::FocusPlotAxis(ncview_rs::app::PlotAxisField::X);
            }
            if y == popup.y.saturating_add(5) {
                return Command::FocusPlotAxis(ncview_rs::app::PlotAxisField::Y);
            }
        }
        return Command::Pointer { x, y };
    }
    if view.help_visible || view.overlay.is_some() {
        return Command::Pointer { x, y };
    }
    let areas = dashboard_layout::dashboard(area);
    if let Some((row, col, value)) = map_point_at(x, y, area, view, graphics) {
        return if clicked {
            Command::SelectPoint { row, col }
        } else if view
            .hover_point
            .as_ref()
            .is_some_and(|point| point.row == row && point.col == col)
        {
            // Mouse motion within the same source cell does not change the
            // scientific readout. Avoid replacing the hover state (and a
            // needless terminal diff) for every sub-cell cursor movement.
            Command::Pointer { x, y }
        } else {
            Command::HoverPoint {
                x,
                y,
                row,
                col,
                value,
            }
        };
    }
    if !clicked {
        return Command::ClearHover;
    }
    if areas.timeline.contains((x, y).into()) && view.time_length > 0 {
        if x <= areas.timeline.x.saturating_add(7) {
            return Command::TogglePlayback;
        }
        // The timeline speed controls are right-aligned in the panel title:
        // "[−] slower  [＋] faster". Give each control a generous hitbox.
        let speed_start = areas.timeline.right().saturating_sub(23);
        if x >= speed_start {
            return if x < speed_start.saturating_add(11) {
                Command::DecreasePlaybackSpeed
            } else {
                Command::IncreasePlaybackSpeed
            };
        }
        let start = areas.timeline.x.saturating_add(1);
        let end = areas
            .timeline
            .x
            .saturating_add(areas.timeline.width.saturating_sub(2));
        let position = x.clamp(start, end).saturating_sub(start) as usize;
        let width = usize::from(end.saturating_sub(start).max(1));
        let index = position.saturating_mul(view.time_length.saturating_sub(1)) / width;
        return Command::SetTime(index);
    }
    if !areas.sidebar.contains((x, y).into()) {
        return Command::Pointer { x, y };
    }
    let plottable_all = metadata
        .variables
        .iter()
        .filter(|variable| variable.numeric && variable.dimensions.len() >= 2)
        .cloned()
        .collect::<Vec<_>>();
    let plottable = ncview_rs::ui::sidebar::filter_variables(&plottable_all, variable_query)
        .into_iter()
        .take(8)
        .collect::<Vec<_>>();
    // Sidebar rows: filename, colormap name/scale, a View actions heading,
    // two view-action rows, a Navigation heading, two navigation rows,
    // variables, dimensions, then the limit/filter controls. Keep mouse hit
    // targets aligned with the visible button rows.
    let action_row_one = areas.sidebar.y.saturating_add(6);
    let action_row_two = areas.sidebar.y.saturating_add(7);
    let action_third = (areas.sidebar.width / 3).max(1);
    if y == action_row_one {
        return match (x.saturating_sub(areas.sidebar.x)) / action_third {
            0 => Command::CyclePalette,
            1 => Command::TogglePaletteReverse,
            _ => Command::AutomaticLimits,
        };
    }
    if y == action_row_two {
        return match (x.saturating_sub(areas.sidebar.x)) / action_third {
            0 => Command::OpenLimits,
            1 => Command::OpenFilter,
            _ => Command::OpenAxisOverlay,
        };
    }
    let date_row = areas.sidebar.y.saturating_add(9);
    if y == date_row {
        return match (x.saturating_sub(areas.sidebar.x)) / action_third {
            0 => Command::ResetZoom,
            1 => Command::MoveTime(-1),
            _ => Command::MoveTime(1),
        };
    }
    let speed_row = areas.sidebar.y.saturating_add(10);
    if y == speed_row {
        return match (x.saturating_sub(areas.sidebar.x)) / action_third {
            0 => Command::DecreasePlaybackSpeed,
            1 => Command::IncreasePlaybackSpeed,
            _ => Command::ToggleColorScaleScope,
        };
    }
    let search_row = areas.sidebar.y.saturating_add(12);
    if y == search_row {
        return Command::OpenVariableSearch;
    }
    let variable_start = search_row.saturating_add(1);
    if y >= variable_start && usize::from(y - variable_start) < plottable.len() {
        return Command::SelectVariableAt(usize::from(y - variable_start));
    }
    let dimensions = metadata.dimensions.iter().take(8).count();
    let colormap_heading = areas.sidebar.y.saturating_add(2);
    let palette_row = colormap_heading.saturating_add(1);
    let dimensions_heading =
        variable_start + u16::try_from(plottable.len()).unwrap_or(u16::MAX) + 1;
    let dimensions_end = dimensions_heading
        .saturating_add(1)
        .saturating_add(u16::try_from(dimensions).unwrap_or(u16::MAX));
    let limits_row = dimensions_end.saturating_add(1);
    match y {
        value if value == palette_row => Command::CyclePalette,
        value if value == limits_row => Command::OpenLimits,
        _ => Command::Pointer { x, y },
    }
}

fn close_button_hit(popup: Rect, x: u16, y: u16) -> bool {
    popup.width >= 4
        && y == popup.y
        && x >= popup.x.saturating_add(popup.width.saturating_sub(10))
        && x < popup.x.saturating_add(popup.width)
}

fn variable_browser_rect(area: Rect) -> Rect {
    let width = area.width.saturating_mul(4).saturating_div(5).max(1);
    let height = area.height.saturating_mul(4).saturating_div(5).max(1);
    Rect {
        x: area.x + area.width.saturating_sub(width.min(area.width)) / 2,
        y: area.y + area.height.saturating_sub(height.min(area.height)) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

fn help_rect(area: Rect) -> Rect {
    let width = area.width.saturating_mul(3) / 4;
    let height = area.height.saturating_mul(3) / 5;
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn overlay_rect(area: Rect, overlay: Overlay) -> Rect {
    let large = matches!(overlay, Overlay::CommandPalette | Overlay::Plot);
    let width = if large {
        area.width.saturating_mul(3) / 4
    } else {
        area.width.saturating_mul(3) / 5
    };
    let height = if large {
        area.height.saturating_mul(3) / 5
    } else {
        area.height.saturating_mul(2) / 5
    };
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn map_point_at(
    x: u16,
    y: u16,
    area: Rect,
    view: &ncview_rs::app::ViewModel,
    graphics: Option<&GraphicsRenderer>,
) -> Option<(usize, usize, Option<f64>)> {
    let slice = view.slice.as_ref()?;
    let drawable = map_drawable(area, view, graphics)?;
    let (rows, cols) = slice.values.dim();
    let drawable = if graphics.is_some_and(GraphicsRenderer::supports_graphics) {
        drawable
    } else {
        Rect::new(
            drawable.x,
            drawable.y,
            drawable.width.min(u16::try_from(cols).unwrap_or(u16::MAX)),
            drawable.height.min(u16::try_from(rows).unwrap_or(u16::MAX)),
        )
    };
    let (row, col) = screen_to_source(x, y, drawable, slice.source_bounds)?;
    let value = slice.value_at_source(row, col);
    Some((row, col, value))
}

fn map_drawable(
    area: Rect,
    view: &ncview_rs::app::ViewModel,
    graphics: Option<&GraphicsRenderer>,
) -> Option<Rect> {
    let _ = view.slice.as_ref()?;
    let panel = dashboard_layout::dashboard(area).canvas;
    let inner = Rect::new(
        panel.x.saturating_add(1),
        panel.y.saturating_add(1),
        panel.width.saturating_sub(2),
        panel.height.saturating_sub(2),
    );
    if graphics.is_some_and(GraphicsRenderer::supports_graphics) {
        return Some(graphics.map_or(inner, |renderer| renderer.drawable_area(inner)));
    }
    {
        let slice = view.slice.as_ref()?;
        let (rows, cols) = slice.values.dim();
        Some(Rect::new(
            inner.x,
            inner.y,
            inner.width.min(u16::try_from(cols).unwrap_or(u16::MAX)),
            inner.height.min(u16::try_from(rows).unwrap_or(u16::MAX)),
        ))
    }
}

fn select_initial_variable(state: &mut AppState, metadata: &DatasetMetadata) {
    let selected = state
        .variables
        .iter()
        .filter(|variable| variable.numeric && variable.dimensions.len() >= 2)
        .max_by_key(|variable| {
            let area_penalty = variable.name.to_ascii_lowercase().contains("area");
            (variable.dimensions.len(), !area_penalty)
        })
        .map(|variable| variable.name.clone());
    state.view.selected_variable = selected;
    if let Some(variable) = state.view.selected_variable.clone()
        && let Some(metadata_variable) =
            metadata.variables.iter().find(|item| item.name == variable)
    {
        state.view.axis_options = metadata_variable.dimensions.clone();
        let (time_length, depth_length) = leading_lengths(metadata, metadata_variable);
        state.view.time_length = time_length;
        state.view.depth_length = depth_length;
        state.view.status = format!(
            "opened {} variable(s); selected {variable}",
            state.variables.len()
        );
    }
}

fn configure_timeline(
    state: &mut AppState,
    sources: &[Box<dyn data::DataSource>],
    active_file: usize,
) {
    state.view.timeline.clear();
    let Some(variable_name) = state.view.selected_variable.as_deref() else {
        state.view.time_length = 1;
        state.view.time_index = 0;
        state.view.time_label = None;
        return;
    };
    for (source_index, source) in sources.iter().enumerate() {
        let Some(variable) = source
            .metadata()
            .variables
            .iter()
            .find(|variable| variable.name == variable_name)
        else {
            continue;
        };
        let (time_length, _) = leading_lengths(source.metadata(), variable);
        for local_index in 0..time_length.max(1) {
            let global_index = state.view.timeline.len();
            let label = source
                .time_label_for_variable(variable_name, local_index)
                .unwrap_or_else(|| format!("t={global_index}"));
            state.view.timeline.push(TimelinePoint {
                source_index,
                local_index,
                label,
            });
        }
    }
    if state.view.timeline.is_empty()
        && let Some(source) = sources.get(active_file)
        && source
            .metadata()
            .variables
            .iter()
            .any(|variable| variable.name == variable_name)
    {
        state.view.timeline.push(TimelinePoint {
            source_index: active_file,
            local_index: 0,
            label: "coordinate index".into(),
        });
    }
    state.view.time_length = state.view.timeline.len().max(1);
    state.view.time_index = state
        .view
        .time_index
        .min(state.view.time_length.saturating_sub(1));
    state.view.time_label = state
        .view
        .timeline
        .get(state.view.time_index)
        .map(|point| point.label.clone());
}

fn load_selected(state: &mut AppState, sources: &[Box<dyn data::DataSource>], active_file: usize) {
    let Some(variable_name) = state.view.selected_variable.clone() else {
        return;
    };
    let timeline_point = state
        .view
        .timeline
        .get(state.view.time_index)
        .cloned()
        .unwrap_or(TimelinePoint {
            source_index: active_file,
            local_index: 0,
            label: "coordinate index".into(),
        });
    let Some(source) = sources.get(timeline_point.source_index) else {
        return;
    };
    let metadata = source.metadata();
    let Some(variable) = metadata
        .variables
        .iter()
        .find(|item| item.name == variable_name)
    else {
        return;
    };
    let Some((full_bounds, _time_length, depth_length)) = plane_bounds(
        metadata,
        variable,
        state.view.x_axis.as_deref(),
        state.view.y_axis.as_deref(),
    ) else {
        state.view.status = format!("{variable_name}: needs at least two dimensions");
        return;
    };
    state.view.time_length = state.view.timeline.len().max(1);
    state.view.depth_length = depth_length;
    state.view.depth_index = state.view.depth_index.min(depth_length.saturating_sub(1));
    state.view.time_label = Some(timeline_point.label.clone());
    state.view.level_label = source.vertical_label(&variable_name, state.view.depth_index);
    state.view.full_bounds = Some(full_bounds);
    let bounds = state.view.zoom_bounds.unwrap_or(full_bounds);
    let fixed_axes = fixed_axes_for_plane(
        metadata,
        variable,
        state.view.x_axis.as_deref(),
        state.view.y_axis.as_deref(),
        timeline_point.local_index,
        state.view.depth_index,
    );
    let request = SliceRequest {
        variable: variable_name.clone(),
        time: timeline_point.local_index,
        depth: state.view.depth_index,
        bounds,
    };
    match source.read_slice_on_axes(
        &request,
        state.view.y_axis.as_deref(),
        state.view.x_axis.as_deref(),
        &fixed_axes,
    ) {
        Ok(slice) => {
            let current_limits = slice_limits(&slice, state.view.scale_mode);
            let full_limits = if bounds == full_bounds {
                current_limits
            } else if state.view.color_scale_scope == ColorScaleScope::GlobalView {
                let full_request = SliceRequest {
                    variable: variable_name.clone(),
                    time: timeline_point.local_index,
                    depth: state.view.depth_index,
                    bounds: full_bounds,
                };
                source
                    .read_slice_on_axes(
                        &full_request,
                        state.view.y_axis.as_deref(),
                        state.view.x_axis.as_deref(),
                        &fixed_axes,
                    )
                    .ok()
                    .and_then(|full_slice| slice_limits(&full_slice, state.view.scale_mode))
            } else {
                None
            };
            if bounds == full_bounds {
                state.view.global_limits = current_limits;
            } else if state.view.color_scale_scope == ColorScaleScope::GlobalView
                && let Some(full_limits) = full_limits
            {
                state.view.global_limits = Some(full_limits);
            }
            if !state.view.limits_manual {
                state.view.limits = match state.view.color_scale_scope {
                    ColorScaleScope::CurrentView => current_limits,
                    ColorScaleScope::GlobalView => {
                        state.view.global_limits.or(full_limits).or(current_limits)
                    }
                };
            }
            state.view.slice = Some(slice);
            if let Some(point) = state.view.hover_point.as_mut()
                && let Some(value) = state
                    .view
                    .slice
                    .as_ref()
                    .and_then(|slice| slice.value_at_source(point.row, point.col))
            {
                point.value = Some(value);
            }
            state.view.loading = ncview_rs::app::LoadingState::Ready;
            let time_text = state
                .view
                .time_label
                .as_deref()
                .unwrap_or("coordinate index");
            let level_text = state
                .view
                .level_label
                .as_deref()
                .map(|label| format!("  level={label}"))
                .unwrap_or_default();
            state.view.status = format!("{variable_name}  time={time_text}{level_text}  ready");
        }
        Err(error) => {
            state.view.slice = None;
            state.view.loading = ncview_rs::app::LoadingState::Error;
            state.view.status = format!("{variable_name}: {error}");
        }
    }
}

fn slice_limits(
    slice: &ncview_rs::data::slice::Slice2D,
    scale: ncview_rs::app::ScaleMode,
) -> Option<(f64, f64)> {
    if scale == ncview_rs::app::ScaleMode::Log {
        ncview_rs::app::positive_slice_limits(slice)
    } else {
        slice.statistics.map(|stats| (stats.min, stats.max))
    }
}

fn load_time_series(
    state: &mut AppState,
    sources: &[Box<dyn data::DataSource>],
    active_file: usize,
) {
    state.view.time_series.clear();
    state.view.time_series_labels.clear();
    state.view.plot_series.clear();
    let selected_points = if state.view.selected_points.is_empty() {
        state.view.selected_point.into_iter().collect::<Vec<_>>()
    } else {
        state.view.selected_points.clone()
    };
    if selected_points.is_empty() {
        load_domain_summary(state, sources);
        return;
    }
    if matches!(
        state.view.plot_draft.x_axis,
        ncview_rs::app::PlotXAxis::Longitude
            | ncview_rs::app::PlotXAxis::Latitude
            | ncview_rs::app::PlotXAxis::Dimension(_)
    ) {
        load_cross_section(state, sources, &selected_points);
        return;
    }
    let Some(variable_name) = state.view.selected_variable.clone() else {
        return;
    };
    let Some(active_source) = sources.get(active_file) else {
        return;
    };
    let timeline = state.view.timeline.clone();
    let depth_index = state.view.depth_index;
    let mut used_labels = Vec::new();
    let mut total_finite = 0;

    for (point_number, (row, col)) in selected_points.iter().copied().enumerate() {
        let mut data = Vec::with_capacity(timeline.len());
        let mut labels = Vec::with_capacity(timeline.len());
        let mut finite_samples = 0;
        for (time_index, point) in timeline.iter().enumerate() {
            let Some(source) = sources.get(point.source_index) else {
                data.push((time_index as f64, f64::NAN));
                labels.push(point.label.clone());
                continue;
            };
            let Some(variable) = source
                .metadata()
                .variables
                .iter()
                .find(|variable| variable.name == variable_name)
            else {
                data.push((time_index as f64, f64::NAN));
                labels.push(point.label.clone());
                continue;
            };
            let Some((source_bounds, _, depth_length)) =
                spatial_bounds(source.metadata(), variable)
            else {
                data.push((time_index as f64, f64::NAN));
                labels.push(point.label.clone());
                continue;
            };
            if row < source_bounds.row_start
                || row >= source_bounds.row_end
                || col < source_bounds.col_start
                || col >= source_bounds.col_end
            {
                data.push((time_index as f64, f64::NAN));
                labels.push(point.label.clone());
                continue;
            }
            let request = SliceRequest {
                variable: variable_name.clone(),
                time: point.local_index,
                depth: depth_index.min(depth_length.saturating_sub(1)),
                bounds: Bounds::new(row, row + 1, col, col + 1)
                    .expect("point bounds are non-empty"),
            };
            let value = source
                .read_slice_on_axes(&request, None, None, &[])
                .ok()
                .and_then(|slice| slice.value_at_source(row, col))
                .unwrap_or(f64::NAN);
            finite_samples += usize::from(value.is_finite());
            data.push((time_index as f64, value));
            labels.push(point.label.clone());
        }
        total_finite += finite_samples;
        let base_label = point_label(active_source.as_ref(), &variable_name, row, col);
        let label = if used_labels.contains(&base_label) {
            format!("{base_label} #{}", point_number + 1)
        } else {
            base_label
        };
        used_labels.push(label.clone());
        state.view.plot_series.push(PlotSeries {
            point: (row, col),
            label,
            data,
            labels,
        });
    }

    if let Some(first) = state.view.plot_series.first() {
        state.view.time_series = first.data.clone();
        state.view.time_series_labels = first.labels.clone();
    }
    let sample_count = state
        .view
        .plot_series
        .first()
        .map_or(0, |series| series.data.len());
    state.view.status = format!(
        "{} point(s): {total_finite}/{} finite timeline samples",
        selected_points.len(),
        sample_count.saturating_mul(selected_points.len())
    );
}

fn load_cross_section(
    state: &mut AppState,
    sources: &[Box<dyn data::DataSource>],
    selected_points: &[(usize, usize)],
) {
    let Some(variable_name) = state.view.selected_variable.clone() else {
        return;
    };
    let x_axis = state.view.plot_draft.x_axis;
    let Some(time_point) = state.view.timeline.get(state.view.time_index) else {
        return;
    };
    let Some(source) = sources.get(time_point.source_index) else {
        return;
    };
    let Some(variable) = source
        .metadata()
        .variables
        .iter()
        .find(|variable| variable.name == variable_name)
    else {
        return;
    };
    let Some((source_bounds, _, depth_length)) = spatial_bounds(source.metadata(), variable) else {
        return;
    };
    let depth = state.view.depth_index.min(depth_length.saturating_sub(1));
    let Some(x_dimension) = plot_dimension_name(source.metadata(), variable, x_axis) else {
        state.view.status = "the selected plot dimension is unavailable for this field".into();
        return;
    };
    let Some(x_length) = dimension_length(source.metadata(), &x_dimension) else {
        return;
    };
    let dimension_coordinates = source.dimension_values(&variable_name, &x_dimension);
    let fixed_dimension = variable
        .dimensions
        .iter()
        .find(|name| {
            !name.eq_ignore_ascii_case(&x_dimension)
                && (state.view.x_axis.as_deref() == Some(name.as_str())
                    || state.view.y_axis.as_deref() == Some(name.as_str()))
        })
        .or_else(|| {
            variable
                .dimensions
                .iter()
                .find(|name| !name.eq_ignore_ascii_case(&x_dimension))
        })
        .cloned();
    let Some(fixed_dimension) = fixed_dimension else {
        state.view.status = "a cross-section needs at least two dimensions".into();
        return;
    };
    let mut series = Vec::new();
    let mut used_labels = Vec::new();
    let level_label = state
        .view
        .level_label
        .clone()
        .unwrap_or_else(|| format!("depth={depth}"));

    for (point_number, &(row, col)) in selected_points.iter().enumerate() {
        let x_is_grib = source.metadata().format == DatasetFormat::Grib2;
        let latitude_dimension = plot_dimension_name(
            source.metadata(),
            variable,
            ncview_rs::app::PlotXAxis::Latitude,
        )
        .unwrap_or_else(|| "latitude".into());
        let longitude_dimension = plot_dimension_name(
            source.metadata(),
            variable,
            ncview_rs::app::PlotXAxis::Longitude,
        )
        .unwrap_or_else(|| "longitude".into());
        let x_on_row = !x_is_grib || x_dimension.eq_ignore_ascii_case(&latitude_dimension);
        let fixed_index = dimension_index_for_plot(
            source.metadata(),
            &fixed_dimension,
            row,
            col,
            time_point.local_index,
            depth,
        );
        let bounds = if x_is_grib {
            if x_on_row {
                Bounds::new(source_bounds.row_start, source_bounds.row_end, col, col + 1)
            } else {
                Bounds::new(row, row + 1, source_bounds.col_start, source_bounds.col_end)
            }
        } else if x_on_row {
            Bounds::new(0, x_length, fixed_index, fixed_index + 1)
        } else {
            Bounds::new(fixed_index, fixed_index + 1, 0, x_length)
        };
        let Ok(bounds) = bounds else {
            continue;
        };
        if row < source_bounds.row_start
            || row >= source_bounds.row_end
            || col < source_bounds.col_start
            || col >= source_bounds.col_end
        {
            continue;
        }
        let request = SliceRequest {
            variable: variable_name.clone(),
            time: time_point.local_index,
            depth,
            bounds,
        };
        let fixed_axes = variable
            .dimensions
            .iter()
            .filter(|name| {
                !name.eq_ignore_ascii_case(&x_dimension)
                    && !name.eq_ignore_ascii_case(&fixed_dimension)
            })
            .map(|name| {
                (
                    name.clone(),
                    dimension_index_for_plot(
                        source.metadata(),
                        name,
                        row,
                        col,
                        time_point.local_index,
                        depth,
                    ),
                )
            })
            .collect::<Vec<_>>();
        let row_axis = if x_is_grib {
            Some(latitude_dimension.as_str())
        } else if x_on_row {
            Some(x_dimension.as_str())
        } else {
            Some(fixed_dimension.as_str())
        };
        let col_axis = if x_is_grib {
            Some(longitude_dimension.as_str())
        } else if x_on_row {
            Some(fixed_dimension.as_str())
        } else {
            Some(x_dimension.as_str())
        };
        let Ok(slice) = source.read_slice_on_axes(&request, row_axis, col_axis, &fixed_axes) else {
            continue;
        };
        let count = if x_is_grib {
            if x_on_row {
                source_bounds.row_end - source_bounds.row_start
            } else {
                source_bounds.col_end - source_bounds.col_start
            }
        } else {
            x_length
        };
        let mut data = Vec::with_capacity(count);
        for offset in 0..count {
            let (sample_row, sample_col) = if x_is_grib {
                if x_on_row {
                    (source_bounds.row_start + offset, col)
                } else {
                    (row, source_bounds.col_start + offset)
                }
            } else if x_on_row {
                (offset, fixed_index)
            } else {
                (fixed_index, offset)
            };
            let coordinate = source.point_coordinates(
                &variable_name,
                if x_dimension.eq_ignore_ascii_case(&latitude_dimension) {
                    if x_on_row { offset } else { row }
                } else {
                    row
                },
                if x_dimension.eq_ignore_ascii_case(&longitude_dimension) {
                    if x_on_row { offset } else { col }
                } else {
                    col
                },
            );
            let x = if x_dimension.eq_ignore_ascii_case(&longitude_dimension) {
                coordinate
                    .longitude
                    .filter(|value| value.is_finite())
                    .unwrap_or(offset as f64)
            } else if x_dimension.eq_ignore_ascii_case(&latitude_dimension) {
                coordinate
                    .latitude
                    .filter(|value| value.is_finite())
                    .unwrap_or(offset as f64)
            } else {
                dimension_coordinates
                    .as_ref()
                    .and_then(|values| values.get(offset).copied())
                    .filter(|value| value.is_finite())
                    .unwrap_or(offset as f64)
            };
            let y = slice
                .value_at_source(sample_row, sample_col)
                .unwrap_or(f64::NAN);
            data.push((x, y));
        }
        // Latitude/longitude coordinates can be stored north-to-south or
        // east-to-west. Sort the horizontal section so its x axis is always
        // monotonic before it reaches the chart renderer.
        data.sort_by(|left, right| left.0.total_cmp(&right.0));
        let base = format!(
            "{} @ {level_label}",
            point_label(source.as_ref(), &variable_name, row, col)
        );
        let label = if used_labels.contains(&base) {
            format!("{base} #{}", point_number + 1)
        } else {
            base
        };
        used_labels.push(label.clone());
        series.push(PlotSeries {
            point: (row, col),
            label,
            data,
            labels: Vec::new(),
        });
    }
    if let Some(first) = series.first() {
        state.view.time_series = first.data.clone();
        state.view.time_series_labels.clear();
    }
    state.view.plot_series = series;
    state.view.status = format!(
        "{x_dimension} cross-section at time {} and {level_label}",
        time_point.label
    );
}

fn plot_dimension_name(
    metadata: &DatasetMetadata,
    variable: &Variable,
    x_axis: ncview_rs::app::PlotXAxis,
) -> Option<String> {
    match x_axis {
        ncview_rs::app::PlotXAxis::Longitude => variable
            .dimensions
            .iter()
            .find(|name| {
                metadata
                    .dimensions
                    .iter()
                    .find(|dimension| dimension.name.eq_ignore_ascii_case(name))
                    .is_some_and(|dimension| dimension.role == AxisRole::Longitude)
                    || name.to_ascii_lowercase().contains("lon")
            })
            .cloned(),
        ncview_rs::app::PlotXAxis::Latitude => variable
            .dimensions
            .iter()
            .find(|name| {
                metadata
                    .dimensions
                    .iter()
                    .find(|dimension| dimension.name.eq_ignore_ascii_case(name))
                    .is_some_and(|dimension| dimension.role == AxisRole::Latitude)
                    || name.to_ascii_lowercase().contains("lat")
            })
            .cloned(),
        ncview_rs::app::PlotXAxis::Dimension(index) => variable.dimensions.get(index).cloned(),
        _ => None,
    }
}

fn dimension_index_for_plot(
    metadata: &DatasetMetadata,
    dimension_name: &str,
    row: usize,
    col: usize,
    time: usize,
    depth: usize,
) -> usize {
    let dimension = metadata
        .dimensions
        .iter()
        .find(|dimension| dimension.name.eq_ignore_ascii_case(dimension_name));
    let index = match dimension.map(|dimension| dimension.role) {
        Some(AxisRole::Latitude) => row,
        Some(AxisRole::Longitude) => col,
        Some(AxisRole::Time) => time,
        Some(AxisRole::Depth) => depth,
        _ => 0,
    };
    dimension
        .map(|dimension| index.min(dimension.length.saturating_sub(1)))
        .unwrap_or(index)
}

fn point_label(
    source: &dyn data::DataSource,
    variable_name: &str,
    row: usize,
    col: usize,
) -> String {
    let coordinates = source.point_coordinates(variable_name, row, col);
    match (coordinates.latitude, coordinates.longitude) {
        (Some(latitude), Some(longitude)) => format!("lat={latitude:.2}, lon={longitude:.2}"),
        _ => format!("row={row}, col={col}"),
    }
}

fn load_domain_summary(state: &mut AppState, sources: &[Box<dyn data::DataSource>]) {
    let Some(variable_name) = state.view.selected_variable.clone() else {
        return;
    };
    let timeline = state.view.timeline.clone();
    let depth_index = state.view.depth_index;
    let view_bounds = state.view.zoom_bounds;
    let mut mean = Vec::with_capacity(timeline.len());
    let mut minimum = Vec::with_capacity(timeline.len());
    let mut maximum = Vec::with_capacity(timeline.len());
    let mut labels = Vec::with_capacity(timeline.len());
    let mut finite_samples = 0;

    for point in &timeline {
        let Some(source) = sources.get(point.source_index) else {
            continue;
        };
        let Some(variable) = source
            .metadata()
            .variables
            .iter()
            .find(|variable| variable.name == variable_name)
        else {
            continue;
        };
        let Some((source_bounds, _, depth_length)) = spatial_bounds(source.metadata(), variable)
        else {
            continue;
        };
        let Some(bounds) = clamp_domain_bounds(source_bounds, view_bounds) else {
            continue;
        };
        let request = SliceRequest {
            variable: variable_name.clone(),
            time: point.local_index,
            depth: depth_index.min(depth_length.saturating_sub(1)),
            bounds,
        };
        let statistics = source
            .read_slice_on_axes(&request, None, None, &[])
            .ok()
            .and_then(|slice| slice.statistics);
        if let Some(statistics) = statistics {
            finite_samples += statistics.finite_count;
            mean.push((mean.len() as f64, statistics.mean));
            minimum.push((minimum.len() as f64, statistics.min));
            maximum.push((maximum.len() as f64, statistics.max));
        } else {
            let index = mean.len() as f64;
            mean.push((index, f64::NAN));
            minimum.push((index, f64::NAN));
            maximum.push((index, f64::NAN));
        }
        labels.push(point.label.clone());
    }

    state.view.plot_series = vec![
        PlotSeries {
            point: (0, 0),
            label: "mean".into(),
            data: mean.clone(),
            labels: labels.clone(),
        },
        PlotSeries {
            point: (0, 0),
            label: "minimum".into(),
            data: minimum,
            labels: labels.clone(),
        },
        PlotSeries {
            point: (0, 0),
            label: "maximum".into(),
            data: maximum,
            labels: labels.clone(),
        },
    ];
    state.view.time_series = mean;
    state.view.time_series_labels = labels;
    let domain_label = if view_bounds.is_some() {
        "view window"
    } else {
        "full field"
    };
    state.view.status = format!(
        "domain summary ({domain_label}): {finite_samples} finite values across {} timeline samples",
        state.view.time_series.len()
    );
}

fn clamp_domain_bounds(source_bounds: Bounds, requested: Option<Bounds>) -> Option<Bounds> {
    let requested = requested.unwrap_or(source_bounds);
    let row_start = requested
        .row_start
        .max(source_bounds.row_start)
        .min(source_bounds.row_end.saturating_sub(1));
    let row_end = requested.row_end.min(source_bounds.row_end);
    let col_start = requested
        .col_start
        .max(source_bounds.col_start)
        .min(source_bounds.col_end.saturating_sub(1));
    let col_end = requested.col_end.min(source_bounds.col_end);
    Bounds::new(row_start, row_end, col_start, col_end).ok()
}

fn leading_lengths(metadata: &DatasetMetadata, variable: &Variable) -> (usize, usize) {
    let row_index = variable
        .dimensions
        .iter()
        .position(|name| {
            metadata
                .dimensions
                .iter()
                .find(|dimension| dimension.name == *name)
                .is_some_and(|dimension| dimension.role == AxisRole::Latitude)
        })
        .unwrap_or(variable.dimensions.len().saturating_sub(2));
    let col_index = variable
        .dimensions
        .iter()
        .position(|name| {
            metadata
                .dimensions
                .iter()
                .find(|dimension| dimension.name == *name)
                .is_some_and(|dimension| dimension.role == AxisRole::Longitude)
        })
        .unwrap_or(variable.dimensions.len().saturating_sub(1));
    variable
        .dimensions
        .iter()
        .enumerate()
        .filter(|(axis, _)| *axis != row_index && *axis != col_index)
        .fold((1, 1), |(time, depth), (axis, name)| {
            let length = metadata
                .dimensions
                .iter()
                .find(|dimension| dimension.name == *name)
                .map_or(1, |dimension| dimension.length);
            let role = match metadata
                .dimensions
                .iter()
                .find(|dimension| dimension.name == *name)
                .map(|dimension| dimension.role)
            {
                Some(AxisRole::Time) => AxisRole::Time,
                Some(AxisRole::Depth) => AxisRole::Depth,
                _ if axis == row_index => AxisRole::Other,
                _ => match axis {
                    0 => AxisRole::Time,
                    1 => AxisRole::Depth,
                    _ => AxisRole::Other,
                },
            };
            match role {
                AxisRole::Time => (length, depth),
                AxisRole::Depth => (time, length),
                _ => (time, depth),
            }
        })
}

fn spatial_bounds(
    metadata: &DatasetMetadata,
    variable: &Variable,
) -> Option<(Bounds, usize, usize)> {
    if variable.dimensions.len() < 2 {
        return None;
    }
    let row_name = variable
        .dimensions
        .iter()
        .find(|name| {
            metadata
                .dimensions
                .iter()
                .find(|dimension| dimension.name == **name)
                .is_some_and(|dimension| dimension.role == AxisRole::Latitude)
        })
        .unwrap_or(&variable.dimensions[variable.dimensions.len() - 2]);
    let col_name = variable
        .dimensions
        .iter()
        .find(|name| {
            metadata
                .dimensions
                .iter()
                .find(|dimension| dimension.name == **name)
                .is_some_and(|dimension| dimension.role == AxisRole::Longitude)
        })
        .unwrap_or(&variable.dimensions[variable.dimensions.len() - 1]);
    if row_name == col_name {
        return None;
    }
    let rows = metadata
        .dimensions
        .iter()
        .find(|dimension| dimension.name == *row_name)?
        .length;
    let cols = metadata
        .dimensions
        .iter()
        .find(|dimension| dimension.name == *col_name)?
        .length;
    let (time, depth) = leading_lengths(metadata, variable);
    Some((Bounds::new(0, rows, 0, cols).ok()?, time, depth))
}

fn plane_bounds(
    metadata: &DatasetMetadata,
    variable: &Variable,
    x_axis: Option<&str>,
    y_axis: Option<&str>,
) -> Option<(Bounds, usize, usize)> {
    let (Some(x_axis), Some(y_axis)) = (x_axis, y_axis) else {
        return spatial_bounds(metadata, variable);
    };
    let col_name = variable
        .dimensions
        .iter()
        .find(|name| name.eq_ignore_ascii_case(x_axis))?;
    let row_name = variable
        .dimensions
        .iter()
        .find(|name| name.eq_ignore_ascii_case(y_axis))?;
    if col_name == row_name {
        return None;
    }
    let rows = dimension_length(metadata, row_name)?;
    let cols = dimension_length(metadata, col_name)?;
    let (time, depth) = axis_lengths(metadata, variable);
    Some((Bounds::new(0, rows, 0, cols).ok()?, time, depth))
}

fn dimension_length(metadata: &DatasetMetadata, name: &str) -> Option<usize> {
    metadata
        .dimensions
        .iter()
        .find(|dimension| dimension.name.eq_ignore_ascii_case(name))
        .map(|dimension| dimension.length)
}

fn axis_lengths(metadata: &DatasetMetadata, variable: &Variable) -> (usize, usize) {
    let time = variable
        .dimensions
        .iter()
        .find_map(|name| {
            metadata
                .dimensions
                .iter()
                .find(|dimension| dimension.name.eq_ignore_ascii_case(name))
                .filter(|dimension| dimension.role == AxisRole::Time)
                .map(|dimension| dimension.length)
        })
        .unwrap_or(1);
    let depth = variable
        .dimensions
        .iter()
        .find_map(|name| {
            metadata
                .dimensions
                .iter()
                .find(|dimension| dimension.name.eq_ignore_ascii_case(name))
                .filter(|dimension| dimension.role == AxisRole::Depth)
                .map(|dimension| dimension.length)
        })
        .unwrap_or(1);
    (time, depth)
}

fn fixed_axes_for_plane(
    metadata: &DatasetMetadata,
    variable: &Variable,
    x_axis: Option<&str>,
    y_axis: Option<&str>,
    time_index: usize,
    depth_index: usize,
) -> Vec<(String, usize)> {
    let Some((x_axis, y_axis)) = x_axis.zip(y_axis) else {
        return Vec::new();
    };
    variable
        .dimensions
        .iter()
        .filter_map(|name| {
            if name.eq_ignore_ascii_case(x_axis) || name.eq_ignore_ascii_case(y_axis) {
                return None;
            }
            let dimension = metadata
                .dimensions
                .iter()
                .find(|dimension| dimension.name.eq_ignore_ascii_case(name));
            let length = dimension.map_or(1, |dimension| dimension.length);
            let index = match dimension.map(|dimension| dimension.role) {
                Some(AxisRole::Time) => time_index,
                Some(AxisRole::Depth) => depth_index,
                _ => 0,
            };
            Some((name.clone(), index.min(length.saturating_sub(1))))
        })
        .collect()
}
