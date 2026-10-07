use super::chart;
use crate::app::{
    AxisField, COMMAND_PALETTE, LimitField, Overlay, PlotAxisField, PlotKind, PlotXAxis, PlotYAxis,
    ViewModel, palette_catalog_matches, palette_matches,
};
use crate::data::{DatasetMetadata, Variable};
use crate::render::colors::Palette;
use crate::render::protocol::GraphicsRenderer;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph, Wrap},
};

use super::{sidebar, theme};

fn popup_panel<'a>(title: &'a str, accent: ratatui::style::Color) -> Block<'a> {
    theme::panel(title, accent).title_top(theme::close_button())
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    view: &ViewModel,
    metadata: &DatasetMetadata,
    plottable: &[Variable],
    variable_query: &str,
    chart_graphics: Option<&mut GraphicsRenderer>,
) {
    if view.variable_search_active {
        render_variable_browser(frame, area, view, plottable, variable_query);
        return;
    }
    let Some(overlay) = view.overlay else { return };
    let title = match overlay {
        Overlay::Limits => "Limits",
        Overlay::Filter => "Data filter",
        Overlay::Axis => "Axes",
        Overlay::TimeSeries => "Time series",
        Overlay::Plot => "Plot",
        Overlay::CommandPalette => "Command Palette",
        Overlay::PalettePicker => "Colormap",
        Overlay::ViewBounds => "Set view bounds",
    };
    let message = match overlay {
        Overlay::Limits => "Type to replace the selected value; Tab switches fields",
        Overlay::Filter => "Values outside the range are masked",
        Overlay::Axis => "Choose distinct X and Y dimensions",
        Overlay::TimeSeries => "Values across the time dimension",
        Overlay::Plot => "Choose a plot and its axes",
        Overlay::CommandPalette => "Type to filter commands; Enter runs the selected action",
        Overlay::PalettePicker => "Choose a colormap; Enter applies it",
        Overlay::ViewBounds => "Enter x/y coordinate bounds for the displayed region",
    };
    let width = if matches!(
        overlay,
        Overlay::CommandPalette | Overlay::Plot | Overlay::PalettePicker
    ) {
        area.width.saturating_mul(3) / 4
    } else {
        area.width.saturating_mul(3) / 5
    };
    let height = if matches!(
        overlay,
        Overlay::CommandPalette | Overlay::Plot | Overlay::PalettePicker
    ) {
        area.height.saturating_mul(3) / 5
    } else {
        area.height.saturating_mul(2) / 5
    };
    let popup = overlay_popup_rect(area, overlay, width, height);
    let shadow = popup_shadow_rect(area, popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        ratatui::widgets::Block::default().style(Style::default().bg(theme::SHADOW)),
        shadow,
    );
    if matches!(overlay, Overlay::CommandPalette) {
        let matches = palette_matches(&view.palette_query);
        let mut lines = vec![format!("Search: {}", view.palette_query)];
        lines.push(String::new());
        if matches.is_empty() {
            lines.push("No matching commands".into());
        } else {
            for (position, index) in matches.iter().enumerate() {
                let entry = &COMMAND_PALETTE[*index];
                let marker = if position == view.palette_index {
                    ">"
                } else {
                    " "
                };
                lines.push(format!("{marker} {:<34} {}", entry.label, entry.shortcut));
            }
        }
        lines.push(String::new());
        lines.push("↑↓ select   Enter run   Esc close".into());
        frame.render_widget(
            Paragraph::new(lines.join("\n")).block(popup_panel(title, theme::MAUVE)),
            popup,
        );
    } else if matches!(overlay, Overlay::PalettePicker) {
        render_palette_picker(frame, popup, title, view);
    } else if matches!(overlay, Overlay::Limits | Overlay::Filter) {
        let draft = view.limit_draft.as_ref();
        let min = draft.map_or("".to_string(), |draft| draft.min.clone());
        let max = draft.map_or("".to_string(), |draft| draft.max.clone());
        let active = draft.map(|draft| draft.active);
        let text = format!(
            "{}: [ {}{} ]\n{}: [ {}{} ]\n\nType replaces value   Backspace edits\nTab: switch field   Enter: apply   Esc: cancel",
            if matches!(overlay, Overlay::Filter) {
                "Keep from"
            } else {
                "Minimum"
            },
            if active == Some(LimitField::Min) {
                "> "
            } else {
                "  "
            },
            min,
            if matches!(overlay, Overlay::Filter) {
                "Keep through"
            } else {
                "Maximum"
            },
            if active == Some(LimitField::Max) {
                "> "
            } else {
                "  "
            },
            max
        );
        frame.render_widget(
            Paragraph::new(text).block(popup_panel(title, theme::PEACH)),
            popup,
        );
    } else if overlay == Overlay::ViewBounds {
        let draft = view.view_bounds_draft.as_ref();
        let labels = ["Min X", "Max X", "Min Y", "Max Y"];
        let mut lines = labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let active = draft.is_some_and(|draft| draft.active == index);
                let value = draft.map_or("", |draft| draft.fields[index].as_str());
                format!("{label}: [{}{}]", if active { "> " } else { "  " }, value)
            })
            .collect::<Vec<_>>();
        lines.push("Type replaces   Backspace edits".into());
        lines.push("Tab next field   Enter apply   Esc cancel".into());
        if let Some(error) = draft.and_then(|draft| draft.error.as_deref()) {
            lines.push(error.to_string());
        }
        frame.render_widget(
            Paragraph::new(lines.join("\n"))
                .wrap(Wrap { trim: false })
                .block(popup_panel(title, theme::MAUVE)),
            popup,
        );
    } else if matches!(overlay, Overlay::Axis) {
        let draft = view.axis_draft.as_ref();
        let x = draft.map_or("".to_string(), |draft| draft.x.clone());
        let y = draft.map_or("".to_string(), |draft| draft.y.clone());
        let active = draft.map(|draft| draft.active);
        let options = if view.axis_options.is_empty() {
            metadata
                .variables
                .iter()
                .find(|variable| view.selected_variable.as_deref() == Some(variable.name.as_str()))
                .map(|variable| variable.dimensions.join(", "))
                .unwrap_or_else(|| "no dimensions discovered".into())
        } else {
            view.axis_options.join(", ")
        };
        let text = format!(
            "X axis: [ {}{} ]\nY axis: [ {}{} ]\n\nAvailable: {options}\n↑↓ choose axis   Tab: switch field\nType to replace value   Enter: apply   Esc: cancel",
            if active == Some(AxisField::X) {
                "> "
            } else {
                "  "
            },
            x,
            if active == Some(AxisField::Y) {
                "> "
            } else {
                "  "
            },
            y
        );
        frame.render_widget(
            Paragraph::new(text).block(popup_panel(title, theme::BLUE)),
            popup,
        );
    } else if matches!(overlay, Overlay::Plot) {
        render_plot(frame, popup, view, chart_graphics);
    } else if matches!(overlay, Overlay::TimeSeries) {
        let series = plot_series_for_view(view);
        chart::render_plot(
            frame,
            popup,
            PlotKind::TimeSeries,
            PlotXAxis::ValidTime,
            PlotYAxis::Value,
            None,
            &series,
            &[],
            chart_graphics,
        );
    } else {
        frame.render_widget(
            Paragraph::new(message).block(theme::panel(title, theme::MAUVE)),
            popup,
        );
    }
}

