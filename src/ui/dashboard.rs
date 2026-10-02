use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::ViewModel;
use crate::data::{AxisRole, DatasetMetadata};
use crate::render::protocol::GraphicsRenderer;

use super::{canvas, colorbar, help, layout, popup, sidebar, status, theme, timeline};

/// Format a point value without hiding precision behind a fixed six-place
/// decimal display. Scientific notation is used for values that would be
/// difficult to read accurately in the status bar.
pub fn format_point_value(value: f64) -> String {
    let magnitude = value.abs();
    if value.is_finite() && magnitude > 0.0 && !(1.0e-4..1.0e6).contains(&magnitude) {
        format!("{value:.8e}")
    } else {
        format!("{value:.6}")
    }
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    view: &ViewModel,
    filename: &str,
    metadata: &DatasetMetadata,
    plottable: &[crate::data::Variable],
) {
    render_with_search(frame, area, view, filename, metadata, plottable, "", false);
}

#[allow(clippy::too_many_arguments)]
pub fn render_with_search(
    frame: &mut Frame,
    area: Rect,
    view: &ViewModel,
    filename: &str,
    metadata: &DatasetMetadata,
    plottable: &[crate::data::Variable],
    variable_query: &str,
    variable_search_active: bool,
) {
    render_with_search_and_image(
        frame,
        area,
        view,
        filename,
        metadata,
        plottable,
        variable_query,
        variable_search_active,
        None,
        None,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn render_with_search_and_image(
    frame: &mut Frame,
    area: Rect,
    view: &ViewModel,
    filename: &str,
    metadata: &DatasetMetadata,
    plottable: &[crate::data::Variable],
    variable_query: &str,
    variable_search_active: bool,
    graphics: Option<&mut GraphicsRenderer>,
    mut chart_graphics: Option<&mut GraphicsRenderer>,
) {
    let graphics_label = graphics
        .as_ref()
        .map(|renderer| renderer.mode_label())
        .unwrap_or("cell fallback");
    let show_land_borders = view.show_land_borders && spatial_axes_selected(view, metadata);
    frame.render_widget(
        Block::default().style(Style::default().bg(theme::BASE)),
        area,
    );
    let areas = layout::dashboard(area, view.depth_length > 1);
    let header = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme::SURFACE_ALT))
        .style(Style::default().bg(theme::MANTLE));
    frame.render_widget(header, areas.header);
    let header_line = Line::from(vec![
        Span::styled(" ncv ", theme::title_style(theme::MAUVE)),
        Span::styled(
            format!("v{}", env!("CARGO_PKG_VERSION")),
            Style::default().fg(theme::SUBTEXT),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(theme::ICON_FILE, theme::title_style(theme::BLUE)),
        Span::styled(format!(" {filename}"), Style::default().fg(theme::TEXT)),
        Span::styled("  •  ", Style::default().fg(theme::SURFACE_ALT)),
        Span::styled(
            format!("gfx: {graphics_label}"),
            Style::default().fg(theme::TEAL),
        ),
        Span::styled("  •  ", Style::default().fg(theme::SURFACE_ALT)),
        Span::styled(
            format!("{} Ctrl-P/: commands", theme::ICON_COMMAND),
            Style::default().fg(theme::TEAL),
        ),
        Span::styled(
            "  ? help  hover/click map  Enter series  e export  { / } files  q quit",
            theme::muted_style(),
        ),
    ]);
    frame.render_widget(Paragraph::new(header_line), areas.header);
    let limits = view
        .slice
        .as_ref()
        .and_then(|slice| slice.statistics.map(|s| (s.min, s.max)));
    sidebar::render_with_search(
        frame,
        areas.sidebar,
        filename,
        metadata,
        view.selected_variable.as_deref(),
        &view.palette,
        view.limits.or(limits),
        view.filter_range,
        view.scale_mode,
        view.color_scale_scope,
        variable_query,
        variable_search_active,
        plottable,
        view.slice.as_ref(),
        (view.depth_length > 1).then(|| crate::ui::level::LevelPanel {
            labels: &view.level_labels,
            selected: view.depth_index,
            cursor: view.depth_cursor,
            focused: view.sidebar_focused,
        }),
        &view.fixed_dimensions,
        view.focused_fixed_dimension,
    );
    canvas::render_with_points_and_image(
        frame,
        areas.canvas,
        view.loading == crate::app::LoadingState::Loading,
        areas.constrained,
        view.slice.as_ref(),
        view.palette.clone(),
        view.limits.or(limits),
        view.filter_range,
        show_land_borders,
        view.scale_mode,
        view.hover_point
            .as_ref()
            .map(|point| (point.row, point.col)),
        view.selected_point,
        view.drag,
        view.zoom_bounds.is_some(),
        view.generation.0,
        graphics,
    );
    let selected_metadata = metadata
        .variables
        .iter()
        .find(|variable| view.selected_variable.as_deref() == Some(variable.name.as_str()));
    colorbar::render_with_metadata(
        frame,
        areas.legend,
        &view.palette,
        view.limits.or(limits),
        view.scale_mode,
        view.selected_variable.as_deref(),
        selected_metadata.and_then(|variable| variable.units.as_deref()),
        selected_metadata.and_then(|variable| variable.long_name.as_deref()),
        selected_metadata.and_then(|variable| variable.standard_name.as_deref()),
        selected_level_label(metadata, selected_metadata, view).as_deref(),
    );
    if !areas.level.is_empty() {
        let level_text =
            selected_level_label(metadata, selected_metadata, view).unwrap_or_else(|| {
                format!(
                    "index {} of {}",
                    view.depth_index,
                    view.depth_length.saturating_sub(1)
                )
            });
        crate::ui::level::render_gauge(
            frame,
            areas.level,
            &level_text,
            view.depth_index,
            view.depth_length,
        );
    }
    timeline::render(
        frame,
        areas.timeline,
        view.time_index,
        view.time_length,
        view.time_label.as_deref(),
        view.playing,
        view.playback_speed,
    );
    let status = if view.help_visible {
        "Keys: arrows navigate  [/] depth  Tab: level list  click map to select  Enter: plots  m: add point  ? help"
            .to_string()
    } else if let Some(drag) = view.drag {
        if view.zoom_bounds.is_some() && !drag.zoom {
            "dragging map — release to pan  •  r resets zoom".to_string()
        } else {
            format!(
                "zoom box  ({}, {}) → ({}, {})  •  release to zoom",
                drag.start.0, drag.start.1, drag.current.0, drag.current.1,
            )
        }
    } else if (view.variable_search_active || view.overlay.is_some()) && !view.status.is_empty() {
        view.status.clone()
    } else if let Some(point) = view.hover_point.as_ref() {
        let (row_dimension, column_dimension) = plane_dimension_names(view, selected_metadata);
        let value = view
            .slice
            .as_ref()
            .and_then(|slice| slice.value_at_source(point.row, point.col))
            .or(point.value)
            .map_or_else(|| "masked".to_string(), format_point_value);
        let selected = if view.selected_point == Some((point.row, point.col)) {
            "  selected"
        } else {
            ""
        };
        let latitude = point_axis_label("lat", row_dimension, point.latitude, point.row);
        let longitude = point_axis_label("lon", column_dimension, point.longitude, point.col);
        let statistics = slice_statistics(view);
        format!(
            "hover  {latitude}  {longitude}  value {:>14}{}  |  {statistics}  (click to pin)",
            value, selected,
        )
    } else if let Some((row, col)) = view.selected_point {
        let (row_dimension, column_dimension) = plane_dimension_names(view, selected_metadata);
        let latitude = point_axis_label(
            "lat",
            row_dimension,
            view.selected_coordinates.latitude,
            row,
        );
        let longitude = point_axis_label(
            "lon",
            column_dimension,
            view.selected_coordinates.longitude,
            col,
        );
        let value = view
            .slice
            .as_ref()
            .and_then(|slice| slice.value_at_source(row, col))
            .map_or_else(|| "masked".to_string(), format_point_value);
        let statistics = slice_statistics(view);
        format!(
            "point  {latitude}  {longitude}  value {value:>14}  |  {statistics}  (Enter for plots, m adds points)"
        )
    } else {
        view.status.clone()
    };
    let status =
        if let Some((finished, total)) = view.collection_progress.filter(|(_, total)| *total > 1) {
            format!("{status}  |  sources {finished}/{total}")
        } else {
            status
        };
    let status = if let Some(diagnostic) = view.collection_diagnostics.first() {
        format!("{status}  |  collection: {diagnostic}")
    } else {
        status
    };
    status::render(frame, areas.status, &status);
    if let Some(renderer) = chart_graphics.as_deref_mut()
        && (view.variable_search_active
            || !matches!(
                view.overlay,
                Some(crate::app::Overlay::Plot | crate::app::Overlay::TimeSeries)
            ))
    {
        renderer.retire_overlay();
    }
    popup::render(
        frame,
        area,
        view,
        metadata,
        plottable,
        variable_query,
        chart_graphics,
    );
    if view.help_visible {
        help::render(frame, area);
    }
}

fn selected_level_label(
    metadata: &DatasetMetadata,
    variable: Option<&crate::data::Variable>,
    view: &ViewModel,
) -> Option<String> {
    if let Some(level_label) = view.level_label.as_ref() {
        return Some(level_label.clone());
    }
    let variable = variable?;
    let dimension = variable.dimensions.iter().find_map(|name| {
        metadata
            .dimensions
            .iter()
            .find(|dimension| dimension.name == *name)
            .filter(|dimension| dimension.role == AxisRole::Depth)
    })?;
    (view.depth_length > 1).then(|| {
        format!(
            "{} index {} of {}",
            dimension.name,
            view.depth_index,
            view.depth_length.saturating_sub(1)
        )
    })
}

fn plane_dimension_names<'a>(
    view: &'a ViewModel,
    variable: Option<&'a crate::data::Variable>,
) -> (Option<&'a str>, Option<&'a str>) {
    let dimensions = variable.map(|variable| variable.dimensions.as_slice());
    let row = view.y_axis.as_deref().or_else(|| {
        dimensions.and_then(|dimensions| {
            dimensions
                .get(dimensions.len().checked_sub(2)?)
                .map(String::as_str)
        })
    });
    let column = view
        .x_axis
        .as_deref()
        .or_else(|| dimensions.and_then(|dimensions| dimensions.last().map(String::as_str)));
    (row, column)
}

