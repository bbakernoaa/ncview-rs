use std::collections::VecDeque;

use crate::data::slice::{Bounds, Slice2D};
use crate::data::{PointCoordinates, Variable};
pub use crate::render::colors::ScaleMode;
use crate::render::colors::{Palette, discover_colormaps};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Generation(pub u64);

pub const DEFAULT_DECODED_WORKING_SET_LIMIT: usize = 512 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadingState {
    Idle,
    Loading,
    Ready,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Quit,
    SelectVariable(usize),
    SelectVariableAt(usize),
    MoveTime(isize),
    SetTime(usize),
    MoveDepth(isize),
    SetDepth(usize),
    ToggleSidebarFocus,
    MoveDepthCursor(isize),
    ApplyDepthCursor,
    PointerScroll {
        x: u16,
        y: u16,
        delta: isize,
    },
    SetBounds(Bounds),
    ToggleHelp,
    UpdateVariableQuery(String),
    Resize {
        width: u16,
        height: u16,
    },
    CyclePalette,
    TogglePaletteReverse,
    CycleImageFilter,
    ExportCurrent,
    AutomaticLimits,
    ManualLimits {
        min: f64,
        max: f64,
    },
    OpenLimits,
    OpenFilter,
    ClearFilter,
    OpenCommandPalette,
    OpenVariableSearch,
    SubmitVariableSearch,
    ExecuteCommandPalette,
    ExecutePaletteChoice(usize),
    InputChar(char),
    DeleteInput,
    ApplyLimitDraft,
    NextLimitField,
    FocusLimitField(LimitField),
    FocusAxisField(AxisField),
    Zoom(Bounds),
    ResetZoom,
    Pan {
        rows: isize,
        cols: isize,
    },
    BeginDrag {
        x: u16,
        y: u16,
        zoom: bool,
    },
    UpdateDrag {
        x: u16,
        y: u16,
    },
    CancelDrag,
    OpenAxisOverlay,
    CycleAxis(isize),
    ActivatePoint,
    OpenPlot,
    SetPlotKind(PlotKind),
    CyclePlotAxis(isize),
    FocusPlotAxis(PlotAxisField),
    TogglePointSelection,
    Pointer {
        x: u16,
        y: u16,
    },
    HoverPoint {
        x: u16,
        y: u16,
        row: usize,
        col: usize,
        value: Option<f64>,
    },
    ClearHover,
    SelectPoint {
        row: usize,
        col: usize,
    },
    MouseClick {
        x: u16,
        y: u16,
        right: bool,
    },
    MouseRelease {
        x: u16,
        y: u16,
    },
    PaletteMove(isize),
    SetAxes {
        x: String,
        y: String,
    },
    ToggleGridMode,
    ToggleLandBorders,
    ToggleColorScaleScope,
    ToggleScale,
    TogglePlayback,
    IncreasePlaybackSpeed,
    DecreasePlaybackSpeed,
    TickPlayback,
    PreviousFile,
    NextFile,
}

#[derive(Debug, Clone)]
pub enum Effect {
    ReadSlice {
        generation: Generation,
        variable: String,
    },
}

