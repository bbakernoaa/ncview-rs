use ratatui::{
    style::{Color, Modifier, Style},
    symbols::border,
    widgets::{Block, BorderType, Borders},
};

/// Catppuccin Mocha-inspired colors. They are explicit RGB values so the
/// dashboard remains coherent on true-color terminals while still degrading
/// gracefully through ratatui's color handling.
pub const BASE: Color = Color::Rgb(30, 30, 46);
pub const MANTLE: Color = Color::Rgb(24, 24, 37);
pub const SURFACE: Color = Color::Rgb(49, 50, 68);
pub const SURFACE_ALT: Color = Color::Rgb(69, 71, 90);
pub const TEXT: Color = Color::Rgb(205, 214, 244);
pub const SUBTEXT: Color = Color::Rgb(166, 173, 200);
pub const BLUE: Color = Color::Rgb(137, 180, 250);
pub const TEAL: Color = Color::Rgb(148, 226, 213);
pub const PEACH: Color = Color::Rgb(250, 179, 135);
pub const MAUVE: Color = Color::Rgb(203, 166, 247);
pub const RED: Color = Color::Rgb(243, 139, 168);
pub const SHADOW: Color = Color::Rgb(17, 17, 27);

pub const ICON_FILE: &str = "󰈔";
pub const ICON_VARIABLE: &str = "󰘦";
pub const ICON_DIMENSION: &str = "󰋁";
pub const ICON_PALETTE: &str = "󰏘";
pub const ICON_GRID: &str = "󰒋";
pub const ICON_TIME: &str = "󰥔";
pub const ICON_CURSOR: &str = "󰆿";
pub const ICON_COMMAND: &str = "󰘳";

pub fn panel<'a>(title: &'a str, accent: Color) -> Block<'a> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(SURFACE).fg(TEXT))
}

pub fn title_style(accent: Color) -> Style {
    Style::default().fg(accent).add_modifier(Modifier::BOLD)
}

pub fn muted_style() -> Style {
    Style::default().fg(SUBTEXT)
}