fn point_axis_label(
    coordinate_name: &str,
    dimension_name: Option<&str>,
    coordinate: Option<f64>,
    index: usize,
) -> String {
    coordinate.map_or_else(
        || {
            format!(
                "{} index {index}",
                dimension_name.unwrap_or(coordinate_name)
            )
        },
        |value| format!("{coordinate_name} {value:.5}"),
    )
}

fn slice_statistics(view: &ViewModel) -> String {
    view.slice
        .as_ref()
        .and_then(|slice| slice.statistics)
        .map_or_else(
            || "stats: no finite values".to_string(),
            |stats| {
                format!(
                    "stats min {} max {} mean {} n {}",
                    format_point_value(stats.min),
                    format_point_value(stats.max),
                    format_point_value(stats.mean),
                    stats.finite_count
                )
            },
        )
}

fn spatial_axes_selected(view: &ViewModel, metadata: &DatasetMetadata) -> bool {
    let (Some(x), Some(y)) = (view.x_axis.as_deref(), view.y_axis.as_deref()) else {
        // The default plane is the latitude/longitude plane.
        return true;
    };
    let role = |name: &str| {
        metadata
            .dimensions
            .iter()
            .find(|dimension| dimension.name.eq_ignore_ascii_case(name))
            .map(|dimension| dimension.role)
    };
    matches!(
        (role(x), role(y)),
        (Some(AxisRole::Latitude), Some(AxisRole::Longitude))
            | (Some(AxisRole::Longitude), Some(AxisRole::Latitude))
    )
}