#[derive(Debug, Clone)]
pub struct ViewModel {
    pub generation: Generation,
    /// Independent generation for plot/time-series work. Plot reads may run
    /// concurrently with map-slice reads and must not invalidate a slice that
    /// is still being loaded.
    pub plot_generation: Generation,
    pub loading: LoadingState,
    pub slice: Option<Slice2D>,
    pub decoded_bytes: usize,
    pub decoded_limit: usize,
    pub collection_diagnostics: Vec<String>,
    pub collection_progress: Option<(usize, usize)>,
    pub status: String,
    pub selected_variable: Option<String>,
    pub time_index: usize,
    pub time_length: usize,
    pub time_label: Option<String>,
    pub timeline: Vec<TimelinePoint>,
    pub level_label: Option<String>,
    pub playing: bool,
    pub playback_speed: f32,
    pub depth_index: usize,
    pub depth_length: usize,
    pub depth_cursor: usize,
    pub sidebar_focused: bool,
    pub level_labels: Vec<String>,
    pub help_visible: bool,
    pub palette: Palette,
    pub palette_catalog: Vec<Palette>,
    pub limits: Option<(f64, f64)>,
    pub global_limits: Option<(f64, f64)>,
    pub limits_manual: bool,
    pub filter_range: Option<(f64, f64)>,
    pub limit_draft: Option<LimitDraft>,
    pub axis_draft: Option<AxisDraft>,
    pub palette_query: String,
    pub palette_index: usize,
    pub zoom_bounds: Option<Bounds>,
    pub full_bounds: Option<Bounds>,
    pub drag: Option<crate::events::mouse::DragState>,
    pub overlay: Option<Overlay>,
    pub cursor: Option<(u16, u16)>,
    pub hover_point: Option<MapPoint>,
    pub selected_point: Option<(usize, usize)>,
    pub selected_points: Vec<(usize, usize)>,
    pub selected_coordinates: PointCoordinates,
    pub time_series: Vec<(f64, f64)>,
    pub time_series_labels: Vec<String>,
    pub plot_series: Vec<PlotSeries>,
    pub plot_draft: PlotDraft,
    pub variable_search_active: bool,
    pub variable_browser_index: usize,
    pub x_axis: Option<String>,
    pub y_axis: Option<String>,
    pub axis_options: Vec<String>,
    pub grid_mode: GridMode,
    pub show_land_borders: bool,
    pub scale_mode: ScaleMode,
    pub color_scale_scope: ColorScaleScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelinePoint {
    pub source_index: usize,
    pub local_index: usize,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapPoint {
    pub x: u16,
    pub y: u16,
    pub row: usize,
    pub col: usize,
    pub value: Option<f64>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    Limits,
    Filter,
    Axis,
    TimeSeries,
    Plot,
    CommandPalette,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    VariableSearch,
    Sidebar,
    TextOverlay(Overlay),
    PlotOverlay,
    Help,
}

impl ViewModel {
    pub fn input_mode(&self) -> InputMode {
        if self.variable_search_active {
            InputMode::VariableSearch
        } else if let Some(overlay) = self.overlay {
            match overlay {
                Overlay::Limits | Overlay::Filter | Overlay::Axis | Overlay::CommandPalette => {
                    InputMode::TextOverlay(overlay)
                }
                Overlay::Plot | Overlay::TimeSeries => InputMode::PlotOverlay,
            }
        } else if self.help_visible {
            InputMode::Help
        } else if self.sidebar_focused {
            InputMode::Sidebar
        } else {
            InputMode::Normal
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotKind {
    TimeSeries,
    Scatter,
    Histogram,
    Cdf,
    VerticalProfile,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlotSeries {
    pub point: (usize, usize),
    pub label: String,
    pub data: Vec<(f64, f64)>,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotAxisField {
    X,
    Y,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotXAxis {
    ValidTime,
    SampleIndex,
    Longitude,
    Latitude,
    Dimension(usize),
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlotYAxis {
    Value,
    Frequency,
    Density,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlotDraft {
    pub kind: PlotKind,
    pub x_axis: PlotXAxis,
    pub y_axis: PlotYAxis,
    pub active: PlotAxisField,
}

impl Default for PlotDraft {
    fn default() -> Self {
        Self {
            kind: PlotKind::TimeSeries,
            x_axis: PlotXAxis::ValidTime,
            y_axis: PlotYAxis::Value,
            active: PlotAxisField::X,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitField {
    Min,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisField {
    X,
    Y,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxisDraft {
    pub x: String,
    pub y: String,
    pub active: AxisField,
    pub replace_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitDraft {
    pub min: String,
    pub max: String,
    pub active: LimitField,
    pub replace_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridMode {
    Logical,
    Projected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScaleScope {
    CurrentView,
    GlobalView,
}

impl ColorScaleScope {
    pub fn name(self) -> &'static str {
        match self {
            Self::CurrentView => "current",
            Self::GlobalView => "global",
        }
    }
}

impl Default for ViewModel {
    fn default() -> Self {
        Self {
            generation: Generation(0),
            plot_generation: Generation(0),
            loading: LoadingState::Idle,
            slice: None,
            decoded_bytes: 0,
            decoded_limit: DEFAULT_DECODED_WORKING_SET_LIMIT,
            collection_diagnostics: Vec::new(),
            collection_progress: None,
            status: String::new(),
            selected_variable: None,
            time_index: 0,
            time_length: 1,
            time_label: None,
            timeline: Vec::new(),
            level_label: None,
            playing: false,
            playback_speed: 1.0,
            depth_index: 0,
            depth_length: 1,
            depth_cursor: 0,
            sidebar_focused: false,
            level_labels: Vec::new(),
            help_visible: false,
            palette: Palette::Viridis,
            palette_catalog: discover_colormaps(),
            limits: None,
            global_limits: None,
            limits_manual: false,
            filter_range: None,
            limit_draft: None,
            axis_draft: None,
            palette_query: String::new(),
            palette_index: 0,
            zoom_bounds: None,
            full_bounds: None,
            drag: None,
            overlay: None,
            cursor: None,
            hover_point: None,
            selected_point: None,
            selected_points: Vec::new(),
            selected_coordinates: PointCoordinates::default(),
            time_series: Vec::new(),
            time_series_labels: Vec::new(),
            plot_series: Vec::new(),
            plot_draft: PlotDraft::default(),
            variable_search_active: false,
            variable_browser_index: 0,
            x_axis: None,
            y_axis: None,
            axis_options: Vec::new(),
            grid_mode: GridMode::Logical,
            // Coastline polygons are an opt-in presentation overlay. Avoid
            // decoding/indexing them during the initial map render; press b
            // or use the command palette to enable them when needed.
            show_land_borders: false,
            scale_mode: ScaleMode::Linear,
            color_scale_scope: ColorScaleScope::CurrentView,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct AppState {
    pub view: ViewModel,
    pub pending: VecDeque<Effect>,
    pub variables: Vec<Variable>,
    pub variable_query: String,
}

impl AppState {
    pub fn next_generation(&mut self) -> Generation {
        self.view.generation = Generation(self.view.generation.0.saturating_add(1));
        self.view.generation
    }

    pub fn next_plot_generation(&mut self) -> Generation {
        self.view.plot_generation = Generation(self.view.plot_generation.0.saturating_add(1));
        self.view.plot_generation
    }

    pub fn accept_plot(
        &mut self,
        generation: Generation,
        plot_series: Vec<PlotSeries>,
        time_series: Vec<(f64, f64)>,
        time_series_labels: Vec<String>,
        status: String,
    ) -> bool {
        if generation != self.view.plot_generation {
            return false;
        }
        self.view.plot_series = plot_series;
        self.view.time_series = time_series;
        self.view.time_series_labels = time_series_labels;
        self.view.status = status;
        true
    }

    pub fn accept_slice(&mut self, generation: Generation, slice: Slice2D) -> bool {
        if generation != self.view.generation || slice.memory_bytes() > self.view.decoded_limit {
            return false;
        }
        self.set_slice(slice);
        self.view.loading = LoadingState::Ready;
        true
    }

    pub fn set_slice(&mut self, slice: Slice2D) {
        self.view.decoded_bytes = slice.memory_bytes();
        self.view.slice = Some(slice);
    }

    pub fn reduce(&mut self, command: Command) -> Option<Effect> {
        match command {
            command @ (Command::Quit
            | Command::PreviousFile
            | Command::NextFile
            | Command::SetBounds(_)
            | Command::Resize { .. }
            | Command::SelectVariable(_)
            | Command::SelectVariableAt(_)
            | Command::MoveTime(_)
            | Command::TogglePlayback
            | Command::IncreasePlaybackSpeed
            | Command::DecreasePlaybackSpeed
            | Command::TickPlayback
            | Command::SetTime(_)
            | Command::MoveDepth(_)
            | Command::SetDepth(_)
            | Command::ToggleSidebarFocus
            | Command::MoveDepthCursor(_)
            | Command::ApplyDepthCursor
            | Command::PointerScroll { .. }
            | Command::ToggleHelp
            | Command::UpdateVariableQuery(_)) => self.reduce_navigation(command),
            command @ (Command::CyclePalette
            | Command::TogglePaletteReverse
            | Command::CycleImageFilter
            | Command::ExportCurrent
            | Command::AutomaticLimits
            | Command::ManualLimits { .. }
            | Command::OpenLimits
            | Command::OpenFilter
            | Command::ClearFilter
            | Command::OpenCommandPalette) => self.reduce_display(command),
            command @ (Command::OpenPlot
            | Command::SetPlotKind(_)
            | Command::CyclePlotAxis(_)
            | Command::FocusPlotAxis(_)) => self.reduce_plot(command),
            command @ (Command::OpenVariableSearch
            | Command::SubmitVariableSearch
            | Command::ExecuteCommandPalette
            | Command::ExecutePaletteChoice(_)
            | Command::InputChar(_)
            | Command::DeleteInput
            | Command::NextLimitField
            | Command::FocusLimitField(_)
            | Command::FocusAxisField(_)) => self.reduce_text_input(command),
            command @ (Command::Zoom(_)
            | Command::ResetZoom
            | Command::BeginDrag { .. }
            | Command::UpdateDrag { .. }
            | Command::CancelDrag
            | Command::Pan { .. }) => self.reduce_viewport(command),
            command @ (Command::OpenAxisOverlay
            | Command::CycleAxis(_)
            | Command::ApplyLimitDraft
            | Command::ActivatePoint
            | Command::Pointer { .. }
            | Command::HoverPoint { .. }
            | Command::ClearHover
            | Command::SelectPoint { .. }
            | Command::TogglePointSelection
            | Command::MouseClick { .. }
            | Command::MouseRelease { .. }) => self.reduce_axes_and_points(command),
            command @ (Command::PaletteMove(_)
            | Command::SetAxes { .. }
            | Command::ToggleGridMode
            | Command::ToggleLandBorders
            | Command::ToggleColorScaleScope
            | Command::ToggleScale) => self.reduce_commands(command),
        }
    }

    fn reduce_navigation(&mut self, command: Command) -> Option<Effect> {
        match command {
            Command::Quit => {
                if self.view.variable_search_active {
                    self.view.variable_search_active = false;
                    self.variable_query.clear();
                    self.view.variable_browser_index = 0;
                } else if self.view.overlay.is_some() {
                    self.view.overlay = None;
                    self.view.limit_draft = None;
                    self.view.axis_draft = None;
                } else if self.view.help_visible {
                    self.view.help_visible = false;
                }
                None
            }
            Command::PreviousFile | Command::NextFile => None,
            Command::SetBounds(_) | Command::Resize { .. } => None,
            Command::SelectVariable(index) => {
                if self.view.variable_search_active {
                    let visible =
                        crate::ui::sidebar::filter_variables(&self.variables, &self.variable_query);
                    if visible.is_empty() {
                        return None;
                    }
                    let current = self
                        .view
                        .variable_browser_index
                        .min(visible.len().saturating_sub(1));
                    let delta = if index == 0 { -1 } else { 1 };
                    self.view.variable_browser_index = current
                        .saturating_add_signed(delta)
                        .min(visible.len().saturating_sub(1));
                    return None;
                }
                if matches!(self.view.overlay, Some(Overlay::CommandPalette)) {
                    return self.reduce(Command::PaletteMove(if index == 0 { -1 } else { 1 }));
                }
                if matches!(self.view.overlay, Some(Overlay::Axis)) {
                    return self.reduce(Command::CycleAxis(if index == 0 { -1 } else { 1 }));
                }
                if matches!(self.view.overlay, Some(Overlay::Plot)) {
                    return self.reduce(Command::CyclePlotAxis(if index == 0 { -1 } else { 1 }));
                }
                let visible =
                    crate::ui::sidebar::filter_variables(&self.variables, &self.variable_query);
                let target = if let Some(selected) = self.view.selected_variable.as_deref() {
                    if let Some(current) = visible
                        .iter()
                        .position(|variable| variable.name == selected)
                    {
                        if index == 0 {
                            current.saturating_sub(1)
                        } else {
                            (current + 1).min(visible.len().saturating_sub(1))
                        }
                    } else {
                        index.min(visible.len().saturating_sub(1))
                    }
                } else {
                    index.min(visible.len().saturating_sub(1))
                };
                self.select_variable(visible.get(target)?.name.clone())
            }
            Command::SelectVariableAt(target) => {
                let visible =
                    crate::ui::sidebar::filter_variables(&self.variables, &self.variable_query);
                self.select_variable(visible.get(target)?.name.clone())
            }
            Command::MoveTime(delta) => {
                if matches!(self.view.overlay, Some(Overlay::CommandPalette)) {
                    return self.reduce(Command::PaletteMove(delta));
                }
                if matches!(self.view.overlay, Some(Overlay::Plot)) {
                    return self.reduce(Command::CyclePlotAxis(delta));
                }
                self.view.time_index =
                    bounded_index(self.view.time_index, delta, self.view.time_length);
                None
            }
            Command::TogglePlayback => {
                self.view.playing = !self.view.playing;
                None
            }
            Command::IncreasePlaybackSpeed => {
                self.view.playback_speed = (self.view.playback_speed * 2.0).min(16.0);
                None
            }
            Command::DecreasePlaybackSpeed => {
                self.view.playback_speed = (self.view.playback_speed / 2.0).max(0.25);
                None
            }
            Command::TickPlayback => {
                if self.view.playing && self.view.time_length > 0 {
                    self.view.time_index = (self.view.time_index + 1) % self.view.time_length;
                }
                None
            }
            Command::SetTime(index) => {
                self.view.time_index = index.min(self.view.time_length.saturating_sub(1));
                None
            }
            Command::MoveDepth(delta) => {
                self.view.depth_index =
                    bounded_index(self.view.depth_index, delta, self.view.depth_length);
                self.view.depth_cursor = self.view.depth_index;
                None
            }
            Command::SetDepth(index) => {
                self.view.depth_index = index.min(self.view.depth_length.saturating_sub(1));
                self.view.depth_cursor = self.view.depth_index;
                None
            }
            Command::MoveDepthCursor(delta) => {
                self.view.depth_cursor =
                    bounded_index(self.view.depth_cursor, delta, self.view.depth_length);
                None
            }
            Command::ApplyDepthCursor => {
                let cursor = self.view.depth_cursor;
                self.reduce(Command::SetDepth(cursor))
            }
            Command::ToggleSidebarFocus => {
                self.view.sidebar_focused = !self.view.sidebar_focused;
                if self.view.sidebar_focused {
                    self.view.depth_cursor = self.view.depth_index;
                }
                None
            }
            Command::PointerScroll { .. } => None,
            Command::ToggleHelp => {
                self.view.help_visible = !self.view.help_visible;
                None
            }
            Command::UpdateVariableQuery(query) => {
                self.variable_query = query;
                self.view.variable_browser_index = 0;
                None
            }
            _ => unreachable!("navigation reducer received unrelated command"),
        }
    }

    fn reduce_display(&mut self, command: Command) -> Option<Effect> {
        match command {
            Command::CyclePalette => {
                if self.view.palette_catalog.is_empty() {
                    self.view.palette = self.view.palette.clone().next();
                } else {
                    let reversed = self.view.palette.is_reversed();
                    let lookup_palette = if reversed {
                        self.view.palette.clone().toggle_reversed()
                    } else {
                        self.view.palette.clone()
                    };
                    let current = self
                        .view
                        .palette_catalog
                        .iter()
                        .position(|palette| palette == &lookup_palette)
                        .unwrap_or(0);
                    let next = (current + 1) % self.view.palette_catalog.len();
                    self.view.palette = if reversed {
                        self.view.palette_catalog[next].clone().toggle_reversed()
                    } else {
                        self.view.palette_catalog[next].clone()
                    };
                }
                None
            }
            Command::TogglePaletteReverse => {
                self.view.palette = self.view.palette.clone().toggle_reversed();
                None
            }
            Command::CycleImageFilter => None,
            Command::ExportCurrent => None,
            Command::AutomaticLimits => {
                self.view.limits_manual = false;
                self.view.limits = if self.view.scale_mode == ScaleMode::Log {
                    self.view.slice.as_ref().and_then(positive_slice_limits)
                } else {
                    self.view
                        .slice
                        .as_ref()
                        .and_then(|slice| slice.statistics.map(|s| (s.min, s.max)))
                };
                None
            }
            Command::ManualLimits { min, max } => {
                self.apply_manual_limits(min, max);
                None
            }
            Command::OpenLimits => {
                let limits = self
                    .view
                    .limits
                    .or_else(|| {
                        self.view
                            .slice
                            .as_ref()
                            .and_then(|slice| slice.statistics.map(|s| (s.min, s.max)))
                    })
                    .unwrap_or((0.0, 1.0));
                self.view.limit_draft = Some(LimitDraft {
                    min: limits.0.to_string(),
                    max: limits.1.to_string(),
                    active: LimitField::Min,
                    replace_active: true,
                });
                self.view.overlay = Some(Overlay::Limits);
                None
            }
            Command::OpenFilter => {
                let range = self
                    .view
                    .filter_range
                    .or_else(|| {
                        self.view
                            .slice
                            .as_ref()
                            .and_then(|slice| slice.statistics.map(|s| (s.min, s.max)))
                    })
                    .unwrap_or((0.0, 1.0));
                self.view.limit_draft = Some(LimitDraft {
                    min: range.0.to_string(),
                    max: range.1.to_string(),
                    active: LimitField::Min,
                    replace_active: true,
                });
                self.view.overlay = Some(Overlay::Filter);
                None
            }
            Command::ClearFilter => {
                self.view.filter_range = None;
                self.view.limit_draft = None;
                if matches!(self.view.overlay, Some(Overlay::Filter)) {
                    self.view.overlay = None;
                }
                None
            }
            Command::OpenCommandPalette => {
                self.view.help_visible = false;
                self.view.palette_query.clear();
                self.view.palette_index = 0;
                self.view.overlay = Some(Overlay::CommandPalette);
                None
            }
            _ => unreachable!("display reducer received unrelated command"),
        }
    }

    fn reduce_plot(&mut self, command: Command) -> Option<Effect> {
        match command {
            Command::OpenPlot => {
                if matches!(self.view.overlay, Some(Overlay::CommandPalette)) {
                    if self.view.palette_query.len() < 64 {
                        self.view.palette_query.push('p');
                        self.view.palette_index = 0;
                    }
                    return None;
                }
                self.view.help_visible = false;
                if self.view.selected_point.is_none()
                    && let Some(point) = self.view.hover_point.as_ref()
                {
                    self.view.selected_point = Some((point.row, point.col));
                    self.view.selected_points = vec![(point.row, point.col)];
                    self.view.selected_coordinates = PointCoordinates {
                        latitude: point.latitude,
                        longitude: point.longitude,
                    };
                }
                self.view.plot_draft = PlotDraft::default();
                self.view.overlay = Some(Overlay::Plot);
                None
            }
            Command::SetPlotKind(kind) => {
                if matches!(self.view.overlay, Some(Overlay::CommandPalette)) {
                    let character = match kind {
                        PlotKind::TimeSeries => 't',
                        PlotKind::Scatter => 'd',
                        PlotKind::Histogram => 'h',
                        PlotKind::Cdf => 'k',
                        PlotKind::VerticalProfile => 'u',
                    };
                    if self.view.palette_query.len() < 64 {
                        self.view.palette_query.push(character);
                        self.view.palette_index = 0;
                    }
                    return None;
                }
                self.view.plot_draft.kind = kind;
                self.view.plot_draft.x_axis = match kind {
                    PlotKind::TimeSeries | PlotKind::Scatter => PlotXAxis::ValidTime,
                    PlotKind::Histogram | PlotKind::Cdf => PlotXAxis::Value,
                    PlotKind::VerticalProfile => self
                        .view
                        .axis_options
                        .iter()
                        .enumerate()
                        .find(|(_, name)| is_vertical_dimension(name))
                        .map_or(PlotXAxis::ValidTime, |(index, _)| {
                            PlotXAxis::Dimension(index)
                        }),
                };
                self.view.plot_draft.y_axis = match kind {
                    PlotKind::TimeSeries | PlotKind::Scatter => PlotYAxis::Value,
                    PlotKind::Histogram => PlotYAxis::Frequency,
                    PlotKind::Cdf => PlotYAxis::Density,
                    PlotKind::VerticalProfile => PlotYAxis::Value,
                };
                None
            }
            Command::CyclePlotAxis(delta) => {
                let draft = &mut self.view.plot_draft;
                match (draft.kind, draft.active) {
                    (
                        PlotKind::TimeSeries | PlotKind::Scatter | PlotKind::VerticalProfile,
                        PlotAxisField::X,
                    ) => {
                        let has_point = !self.view.selected_points.is_empty()
                            || self.view.selected_point.is_some();
                        let axes = plot_axis_options(&self.view.axis_options, has_point);
                        let current = axes
                            .iter()
                            .position(|axis| *axis == draft.x_axis)
                            .unwrap_or(0);
                        let next = (current as isize + delta.signum())
                            .rem_euclid(axes.len() as isize)
                            as usize;
                        draft.x_axis = axes[next];
                    }
                    (PlotKind::Histogram, PlotAxisField::Y) => {
                        draft.y_axis = match (draft.y_axis, delta.signum()) {
                            (PlotYAxis::Frequency, 1) => PlotYAxis::Density,
                            (PlotYAxis::Density, -1) => PlotYAxis::Frequency,
                            (PlotYAxis::Density, 1) => PlotYAxis::Frequency,
                            (PlotYAxis::Frequency, -1) => PlotYAxis::Density,
                            (current, _) => current,
                        };
                    }
                    _ => {}
                }
                None
            }
            Command::FocusPlotAxis(field) => {
                if matches!(self.view.overlay, Some(Overlay::Plot)) {
                    self.view.plot_draft.active = field;
                }
                None
            }
            _ => unreachable!("plot reducer received unrelated command"),
        }
    }

    fn reduce_text_input(&mut self, command: Command) -> Option<Effect> {
        match command {
            Command::OpenVariableSearch => {
                self.view.help_visible = false;
                self.view.variable_search_active = true;
                self.view.variable_browser_index =
                    crate::ui::sidebar::filter_variables(&self.variables, "")
                        .iter()
                        .position(|variable| {
                            self.view.selected_variable.as_deref() == Some(variable.name.as_str())
                        })
                        .unwrap_or(0);
                self.view.status =
                    "browse variables: type to filter, Enter loads the selected field, Esc closes"
                        .into();
                None
            }
            Command::SubmitVariableSearch => {
                self.view.variable_search_active = false;
                let visible =
                    crate::ui::sidebar::filter_variables(&self.variables, &self.variable_query);
                let target = self
                    .view
                    .variable_browser_index
                    .min(visible.len().saturating_sub(1));
                if let Some(variable) = visible.get(target) {
                    self.select_variable(variable.name.clone())
                } else {
                    self.view.status = "no matching variables".into();
                    None
                }
            }
            Command::ExecuteCommandPalette => {
                if !matches!(self.view.overlay, Some(Overlay::CommandPalette)) {
                    return None;
                }
                let choices = palette_matches(&self.view.palette_query);
                let &choice = choices.get(self.view.palette_index)?;
                self.view.overlay = None;
                self.view.palette_query.clear();
                self.view.palette_index = 0;
                self.reduce(palette_command(choice))
            }
            Command::ExecutePaletteChoice(index) => {
                if !matches!(self.view.overlay, Some(Overlay::CommandPalette)) {
                    return None;
                }
                let choices = palette_matches(&self.view.palette_query);
                let &choice = choices.get(index)?;
                self.view.overlay = None;
                self.view.palette_query.clear();
                self.view.palette_index = 0;
                self.reduce(palette_command(choice))
            }
            Command::InputChar(character) => {
                if matches!(self.view.overlay, Some(Overlay::CommandPalette)) {
                    if !character.is_control() && self.view.palette_query.len() < 64 {
                        self.view.palette_query.push(character);
                        self.view.palette_index = 0;
                    }
                } else if self.view.variable_search_active
                    && (character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.'))
                {
                    if self.variable_query.len() < 64 {
                        self.variable_query.push(character);
                        self.view.variable_browser_index = 0;
                    }
                } else if matches!(self.view.overlay, Some(Overlay::Axis))
                    && let Some(draft) = self.view.axis_draft.as_mut()
                    && (character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
                {
                    let target = match draft.active {
                        AxisField::X => &mut draft.x,
                        AxisField::Y => &mut draft.y,
                    };
                    if draft.replace_active {
                        target.clear();
                        draft.replace_active = false;
                    }
                    if target.len() < 64 {
                        target.push(character);
                    }
                } else if matches!(self.view.overlay, Some(Overlay::Limits | Overlay::Filter))
                    && let Some(draft) = self.view.limit_draft.as_mut()
                    && (character.is_ascii_digit()
                        || matches!(character, '-' | '+' | '.' | 'e' | 'E'))
                {
                    let target = match draft.active {
                        LimitField::Min => &mut draft.min,
                        LimitField::Max => &mut draft.max,
                    };
                    if draft.replace_active {
                        target.clear();
                        draft.replace_active = false;
                    }
                    if target.len() < 32 {
                        target.push(character);
                    }
                }
                None
            }
            Command::DeleteInput => {
                if matches!(self.view.overlay, Some(Overlay::CommandPalette)) {
                    self.view.palette_query.pop();
                    self.view.palette_index = 0;
                } else if self.view.variable_search_active {
                    self.variable_query.pop();
                    self.view.variable_browser_index = 0;
                } else if matches!(self.view.overlay, Some(Overlay::Axis))
                    && let Some(draft) = self.view.axis_draft.as_mut()
                {
                    let target = match draft.active {
                        AxisField::X => &mut draft.x,
                        AxisField::Y => &mut draft.y,
                    };
                    if draft.replace_active {
                        target.clear();
                        draft.replace_active = false;
                    } else {
                        target.pop();
                    }
                } else if matches!(self.view.overlay, Some(Overlay::Limits | Overlay::Filter))
                    && let Some(draft) = self.view.limit_draft.as_mut()
                {
                    let target = match draft.active {
                        LimitField::Min => &mut draft.min,
                        LimitField::Max => &mut draft.max,
                    };
                    if draft.replace_active {
                        target.clear();
                        draft.replace_active = false;
                    } else {
                        target.pop();
                    }
                }
                None
            }
            Command::NextLimitField => {
                if matches!(self.view.overlay, Some(Overlay::CommandPalette)) {
                    return self.reduce(Command::PaletteMove(1));
                }
                if matches!(self.view.overlay, Some(Overlay::Axis))
                    && let Some(draft) = self.view.axis_draft.as_mut()
                {
                    draft.active = match draft.active {
                        AxisField::X => AxisField::Y,
                        AxisField::Y => AxisField::X,
                    };
                    draft.replace_active = true;
                    return None;
                }
                if matches!(self.view.overlay, Some(Overlay::Plot)) {
                    self.view.plot_draft.active = match self.view.plot_draft.active {
                        PlotAxisField::X => PlotAxisField::Y,
                        PlotAxisField::Y => PlotAxisField::X,
                    };
                    return None;
                }
                if let Some(draft) = self.view.limit_draft.as_mut()
                    && matches!(self.view.overlay, Some(Overlay::Limits | Overlay::Filter))
                {
                    draft.active = match draft.active {
                        LimitField::Min => LimitField::Max,
                        LimitField::Max => LimitField::Min,
                    };
                    draft.replace_active = true;
                }
                None
            }
            Command::FocusLimitField(field) => {
                if let Some(draft) = self.view.limit_draft.as_mut()
                    && matches!(self.view.overlay, Some(Overlay::Limits | Overlay::Filter))
                {
                    draft.active = field;
                    draft.replace_active = true;
                }
                None
            }
            Command::FocusAxisField(field) => {
                if let Some(draft) = self.view.axis_draft.as_mut()
                    && matches!(self.view.overlay, Some(Overlay::Axis))
                {
                    draft.active = field;
                    draft.replace_active = true;
                }
                None
            }
            _ => unreachable!("text-input reducer received unrelated command"),
        }
    }

    fn reduce_viewport(&mut self, command: Command) -> Option<Effect> {
        match command {
            Command::Zoom(bounds) => {
                self.view.zoom_bounds = Some(bounds);
                self.view.drag = None;
                None
            }
            Command::ResetZoom => {
                self.view.zoom_bounds = None;
                self.view.drag = None;
                None
            }
            Command::BeginDrag { x, y, zoom } => {
                self.view.drag = Some(crate::events::mouse::DragState {
                    start: (x, y),
                    current: (x, y),
                    zoom,
                });
                None
            }
            Command::UpdateDrag { x, y } => {
                if let Some(drag) = self.view.drag.as_mut() {
                    drag.current = (x, y);
                }
                None
            }
            Command::CancelDrag => {
                self.view.drag = None;
                None
            }
            Command::Pan { rows, cols } => {
                self.view.drag = None;
                let full = self
                    .view
                    .full_bounds
                    .or_else(|| self.view.slice.as_ref().map(|slice| slice.source_bounds))?;
                let current = self.view.zoom_bounds.unwrap_or(full);
                let row_span = current.row_end - current.row_start;
                let col_span = current.col_end - current.col_start;
                let max_row_start = full.row_end.saturating_sub(row_span);
                let max_col_start = full.col_end.saturating_sub(col_span);
                let row_start = current
                    .row_start
                    .saturating_add_signed(rows)
                    .clamp(full.row_start, max_row_start);
                let col_start = current
                    .col_start
                    .saturating_add_signed(cols)
                    .clamp(full.col_start, max_col_start);
                self.view.zoom_bounds = Bounds::new(
                    row_start,
                    row_start + row_span,
                    col_start,
                    col_start + col_span,
                )
                .ok();
                None
            }
            _ => unreachable!("viewport reducer received unrelated command"),
        }
    }

    fn reduce_axes_and_points(&mut self, command: Command) -> Option<Effect> {
        match command {
            Command::OpenAxisOverlay => {
                let (x, y) = default_axes(&self.view.axis_options);
                self.view.axis_draft = Some(AxisDraft {
                    x: self.view.x_axis.clone().unwrap_or(x),
                    y: self.view.y_axis.clone().unwrap_or(y),
                    active: AxisField::X,
                    replace_active: true,
                });
                self.view.overlay = Some(Overlay::Axis);
                None
            }
            Command::CycleAxis(delta) => {
                let options = &self.view.axis_options;
                if options.is_empty() {
                    return None;
                }
                let draft = self.view.axis_draft.as_mut()?;
                let active = draft.active;
                let current_name = match active {
                    AxisField::X => draft.x.clone(),
                    AxisField::Y => draft.y.clone(),
                };
                let current = options
                    .iter()
                    .position(|option| option.eq_ignore_ascii_case(&current_name))
                    .unwrap_or(0);
                let next = current
                    .saturating_add_signed(delta)
                    .min(options.len().saturating_sub(1));
                let candidate = options[next].clone();
                let other = match active {
                    AxisField::X => draft.y.clone(),
                    AxisField::Y => draft.x.clone(),
                };
                if !candidate.eq_ignore_ascii_case(&other) {
                    match active {
                        AxisField::X => draft.x = candidate,
                        AxisField::Y => draft.y = candidate,
                    }
                    draft.replace_active = false;
                }
                None
            }
            Command::ApplyLimitDraft => {
                if !matches!(self.view.overlay, Some(Overlay::Limits | Overlay::Filter)) {
                    return None;
                }
                let draft = self.view.limit_draft.clone()?;
                let min = draft.min.parse::<f64>();
                let max = draft.max.parse::<f64>();
                match (min, max) {
                    (Ok(min), Ok(max)) if self.view.overlay == Some(Overlay::Filter) => {
                        self.apply_filter(min, max)
                    }
                    (Ok(min), Ok(max)) => self.apply_manual_limits(min, max),
                    _ => self.view.status = "limits must be valid numbers".into(),
                }
                None
            }
            Command::ActivatePoint => {
                if matches!(self.view.overlay, Some(Overlay::CommandPalette)) {
                    return self.reduce(Command::ExecuteCommandPalette);
                }
                if matches!(self.view.overlay, Some(Overlay::Axis)) {
                    let draft = self.view.axis_draft.clone()?;
                    return self.reduce(Command::SetAxes {
                        x: draft.x,
                        y: draft.y,
                    });
                }
                if matches!(self.view.overlay, Some(Overlay::Plot)) {
                    return None;
                }
                if matches!(self.view.overlay, Some(Overlay::Limits | Overlay::Filter)) {
                    return self.reduce(Command::ApplyLimitDraft);
                }
                if self.view.selected_point.is_none()
                    && let Some(point) = self.view.hover_point.as_ref()
                {
                    self.view.selected_point = Some((point.row, point.col));
                    self.view.selected_points = vec![(point.row, point.col)];
                    self.view.selected_coordinates = PointCoordinates {
                        latitude: point.latitude,
                        longitude: point.longitude,
                    };
                }
                self.view.plot_draft = PlotDraft::default();
                self.view.overlay = Some(Overlay::Plot);
                None
            }
            Command::Pointer { x, y } => {
                self.view.cursor = Some((x, y));
                None
            }
            Command::HoverPoint {
                x,
                y,
                row,
                col,
                value,
            } => {
                self.view.cursor = Some((x, y));
                self.view.hover_point = Some(MapPoint {
                    x,
                    y,
                    row,
                    col,
                    value,
                    latitude: None,
                    longitude: None,
                });
                None
            }
            Command::ClearHover => {
                self.view.hover_point = None;
                None
            }
            Command::SelectPoint { row, col } => {
                self.view.drag = None;
                self.view.selected_point = Some((row, col));
                self.view.selected_points = vec![(row, col)];
                self.view.selected_coordinates = PointCoordinates::default();
                self.view.time_series.clear();
                self.view.time_series_labels.clear();
                self.view.plot_series.clear();
                self.view.status =
                    format!("point selected: row {row}  col {col}  press Enter for plot choices");
                None
            }
            Command::TogglePointSelection => {
                let Some(point) = self
                    .view
                    .hover_point
                    .as_ref()
                    .map(|point| (point.row, point.col))
                else {
                    self.view.status = "hover a map point before toggling its selection".into();
                    return None;
                };
                if let Some(index) = self
                    .view
                    .selected_points
                    .iter()
                    .position(|item| *item == point)
                {
                    self.view.selected_points.remove(index);
                } else {
                    self.view.selected_points.push(point);
                }
                self.view.selected_point = self.view.selected_points.last().copied();
                if self.view.selected_points.is_empty() {
                    self.view.plot_draft.x_axis = PlotXAxis::ValidTime;
                }
                self.view.selected_coordinates = PointCoordinates::default();
                self.view.time_series.clear();
                self.view.time_series_labels.clear();
                self.view.plot_series.clear();
                self.view.status = if self.view.selected_points.is_empty() {
                    "no points selected; hover a point and press m to add one".into()
                } else {
                    format!(
                        "{} point(s) selected; press Enter for plot choices",
                        self.view.selected_points.len()
                    )
                };
                None
            }
            Command::MouseClick { .. } => None,
            Command::MouseRelease { .. } => None,
            _ => unreachable!("axes-and-points reducer received unrelated command"),
        }
    }

    fn reduce_commands(&mut self, command: Command) -> Option<Effect> {
        match command {
            Command::PaletteMove(delta) => {
                if matches!(self.view.overlay, Some(Overlay::CommandPalette)) {
                    let length = palette_matches(&self.view.palette_query).len();
                    if length > 0 {
                        self.view.palette_index =
                            bounded_index(self.view.palette_index, delta, length);
                    }
                }
                None
            }
            Command::SetAxes { x, y } => {
                if x.eq_ignore_ascii_case(&y) {
                    self.view.status = "X and Y axes must be distinct".into();
                } else if x.trim().is_empty() || y.trim().is_empty() {
                    self.view.status = "X and Y axes cannot be empty".into();
                } else if !self.view.axis_options.is_empty()
                    && (!self
                        .view
                        .axis_options
                        .iter()
                        .any(|axis| axis.eq_ignore_ascii_case(&x))
                        || !self
                            .view
                            .axis_options
                            .iter()
                            .any(|axis| axis.eq_ignore_ascii_case(&y)))
                {
                    self.view.status = "axes must be dimensions of the selected variable".into();
                } else {
                    let swap = axis_pair_is_reversed(
                        &x,
                        &y,
                        self.view.x_axis.as_deref(),
                        self.view.y_axis.as_deref(),
                    );
                    self.view.x_axis = Some(x);
                    self.view.y_axis = Some(y);
                    self.view.axis_draft = None;
                    self.view.overlay = None;
                    self.view.hover_point = None;
                    self.view.selected_point = None;
                    self.view.selected_points.clear();
                    self.view.selected_coordinates = PointCoordinates::default();
                    self.view.time_series.clear();
                    self.view.time_series_labels.clear();
                    self.view.plot_series.clear();
                    self.view.zoom_bounds = None;
                    self.view.full_bounds = None;
                    if swap && let Some(slice) = self.view.slice.take() {
                        match slice.permuted_axes() {
                            Ok(permuted) => self.set_slice(permuted),
                            Err(error) => self.view.status = error.to_string(),
                        }
                    }
                }
                None
            }
            Command::ToggleGridMode => {
                self.view.grid_mode = match self.view.grid_mode {
                    GridMode::Logical => GridMode::Projected,
                    GridMode::Projected => GridMode::Logical,
                };
                None
            }
            Command::ToggleLandBorders => {
                self.view.show_land_borders = !self.view.show_land_borders;
                None
            }
            Command::ToggleColorScaleScope => {
                self.view.color_scale_scope = match self.view.color_scale_scope {
                    ColorScaleScope::CurrentView => ColorScaleScope::GlobalView,
                    ColorScaleScope::GlobalView => ColorScaleScope::CurrentView,
                };
                if !self.view.limits_manual {
                    // Changing the scope is an explicit request for a new
                    // automatic range. Once recomputed, time navigation keeps
                    // that range stable until another explicit limits action.
                    self.view.limits = None;
                }
                None
            }
            Command::ToggleScale => {
                self.view.scale_mode = match self.view.scale_mode {
                    ScaleMode::Linear => {
                        if self
                            .view
                            .limits
                            .is_some_and(|(min, max)| min <= 0.0 || max <= 0.0)
                        {
                            self.view.limits =
                                self.view.slice.as_ref().and_then(positive_slice_limits);
                            self.view.limits_manual = false;
                        }
                        ScaleMode::Log
                    }
                    ScaleMode::Log => ScaleMode::Linear,
                };
                if self.view.scale_mode == ScaleMode::Log && self.view.limits.is_none() {
                    self.view.status = "log scale requires at least one positive value".into();
                }
                None
            }
            _ => unreachable!("command reducer received unrelated command"),
        }
    }

    fn select_variable(&mut self, variable: String) -> Option<Effect> {
        self.view.variable_search_active = false;
        self.variable_query.clear();
        self.view.variable_browser_index = 0;
        self.view.selected_variable = Some(variable.clone());
        if let Some(selected) = self.variables.iter().find(|item| item.name == variable) {
            self.view.axis_options = selected.dimensions.clone();
        }
        self.view.x_axis = None;
        self.view.y_axis = None;
        self.view.selected_point = None;
        self.view.selected_points.clear();
        self.view.selected_coordinates = PointCoordinates::default();
        self.view.hover_point = None;
        self.view.zoom_bounds = None;
        self.view.full_bounds = None;
        self.view.limits = None;
        self.view.global_limits = None;
        self.view.limits_manual = false;
        self.view.drag = None;
        self.view.time_series.clear();
        self.view.time_series_labels.clear();
        self.view.plot_series.clear();
        self.view.sidebar_focused = false;
        self.view.depth_cursor = 0;
        self.view.level_labels.clear();
        let generation = self.next_generation();
        self.view.loading = LoadingState::Loading;
        let effect = Effect::ReadSlice {
            generation,
            variable,
        };
        self.pending.push_back(effect.clone());
        Some(effect)
    }

    fn apply_manual_limits(&mut self, min: f64, max: f64) {
        if min.is_finite() && max.is_finite() && min < max {
            self.view.limits = Some((min, max));
            self.view.limits_manual = true;
            self.view.overlay = None;
            self.view.limit_draft = None;
        } else {
            self.view.status = "limits require finite min < max".into();
        }
    }

    fn apply_filter(&mut self, min: f64, max: f64) {
        if min.is_finite() && max.is_finite() && min <= max {
            self.view.filter_range = Some((min, max));
            self.view.overlay = None;
            self.view.limit_draft = None;
        } else {
            self.view.status = "filter requires finite minimum ≤ maximum".into();
        }
    }
}

fn bounded_index(index: usize, delta: isize, length: usize) -> usize {
    if length == 0 {
        return 0;
    }
    index.saturating_add_signed(delta).min(length - 1)
}

fn plot_axis_options(dimensions: &[String], has_point: bool) -> Vec<PlotXAxis> {
    let mut axes = vec![PlotXAxis::ValidTime, PlotXAxis::SampleIndex];
    if !has_point {
        return axes;
    }
    if dimensions.is_empty() {
        axes.extend([PlotXAxis::Longitude, PlotXAxis::Latitude]);
        return axes;
    }
    for (index, dimension) in dimensions.iter().enumerate() {
        let lower = dimension.to_ascii_lowercase();
        let axis = if lower.contains("lon") || lower == "x" {
            PlotXAxis::Longitude
        } else if lower.contains("lat") || lower == "y" {
            PlotXAxis::Latitude
        } else if lower.contains("time") || lower == "date" {
            continue;
        } else {
            PlotXAxis::Dimension(index)
        };
        if !axes.contains(&axis) {
            axes.push(axis);
        }
    }
    axes
}

fn is_vertical_dimension(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("depth")
        || lower.contains("level")
        || lower.contains("lev")
        || lower.contains("pressure")
        || lower.contains("isobaric")
        || lower.contains("height")
        || lower.contains("hybrid")
}

fn default_axes(options: &[String]) -> (String, String) {
    let x = options
        .iter()
        .find(|name| {
            let lower = name.to_ascii_lowercase();
            lower.contains("lon") || lower == "x"
        })
        .cloned()
        .or_else(|| options.last().cloned())
        .unwrap_or_else(|| "lon".into());
    let y = options
        .iter()
        .find(|name| {
            let lower = name.to_ascii_lowercase();
            (lower.contains("lat") || lower == "y") && !name.eq_ignore_ascii_case(&x)
        })
        .cloned()
        .or_else(|| {
            options
                .iter()
                .find(|name| !name.eq_ignore_ascii_case(&x))
                .cloned()
        })
        .unwrap_or_else(|| "lat".into());
    (x, y)
}

pub fn positive_slice_limits(slice: &crate::data::slice::Slice2D) -> Option<(f64, f64)> {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for ((row, col), value) in slice.values.indexed_iter() {
        if slice.validity[(row, col)] == crate::data::slice::Validity::Finite && *value > 0.0 {
            min = min.min(*value);
            max = max.max(*value);
        }
    }
    min.is_finite().then_some((min, max))
}

fn axis_pair_is_reversed(
    x: &str,
    y: &str,
    current_x: Option<&str>,
    current_y: Option<&str>,
) -> bool {
    let x = x.to_ascii_lowercase();
    let y = y.to_ascii_lowercase();
    if let (Some(current_x), Some(current_y)) = (current_x, current_y) {
        return x == current_y.to_ascii_lowercase() && y == current_x.to_ascii_lowercase();
    }
    (x.contains("lat") && y.contains("lon")) || (x == "y" && y == "x")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteEntry {
    pub label: &'static str,
    pub shortcut: &'static str,
}

pub const COMMAND_PALETTE: &[PaletteEntry] = &[
    PaletteEntry {
        label: "Show help",
        shortcut: "?",
    },
    PaletteEntry {
        label: "Cycle colormap",
        shortcut: "c",
    },
    PaletteEntry {
        label: "Reverse colormap",
        shortcut: "v",
    },
    PaletteEntry {
        label: "Cycle image interpolation",
        shortcut: "i",
    },
    PaletteEntry {
        label: "Export current slice with colorbar",
        shortcut: "e",
    },
    PaletteEntry {
        label: "Automatic color limits",
        shortcut: "a",
    },
    PaletteEntry {
        label: "Toggle current/global color scale",
        shortcut: "z",
    },
    PaletteEntry {
        label: "Edit color limits",
        shortcut: "l",
    },
    PaletteEntry {
        label: "Mask data outside range",
        shortcut: "f",
    },
    PaletteEntry {
        label: "Clear data mask",
        shortcut: "",
    },
    PaletteEntry {
        label: "Reset zoom",
        shortcut: "r",
    },
    PaletteEntry {
        label: "Toggle logical/projected grid",
        shortcut: "g",
    },
    PaletteEntry {
        label: "Toggle filled map backdrop",
        shortcut: "b",
    },
    PaletteEntry {
        label: "Toggle linear/log color scale",
        shortcut: "s",
    },
    PaletteEntry {
        label: "Increase playback speed",
        shortcut: "",
    },
    PaletteEntry {
        label: "Decrease playback speed",
        shortcut: "",
    },
    PaletteEntry {
        label: "Open axis selector",
        shortcut: "x",
    },
    PaletteEntry {
        label: "Previous variable",
        shortcut: "↑",
    },
    PaletteEntry {
        label: "Next variable",
        shortcut: "↓",
    },
    PaletteEntry {
        label: "Previous time slice",
        shortcut: "←",
    },
    PaletteEntry {
        label: "Next time slice",
        shortcut: "→",
    },
    PaletteEntry {
        label: "Previous depth slice",
        shortcut: "[",
    },
    PaletteEntry {
        label: "Next depth slice",
        shortcut: "]",
    },
    PaletteEntry {
        label: "Focus level list",
        shortcut: "Tab",
    },
    PaletteEntry {
        label: "Search variables",
        shortcut: "/",
    },
    PaletteEntry {
        label: "Previous file",
        shortcut: "{",
    },
    PaletteEntry {
        label: "Next file",
        shortcut: "}",
    },
];

pub fn palette_matches(query: &str) -> Vec<usize> {
    let query = query.to_ascii_lowercase();
    COMMAND_PALETTE
        .iter()
        .enumerate()
        .filter(|(_, entry)| fuzzy_match(entry.label, &query))
        .map(|(index, _)| index)
        .collect()
}

fn fuzzy_match(value: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let normalized = value.to_ascii_lowercase();
    let mut chars = normalized.chars();
    query
        .chars()
        .all(|needle| chars.by_ref().any(|candidate| candidate == needle))
}

fn palette_command(index: usize) -> Command {
    match index {
        0 => Command::ToggleHelp,
        1 => Command::CyclePalette,
        2 => Command::TogglePaletteReverse,
        3 => Command::CycleImageFilter,
        4 => Command::ExportCurrent,
        5 => Command::AutomaticLimits,
        6 => Command::ToggleColorScaleScope,
        7 => Command::OpenLimits,
        8 => Command::OpenFilter,
        9 => Command::ClearFilter,
        10 => Command::ResetZoom,
        11 => Command::ToggleGridMode,
        12 => Command::ToggleLandBorders,
        13 => Command::ToggleScale,
        14 => Command::IncreasePlaybackSpeed,
        15 => Command::DecreasePlaybackSpeed,
        16 => Command::OpenAxisOverlay,
        17 => Command::SelectVariable(0),
        18 => Command::SelectVariable(1),
        19 => Command::MoveTime(-1),
        20 => Command::MoveTime(1),
        21 => Command::MoveDepth(-1),
        22 => Command::MoveDepth(1),
        23 => Command::ToggleSidebarFocus,
        24 => Command::OpenVariableSearch,
        25 => Command::PreviousFile,
        26 => Command::NextFile,
        _ => Command::ToggleHelp,
    }
}
