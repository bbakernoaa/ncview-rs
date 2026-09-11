use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    widgets::{Block, Clear, Paragraph},
};

use super::theme;

pub fn render(frame: &mut Frame, area: Rect) {
    let width = area.width.saturating_mul(3) / 4;
    let height = area.height.saturating_mul(3) / 5;
    let popup = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
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
        Block::default().style(Style::default().bg(theme::SHADOW)),
        shadow,
    );
    let text = "Keyboard\n\
  q / Esc       quit (Esc closes a dialog first)\n\
  ↑ / ↓         previous / next variable\n\
  ← / →         previous / next time slice\n\
  < / >         previous / next time slice\n\
  Space         play / pause timeline\n\
  - / +         decrease / increase playback speed\n\
  { / }         previous / next file\n\
  [ / ]         previous / next depth slice\n\
  c             cycle colormap\n\
  v             reverse colormap\n\
  a             automatic limits\n\
  l             edit min/max limits\n\
  f             mask data outside a range\n\
  s             toggle linear/log color scale\n\
  z             toggle current/global color scale\n\
  r             reset zoom\n\
  g             logical/projected grid\n\
  b             toggle filled land/ocean map backdrop\n\
  Enter         open pinned point time series\n\
Mouse\n\
  move over map  hover row/col/value readout\n\
  click map      pin point (◆), then Enter for time series\n\
  click a variable, palette, limits, grid, or timeline\n\
  right-click or ? closes this help\n\
Command palette\n\
  Ctrl-P or :   search actions, then Enter to run\n\
Limits dialog: type numbers, Tab switches fields, Enter applies, Esc cancels";
    let text = text
        .replace(
            "  c             cycle colormap",
            "  Shift+arrows   pan the zoomed map\n  c             cycle colormap\n  i             cycle interpolation (set NCVIEW_SCIENTIFIC_RENDERING=0 to unlock)\n  e             export current slice (PNG + SVG + JSON)",
        )
        .replace(
            "  r             reset zoom",
            "  r             reset zoom\n  drag map      zoom to a rectangle; drag zoomed map to pan\n  Shift+drag    zoom again while already zoomed",
        )
        .replace(
            "  click a variable, palette, limits, grid, or timeline",
            "  click a sidebar button, variable, timeline play/pause, or speed control",
        );
    frame.render_widget(
        Paragraph::new(text).block(theme::panel("󰋖  Help", theme::TEAL)),
        popup,
    );
}