#[cfg(test)]
mod tests {
    use super::{format_point_value, point_axis_label};
    use crate::{
        app::AppState,
        data::{DatasetFormat, DatasetMetadata},
        render::protocol::GraphicsRenderer,
    };
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn point_values_switch_to_scientific_notation_when_needed() {
        assert_eq!(format_point_value(0.0), "0.000000");
        assert_eq!(format_point_value(0.125), "0.125000");
        assert_eq!(format_point_value(1.23456789e-7), "1.23456789e-7");
        assert_eq!(format_point_value(1.23456789e8), "1.23456789e8");
    }

    #[test]
    fn index_only_axis_labels_use_the_source_dimension_name() {
        assert_eq!(
            point_axis_label("lat", Some("SAT_Tile_Height"), None, 17),
            "SAT_Tile_Height index 17"
        );
        assert_eq!(
            point_axis_label("lon", Some("SAT_Tile_Width"), None, 23),
            "SAT_Tile_Width index 23"
        );
    }

    #[test]
    fn geographic_axis_labels_use_coordinate_values_when_available() {
        assert_eq!(
            point_axis_label("lat", Some("lat"), Some(12.5), 3),
            "lat 12.50000"
        );
    }

    #[test]
    fn closing_chart_overlay_retires_pending_secondary_image() {
        let mut state = AppState::default();
        state.view.overlay = None;
        let metadata = DatasetMetadata {
            path: "test.nc".into(),
            format: DatasetFormat::NetCdf4,
            dimensions: Vec::new(),
            variables: Vec::new(),
        };
        let mut chart_graphics = GraphicsRenderer::probe();
        chart_graphics.stage_test_overlay_state();
        assert!(chart_graphics.has_pending_image());
        assert!(chart_graphics.working_set_bytes() > 0);
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal
            .draw(|frame| {
                super::render_with_search_and_image(
                    frame,
                    frame.area(),
                    &state.view,
                    "fixture",
                    &metadata,
                    &[],
                    "",
                    false,
                    None,
                    Some(&mut chart_graphics),
                );
            })
            .unwrap();
        assert!(!chart_graphics.has_pending_image());
        assert_eq!(chart_graphics.working_set_bytes(), 0);
    }
}
