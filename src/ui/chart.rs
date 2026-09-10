use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Span,
    widgets::{Axis, Chart, Dataset, GraphType},
};

use super::theme;

pub fn render(frame: &mut Frame, area: Rect, data: &[(f64, f64)], labels: &[String]) {
    if data.is_empty() {
        frame.render_widget(
            ratatui::widgets::Paragraph::new("No finite samples at this point.")
                .block(theme::panel("󰈈  Time series", theme::MAUVE)),
            area,
        );
        return;
    }
    let x_min = data.first().map_or(0.0, |point| point.0);
    let x_max = data.last().map_or(1.0, |point| point.0).max(x_min + 1.0);
    let (mut y_min, mut y_max) = data.iter().fold(
        (f64::INFINITY, f64::NEG_INFINITY),
        |(min, max), (_, value)| (min.min(*value), max.max(*value)),
    );
    if !y_min.is_finite() || !y_max.is_finite() {
        frame.render_widget(
            ratatui::widgets::Paragraph::new("No finite samples at this point.")
                .block(theme::panel("󰈈  Time series", theme::MAUVE)),
            area,
        );
        return;
    }
    if (y_max - y_min).abs() < f64::EPSILON {
        let padding = y_min.abs().max(1.0) * 0.05;
        y_min -= padding;
        y_max += padding;
    } else {
        let padding = (y_max - y_min) * 0.05;
        y_min -= padding;
        y_max += padding;
    }
    let dataset = Dataset::default()
        .name("value")
        .marker(ratatui::symbols::Marker::Dot)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(theme::TEAL))
        .data(data);
    let x_labels = if labels.len() >= 2 {
        vec![
            Span::raw(labels.first().cloned().unwrap_or_default()),
            Span::raw(labels[labels.len() / 2].clone()),
            Span::raw(labels.last().cloned().unwrap_or_default()),
        ]
    } else {
        vec![
            Span::raw(format!("{x_min:.0}")),
            Span::raw(format!("{x_max:.0}")),
        ]
    };
    let chart = Chart::new(vec![dataset])
        .block(theme::panel("󰈈  Time series", theme::MAUVE))
        .x_axis(
            Axis::default()
                .bounds([x_min, x_max])
                .labels(x_labels)
                .style(Style::default().fg(theme::SUBTEXT)),
        )
        .y_axis(
            Axis::default()
                .bounds([y_min, y_max])
                .style(Style::default().fg(theme::SUBTEXT)),
        );
    frame.render_widget(chart, area);
}