fn overlay_popup_rect(area: Rect, overlay: Overlay, width: u16, height: u16) -> Rect {
    if overlay == Overlay::PalettePicker && (area.width < 54 || area.height < 12) {
        return area;
    }
    if overlay == Overlay::ViewBounds && (area.width < 52 || area.height < 14) {
        return area;
    }
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

fn popup_shadow_rect(area: Rect, popup: Rect) -> Rect {
    let x = popup.x.saturating_add(1).max(area.x);
    let y = popup.y.saturating_add(1).max(area.y);
    Rect::new(
        x,
        y,
        popup
            .width
            .min(area.x.saturating_add(area.width).saturating_sub(x)),
        popup
            .height
            .min(area.y.saturating_add(area.height).saturating_sub(y)),
    )
}

pub fn picker_popup_rect(area: Rect) -> Rect {
    let width = area.width.saturating_mul(3) / 4;
    let height = area.height.saturating_mul(3) / 5;
    overlay_popup_rect(area, Overlay::PalettePicker, width, height)
}

fn render_palette_picker(frame: &mut Frame, popup: Rect, title: &str, view: &ViewModel) {
    let block = popup_panel(title, theme::MAUVE);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let focused = view
        .palette_picker
        .as_ref()
        .map(|picker| &picker.focused_palette);
    let query = view
        .palette_picker
        .as_ref()
        .map_or("", |picker| picker.query.as_str());
    let matches = palette_catalog_matches(&view.palette_catalog, query);
    let focus_index = focused
        .and_then(|palette| {
            matches
                .iter()
                .position(|&index| palette_identity_eq(&view.palette_catalog[index], palette))
        })
        .unwrap_or(0);
    let compact = inner.width < 54 || inner.height < 7;
    let (list_area, preview_area, instruction_area, search_area) = if compact {
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(2),
            ])
            .split(inner);
        (sections[1], None, sections[2], sections[0])
    } else {
        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(2),
                Constraint::Length(1),
            ])
            .split(inner);
        (sections[1], Some(sections[2]), sections[3], sections[0])
    };
    let first = visible_palette_start(matches.len(), focus_index, usize::from(list_area.height));
    let end = first
        .saturating_add(usize::from(list_area.height))
        .min(matches.len());
    let list = matches
        .iter()
        .skip(first)
        .take(end.saturating_sub(first))
        .map(|&index| {
            let palette = &view.palette_catalog[index];
            let is_focused = focused.is_some_and(|focused| palette_identity_eq(palette, focused));
            let is_applied = palette_identity_eq(palette, &view.palette);
            let marker = if is_focused { ">" } else { " " };
            let state = match (is_applied, is_focused) {
                (true, true) => "applied, focused",
                (true, false) => "applied",
                (false, true) => "focused",
                (false, false) => "",
            };
            let reversed = is_focused && focused.is_some_and(|focused| focused.is_reversed());
            format!(
                "{marker} {:<24} {state}{}",
                palette.name(),
                if reversed { ", reversed" } else { "" }
            )
        })
        .collect::<Vec<_>>();
    let list = if list.is_empty() {
        vec![if query.is_empty() {
            "No colormaps available".into()
        } else {
            "No colormaps match this search".into()
        }]
    } else {
        list
    };
    frame.render_widget(
        Paragraph::new(format!(
            "Search: {query}  {} / {} palettes",
            matches.len(),
            view.palette_catalog.len()
        )),
        search_area,
    );
    frame.render_widget(
        Paragraph::new(list.join("\n")).wrap(Wrap { trim: true }),
        list_area,
    );
    if let (Some(focused), Some(preview_area)) = (focused, preview_area) {
        super::colorbar::render_preview(frame, preview_area, focused, view.limits, view.scale_mode);
    }
    let instructions = if compact {
        "↑↓ browse  Enter apply\nEsc cancel  v reverse"
    } else {
        "↑↓ choose   v reverse   Enter apply   Esc cancel"
    };
    frame.render_widget(Paragraph::new(instructions), instruction_area);
}

