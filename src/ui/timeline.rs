use ratatui::{Frame, layout::Rect, widgets::Gauge};

use super::theme;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    index: usize,
    length: usize,
    time_label: Option<&str>,
    playing: bool,
    speed: f32,
) {
    let ratio = if length <= 1 {
        0.0
    } else {
        (index as f64 / (length - 1) as f64).clamp(0.0, 1.0)
    };
    let button = if playing {
        "[ ⏸ Pause ]"
    } else {
        "[ ▶ Play ]"
    };
    let title = format!("{}  Time   {}  [−] [＋]", theme::ICON_TIME, button);
    let label = format!(
        "{}/{}  {}  ×{speed:.2}",
        index,
        length.saturating_sub(1),
        time_label.unwrap_or("coordinate index"),
    );
    frame.render_widget(
        Gauge::default()
            .block(theme::panel(&title, theme::BLUE))
            .label(label)
            .gauge_style(
                ratatui::style::Style::default()
                    .fg(theme::TEAL)
                    .bg(theme::SURFACE_ALT),
            )
            .ratio(ratio),
        area,
    );
}
