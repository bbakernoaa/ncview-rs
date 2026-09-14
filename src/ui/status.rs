use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
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

pub fn render_loading(
    frame: &mut Frame,
    area: Rect,
    datasets: &[String],
    loaded: usize,
    current_dataset: Option<&str>,
    loading_status: Option<&str>,
) {
    let block = Block::default().title(" ncv ").borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(0),
        ])
        .split(inner);
    frame.render_widget(
        Paragraph::new(format!("Opening {} dataset(s)…", datasets.len())),
        sections[0],
    );
    let ratio = if datasets.is_empty() {
        1.0
    } else {
        loaded as f64 / datasets.len() as f64
    };
    frame.render_widget(
        Gauge::default()
            .block(Block::default().borders(Borders::ALL))
            .ratio(ratio.clamp(0.0, 1.0)),
        sections[1],
    );
    let status = current_dataset
        .map(|dataset| {
            loading_status.map_or_else(
                || format!("{loaded}/{}  {dataset}", datasets.len()),
                |detail| format!("{loaded}/{}  {dataset}  {detail}", datasets.len()),
            )
        })
        .unwrap_or_else(|| format!("{loaded}/{}", datasets.len()));
    frame.render_widget(Paragraph::new(status), sections[2]);
}