fn visible_palette_start(total: usize, focus: usize, visible: usize) -> usize {
    if total <= visible || visible == 0 {
        0
    } else {
        focus.saturating_sub(visible / 2).min(total - visible)
    }
}

fn palette_identity_eq(left: &Palette, right: &Palette) -> bool {
    left.name() == right.name()
}

fn render_plot(
    frame: &mut Frame,
    popup: Rect,
    view: &ViewModel,
    chart_graphics: Option<&mut GraphicsRenderer>,
) {
    let block = popup_panel("Plot", theme::MAUVE);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(5)])
        .split(inner);
    let draft = view.plot_draft;
    let x_label = match draft.x_axis {
        PlotXAxis::ValidTime => "valid time",
        PlotXAxis::SampleIndex => "sample index",
        PlotXAxis::Longitude => "longitude",
        PlotXAxis::Latitude => "latitude",
        PlotXAxis::Dimension(_index) if draft.kind == PlotKind::VerticalProfile => "value",
        PlotXAxis::Dimension(index) => view
            .axis_options
            .get(index)
            .map(String::as_str)
            .unwrap_or("dimension"),
        PlotXAxis::Value => "value",
    };
    let y_label = match draft.y_axis {
        PlotYAxis::Value if draft.kind == PlotKind::VerticalProfile => view
            .axis_options
            .get(match draft.x_axis {
                PlotXAxis::Dimension(index) => index,
                _ => usize::MAX,
            })
            .map(String::as_str)
            .unwrap_or("level"),
        PlotYAxis::Value => "value",
        PlotYAxis::Frequency => "frequency",
        PlotYAxis::Density => "density (%)",
    };
    let kind_label = match draft.kind {
        PlotKind::TimeSeries => "time series",
        PlotKind::Scatter => "scatter",
        PlotKind::Histogram => "histogram",
        PlotKind::Cdf => "CDF",
        PlotKind::VerticalProfile => "vertical profile",
    };
    let x_marker = if draft.active == PlotAxisField::X {
        ">"
    } else {
        " "
    };
    let y_marker = if draft.active == PlotAxisField::Y {
        ">"
    } else {
        " "
    };
    let selected_count = view
        .selected_points
        .len()
        .max(usize::from(view.selected_point.is_some()));
    let target_label = if selected_count == 0 {
        "view domain summary".to_string()
    } else if selected_count == 1
        && let (Some(lat), Some(lon)) = (
            view.selected_coordinates.latitude,
            view.selected_coordinates.longitude,
        )
    {
        format!("point at lat={lat:.2}°, lon={lon:.2}°")
    } else {
        format!("{selected_count} selected point(s)")
    };
    let controls = format!(
        "type: [t] time series  [d] scatter  [h] histogram  [k] CDF  [u] profile   (current: {kind_label})\n\
target: {target_label}\n\
{x_marker} X axis: {x_label}\n\
{y_marker} Y axis: {y_label}\n\
Tab switches axes  •  ↑↓/←→ changes the selected axis  •  m adds/removes points",
    );
    frame.render_widget(
        Paragraph::new(controls).block(theme::panel("Plot controls", theme::BLUE)),
        rows[0],
    );
    let series = plot_series_for_view(view);
    let histogram_values =
        if !view.selected_points.is_empty() && series.iter().any(|item| item.data.len() > 1) {
            series
                .iter()
                .flat_map(|item| item.data.iter().map(|(_, value)| *value))
                .collect::<Vec<_>>()
        } else {
            view.slice
                .as_ref()
                .map(|slice| slice.values.iter().copied().collect::<Vec<_>>())
                .unwrap_or_default()
        };
    chart::render_plot(
        frame,
        rows[1],
        draft.kind,
        draft.x_axis,
        draft.y_axis,
        match draft.x_axis {
            PlotXAxis::Dimension(index) => view.axis_options.get(index).map(String::as_str),
            _ => None,
        },
        &series,
        &histogram_values,
        chart_graphics,
    );
}

