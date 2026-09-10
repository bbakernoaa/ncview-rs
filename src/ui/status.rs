use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::theme;

pub fn render(frame: &mut Frame, area: Rect, status: &str) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {} ", theme::ICON_CURSOR),
                theme::title_style(theme::TEAL),
            ),
            Span::styled(status, Style::default().fg(theme::TEXT)),
        ]))
        .style(Style::default().bg(theme::MANTLE)),
        area,
    );
}
