use super::chart;
use crate::app::{AxisField, COMMAND_PALETTE, LimitField, Overlay, ViewModel, palette_matches};
use crate::data::DatasetMetadata;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    widgets::{Clear, Paragraph},
};

use super::theme;

pub fn render(frame: &mut Frame, area: Rect, view: &ViewModel, metadata: &DatasetMetadata) {
    let Some(overlay) = view.overlay else { return };
    let title = match overlay {
        Overlay::Limits => "Limits",
        Overlay::Filter => "Data filter",
        Overlay::Axis => "Axes",
        Overlay::TimeSeries => "Time series",
        Overlay::CommandPalette => "Command Palette",
    };
    let message = match overlay {
        Overlay::Limits => "Type to replace the selected value; Tab switches fields",
        Overlay::Filter => "Values outside the range are masked",
        Overlay::Axis => "Choose distinct X and Y dimensions",
        Overlay::TimeSeries => "Values across the time dimension",
        Overlay::CommandPalette => "Type to filter commands; Enter runs the selected action",
    };
    let width = if matches!(overlay, Overlay::CommandPalette) {
        area.width.saturating_mul(3) / 4
    } else {
        area.width.saturating_mul(3) / 5
    };
    let height = if matches!(overlay, Overlay::CommandPalette) {
        area.height.saturating_mul(3) / 5
    } else {
        area.height.saturating_mul(2) / 5
    };
    let popup = Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
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
            Paragraph::new(lines.join("\n")).block(theme::panel(title, theme::MAUVE)),
            popup,
        );
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
            Paragraph::new(text).block(theme::panel(title, theme::PEACH)),
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
            Paragraph::new(text).block(theme::panel(title, theme::BLUE)),
            popup,
        );
    } else if matches!(overlay, Overlay::TimeSeries) {
        chart::render(frame, popup, &view.time_series, &view.time_series_labels);
    } else {
        frame.render_widget(
            Paragraph::new(message).block(theme::panel(title, theme::MAUVE)),
            popup,
        );
    }
}