fn plot_series_for_view(view: &ViewModel) -> Vec<crate::app::PlotSeries> {
    if view.plot_series.is_empty() {
        vec![crate::app::PlotSeries {
            point: view.selected_point.unwrap_or((0, 0)),
            label: "selected point".into(),
            data: view.time_series.clone(),
            labels: view.time_series_labels.clone(),
        }]
    } else {
        view.plot_series.clone()
    }
}

fn render_variable_browser(
    frame: &mut Frame,
    area: Rect,
    view: &ViewModel,
    plottable: &[Variable],
    variable_query: &str,
) {
    let width = area.width.saturating_mul(4).saturating_div(5).max(1);
    let height = area.height.saturating_mul(4).saturating_div(5).max(1);
    let popup = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    };
    let shadow = Rect {
        x: popup.x.saturating_add(1),
        y: popup.y.saturating_add(1),
        width: popup.width,
        height: popup.height,
    };
    frame.render_widget(Clear, popup);
    frame.render_widget(
        ratatui::widgets::Block::default().style(Style::default().bg(theme::SHADOW)),
        shadow,
    );

    let block = popup_panel("Variables", theme::MAUVE);
    let inner = block.inner(popup);
    // The browser lists exactly the selectable set (the cross-file plottable
    // union), so a click or Enter always resolves to the row that is shown.
    let filtered = sidebar::filter_variables(plottable, variable_query);
    let selected = view
        .variable_browser_index
        .min(filtered.len().saturating_sub(1));
    let name_width = usize::from(inner.width.saturating_sub(3)).max(1);
    let entries = filtered
        .iter()
        .enumerate()
        .map(|(index, variable)| {
            wrap_name(&variable.name, name_width)
                .into_iter()
                .enumerate()
                .map(|(line_index, chunk)| {
                    let marker = if line_index == 0 {
                        if index == selected { "▶ " } else { "  " }
                    } else {
                        "  "
                    };
                    let style = if index == selected {
                        theme::title_style(theme::TEXT)
                    } else {
                        theme::muted_style()
                    };
                    Line::from(vec![
                        Span::styled(marker, style),
                        Span::styled(chunk, style),
                    ])
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let list_height = usize::from(inner.height).saturating_sub(4);
    let start = visible_window_start(&entries, selected, list_height);
    let mut lines = vec![Line::from(vec![
        Span::styled("Search: ", theme::title_style(theme::TEAL)),
        Span::styled(
            if variable_query.is_empty() {
                "<all>"
            } else {
                variable_query
            },
            theme::muted_style(),
        ),
    ])];
    lines.push(Line::from(""));
    if filtered.is_empty() {
        lines.push(Line::from(Span::styled(
            "No matching plottable variables",
            theme::muted_style(),
        )));
    } else {
        let mut used = 0usize;
        for entry in entries.iter().skip(start) {
            if used >= list_height {
                break;
            }
            let room = list_height - used;
            lines.extend(entry.iter().take(room).cloned());
            used += entry.len().min(room);
        }
    }
    lines.push(Line::from(Span::styled(
        format!(
            "{} field(s)  ↑↓ select  Enter open  Esc close",
            filtered.len()
        ),
        theme::muted_style(),
    )));
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block),
        popup,
    );
}

fn wrap_name(value: &str, width: usize) -> Vec<String> {
    let characters = value.chars().collect::<Vec<_>>();
    if characters.is_empty() {
        return vec![String::new()];
    }
    characters
        .chunks(width.max(1))
        .map(|chunk| chunk.iter().collect())
        .collect()
}

fn visible_window_start(entries: &[Vec<Line<'_>>], selected: usize, height: usize) -> usize {
    if entries.is_empty() || height == 0 {
        return 0;
    }
    let selected = selected.min(entries.len() - 1);
    let mut start = selected;
    let mut used = entries[selected].len();
    while start > 0 && used + entries[start - 1].len() <= height {
        start -= 1;
        used += entries[start].len();
    }
    start
}

#[cfg(test)]
mod palette_picker_tests {
    use super::render;
    use crate::{
        app::{AppState, Overlay, PalettePickerState},
        data::{DatasetFormat, DatasetMetadata},
        render::colors::{Palette, ScaleMode},
    };
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn picker_popup_shows_applied_and_focused_palette_preview_and_controls() {
        let mut state = AppState::default();
        state.view.palette = Palette::Viridis;
        state.view.palette_catalog = vec![Palette::Viridis, Palette::Plasma];
        state.view.palette_picker = Some(PalettePickerState {
            focused_palette: Palette::Plasma,
            query: String::new(),
        });
        state.view.overlay = Some(Overlay::PalettePicker);
        state.view.limits = Some((0.0, 10.0));
        state.view.scale_mode = ScaleMode::Linear;
        let metadata = DatasetMetadata {
            path: "test.nc".into(),
            format: DatasetFormat::NetCdf4,
            dimensions: Vec::new(),
            variables: Vec::new(),
        };
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal
            .draw(|frame| render(frame, frame.area(), &state.view, &metadata, &[], "", None))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let text = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<String>();
        assert!(text.contains("Viridis"));
        assert!(text.contains("Plasma"));
        assert!(text.contains("applied"));
        assert!(text.contains("focused"));
        assert!(text.contains("Enter"));
        assert!(text.contains("Esc"));
        assert!(text.contains("v reverse"));
        assert!(text.contains("linear"));
        assert!(text.contains("10"));
        let first_sample = Palette::Plasma.sample(0.0);
        assert!(buffer.content().iter().any(|cell| {
            cell.style().bg
                == Some(ratatui::style::Color::Rgb(
                    first_sample[0],
                    first_sample[1],
                    first_sample[2],
                ))
        }));
    }

    #[test]
    fn long_picker_catalog_scrolls_focus_into_view_and_small_popup_keeps_text_controls() {
        use crate::render::colors::ScientificColorMap;
        use std::sync::Arc;

        let choices = (0..24)
            .map(|index| {
                Palette::Custom(Arc::new(ScientificColorMap {
                    name: format!("Choice{index:02}"),
                    colors: vec![[12, 34, 56], [200, 201, 202]],
                }))
            })
            .collect::<Vec<_>>();
        let mut state = AppState::default();
        state.view.palette = choices[0].clone();
        state.view.palette_catalog = choices.clone();
        state.view.palette_picker = Some(PalettePickerState {
            focused_palette: choices[20].clone(),
            query: String::new(),
        });
        state.view.overlay = Some(Overlay::PalettePicker);
        let metadata = DatasetMetadata {
            path: "test.nc".into(),
            format: DatasetFormat::NetCdf4,
            dimensions: Vec::new(),
            variables: Vec::new(),
        };

        let render_text = |width, height| {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| render(frame, frame.area(), &state.view, &metadata, &[], "", None))
                .unwrap();
            let buffer = terminal.backend().buffer();
            let text = (0..buffer.area.height)
                .map(|y| {
                    (0..buffer.area.width)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<String>();
            let has_preview_color = buffer
                .content()
                .iter()
                .any(|cell| cell.style().bg == Some(ratatui::style::Color::Rgb(12, 34, 56)));
            (text, has_preview_color)
        };

        let (normal, normal_preview) = render_text(80, 16);
        assert!(normal.contains("Choice20"));
        assert!(normal.contains("focused"));
        assert!(normal_preview);
        let constrained_area = ratatui::layout::Rect::new(0, 0, 48, 9);
        let popup = super::picker_popup_rect(constrained_area);
        assert!(popup.width <= constrained_area.width);
        assert!(popup.height <= constrained_area.height);
        assert!(popup.x + popup.width <= constrained_area.width);
        assert!(popup.y + popup.height <= constrained_area.height);
        let (constrained, constrained_preview) = render_text(48, 9);
        assert!(constrained.contains("Choice20"));
        assert!(constrained.contains("Enter"));
        assert!(constrained.contains("Esc"));
        assert!(constrained.contains("v reverse"));
        assert!(!constrained_preview);
    }

    #[test]
    fn picker_search_shows_filtered_count_and_no_match_feedback() {
        let mut state = AppState::default();
        state.view.palette_catalog = vec![Palette::Viridis, Palette::Plasma, Palette::Magma];
        state.view.palette = Palette::Viridis;
        state.reduce(crate::app::Command::OpenPalettePicker);
        state.reduce(crate::app::Command::InputChar('p'));
        state.reduce(crate::app::Command::InputChar('l'));
        let metadata = DatasetMetadata {
            path: "test.nc".into(),
            format: DatasetFormat::NetCdf4,
            dimensions: Vec::new(),
            variables: Vec::new(),
        };
        let mut terminal = Terminal::new(TestBackend::new(90, 24)).unwrap();
        terminal
            .draw(|frame| render(frame, frame.area(), &state.view, &metadata, &[], "", None))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("1 / 3 palettes"));
        assert!(text.contains("Plasma"));
        assert!(!text.contains("Magma"));

        state.reduce(crate::app::Command::InputChar('z'));
        terminal
            .draw(|frame| render(frame, frame.area(), &state.view, &metadata, &[], "", None))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("No colormaps match this search"));
    }
}

#[cfg(test)]
mod view_bounds_popup_tests {
    use super::render;
    use crate::{
        app::{AppState, Overlay, ViewBoundsDraft},
        data::{DatasetFormat, DatasetMetadata},
    };
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    fn metadata() -> DatasetMetadata {
        DatasetMetadata {
            path: "test.nc".into(),
            format: DatasetFormat::NetCdf4,
            dimensions: Vec::new(),
            variables: Vec::new(),
        }
    }

    fn rendered_text(width: u16, height: u16, error: Option<&str>) -> String {
        let mut state = AppState::default();
        state.view.overlay = Some(Overlay::ViewBounds);
        state.view.view_bounds_draft = Some(ViewBoundsDraft {
            fields: ["-90".into(), "90".into(), "-30".into(), "30".into()],
            active: 2,
            replace_active: true,
            error: error.map(str::to_string),
        });
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| render(frame, frame.area(), &state.view, &metadata(), &[], "", None))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn view_bounds_popup_shows_four_fields_and_actionable_errors() {
        let text = rendered_text(
            80,
            24,
            Some("latitude bounds must be between -90 and 90 degrees"),
        );
        for field in ["Min X", "Max X", "Min Y", "Max Y"] {
            assert!(text.contains(field), "missing {field}: {text}");
        }
        assert!(text.contains("-90"));
        assert!(text.contains("90"));
        assert!(text.contains("latitude bounds"));
        assert!(text.contains("Tab"));
        assert!(text.contains("Enter"));
        assert!(text.contains("Esc"));
    }

    #[test]
    fn view_bounds_popup_uses_available_area_on_small_terminals() {
        let area = Rect::new(0, 0, 40, 10);
        let popup = super::overlay_popup_rect(area, Overlay::ViewBounds, 24, 4);
        assert_eq!(popup, area);
        let text = rendered_text(40, 10, None);
        assert!(text.contains("Min X"));
        assert!(text.contains("Max Y"));
    }
}

#[cfg(test)]
mod dismissal_tests {
    use super::{popup_shadow_rect, render};
    use crate::{
        app::{AppState, Overlay},
        data::{DatasetFormat, DatasetMetadata},
    };
    use ratatui::{Terminal, backend::TestBackend, layout::Rect, widgets::Paragraph};

    #[test]
    fn dismissing_each_popup_restores_the_underlying_frame() {
        let overlays = [
            Overlay::Limits,
            Overlay::Filter,
            Overlay::Axis,
            Overlay::TimeSeries,
            Overlay::Plot,
            Overlay::CommandPalette,
            Overlay::PalettePicker,
            Overlay::ViewBounds,
        ];
        let metadata = DatasetMetadata {
            path: "test.nc".into(),
            format: DatasetFormat::NetCdf4,
            dimensions: Vec::new(),
            variables: Vec::new(),
        };
        for overlay in overlays {
            let mut state = AppState::default();
            let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal builds");
            for _ in 0..100 {
                state.view.overlay = Some(overlay);
                terminal
                    .draw(|frame| {
                        frame.render_widget(Paragraph::new("MAP BACKDROP"), frame.area());
                        render(frame, frame.area(), &state.view, &metadata, &[], "", None);
                    })
                    .expect("popup frame renders");

                state.view.overlay = None;
                terminal
                    .draw(|frame| {
                        frame.render_widget(Paragraph::new("MAP BACKDROP"), frame.area());
                        render(frame, frame.area(), &state.view, &metadata, &[], "", None);
                    })
                    .expect("dismissed frame renders");
                let buffer = terminal.backend().buffer();
                let text = (0..buffer.area.height)
                    .map(|y| {
                        (0..buffer.area.width)
                            .map(|x| buffer[(x, y)].symbol())
                            .collect::<String>()
                    })
                    .collect::<String>();
                assert!(text.contains("MAP BACKDROP"));
                assert!(!text.contains("Choose a colormap"));
                assert!(!text.contains("Limits"));
                assert!(
                    buffer
                        .content()
                        .iter()
                        .all(|cell| { cell.style().bg != Some(crate::ui::theme::SHADOW) })
                );
            }
        }
    }

    #[test]
    fn popup_shadow_is_clipped_to_terminal_area() {
        let area = Rect::new(4, 3, 12, 7);
        let shadow = popup_shadow_rect(area, area);
        assert!(shadow.x >= area.x && shadow.y >= area.y);
        assert!(shadow.right() <= area.right());
        assert!(shadow.bottom() <= area.bottom());
    }
}
