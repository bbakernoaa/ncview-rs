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
) {
    render_with_search(frame, area, view, filename, metadata, "", false);
}

pub fn render_with_search(
    frame: &mut Frame,
    area: Rect,
    view: &ViewModel,
    filename: &str,
    metadata: &DatasetMetadata,
    variable_query: &str,
    variable_search_active: bool,
) {
    render_with_search_and_image(
        frame,
        area,
        view,
        filename,
        metadata,
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
    variable_query: &str,
    variable_search_active: bool,
    graphics: Option<&mut GraphicsRenderer>,
    chart_graphics: Option<&mut GraphicsRenderer>,
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
    let areas = layout::dashboard(area);
    let header = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme::SURFACE_ALT))
        .style(Style::default().bg(theme::MANTLE));
    frame.render_widget(header, areas.header);
    let header_line = Line::from(vec![
        Span::styled(" ncv ", theme::title_style(theme::MAUVE)),
        Span::styled("v0.1.0", Style::default().fg(theme::SUBTEXT)),
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
        "Keys: arrows navigate  [/] depth  click map to select  Enter: plots  m: add point  ? help"
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
        let latitude = point.latitude.map_or_else(
            || format!("lat index {}", point.row),
            |value| format!("lat {value:.5}"),
        );
        let longitude = point.longitude.map_or_else(
            || format!("lon index {}", point.col),
            |value| format!("lon {value:.5}"),
        );
        let statistics = slice_statistics(view);
        format!(
            "hover  {latitude}  {longitude}  value {:>14}{}  |  {statistics}  (click to pin)",
            value, selected,
        )
    } else if let Some((row, col)) = view.selected_point {
        let latitude = view.selected_coordinates.latitude.map_or_else(
            || format!("lat index {row}"),
            |value| format!("lat {value:.5}"),
        );
        let longitude = view.selected_coordinates.longitude.map_or_else(
            || format!("lon index {col}"),
            |value| format!("lon {value:.5}"),
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
    status::render(frame, areas.status, &status);
    popup::render(frame, area, view, metadata, variable_query, chart_graphics);
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
    use super::format_point_value;

    #[test]
    fn point_values_switch_to_scientific_notation_when_needed() {
        assert_eq!(format_point_value(0.0), "0.000000");
        assert_eq!(format_point_value(0.125), "0.125000");
        assert_eq!(format_point_value(1.23456789e-7), "1.23456789e-7");
        assert_eq!(format_point_value(1.23456789e8), "1.23456789e8");
    }
}
