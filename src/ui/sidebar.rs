use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::data::{DatasetMetadata, Variable};
use crate::render::colors::{Palette, ScaleMode};

use super::theme;

pub fn filter_variables<'a>(variables: &'a [Variable], query: &str) -> Vec<&'a Variable> {
    let query = query.to_ascii_lowercase();
    if query.is_empty() {
        return variables.iter().collect();
    }
    variables
        .iter()
        .filter(|variable| {
            let name = variable.name.to_ascii_lowercase();
            let mut chars = name.chars();
            query
                .chars()
                .all(|needle| chars.by_ref().any(|candidate| candidate == needle))
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn render(
    frame: &mut Frame,
    area: Rect,
    filename: &str,
    metadata: &DatasetMetadata,
    selected_variable: Option<&str>,
    palette: &Palette,
    limits: Option<(f64, f64)>,
    filter_range: Option<(f64, f64)>,
    scale: ScaleMode,
) {
    render_with_search(
        frame,
        area,
        filename,
        metadata,
        selected_variable,
        palette,
        limits,
        filter_range,
        scale,
        "",
        false,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn render_with_search(
    frame: &mut Frame,
    area: Rect,
    filename: &str,
    metadata: &DatasetMetadata,
    selected_variable: Option<&str>,
    palette: &Palette,
    limits: Option<(f64, f64)>,
    filter_range: Option<(f64, f64)>,
    scale: ScaleMode,
    variable_query: &str,
    variable_search_active: bool,
) {
    let limit_text = limits.map_or_else(
        || "limits: auto".to_string(),
        |(min, max)| format!("limits: {min:.4}..{max:.4}"),
    );
    let plottable = metadata
        .variables
        .iter()
        .filter(|variable| variable.numeric && variable.dimensions.len() >= 2)
        .cloned()
        .collect::<Vec<_>>();
    let variables = filter_variables(&plottable, variable_query)
        .into_iter()
        .take(8)
        .map(|variable| {
            let marker = if selected_variable == Some(variable.name.as_str()) {
                "▶"
            } else {
                " "
            };
            (marker.to_string(), variable.name.clone())
        })
        .collect::<Vec<_>>();
    let dimensions = metadata.dimensions.iter().take(8).collect::<Vec<_>>();
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!("{} ", theme::ICON_FILE),
            theme::title_style(theme::BLUE),
        ),
        Span::styled(filename, theme::muted_style()),
    ])];
    lines.push(Line::from(Span::styled(
        format!("{}  Colormap", theme::ICON_PALETTE),
        theme::title_style(theme::PEACH),
    )));
    let palette_label = if palette.is_reversed() {
        format!("{} (reversed)", palette.name())
    } else {
        palette.name().to_string()
    };
    lines.push(Line::from(vec![
        Span::styled("  ● ", Style::default().fg(theme::PEACH)),
        Span::styled(palette_label, Style::default().fg(theme::TEXT)),
    ]));
    lines.push(Line::from(Span::styled(
        format!("  scale: {}", scale.name()),
        theme::muted_style(),
    )));
    let button = |label: &str, color| {
        Span::styled(
            label.to_string(),
            Style::default().fg(theme::TEXT).bg(color),
        )
    };
    lines.push(Line::from(vec![
        button("[c] MAP", theme::SURFACE_ALT),
        Span::raw(" "),
        button("[a] AUTO", theme::SURFACE_ALT),
        Span::raw(" "),
        button("[l]LIM", theme::SURFACE_ALT),
    ]));
    lines.push(Line::from(vec![
        button("[f]MASK", theme::SURFACE_ALT),
        Span::raw(" "),
        button("[x] AXES", theme::SURFACE_ALT),
        Span::raw(" "),
        button("[r] RESET", theme::SURFACE_ALT),
    ]));
    lines.push(Line::from(Span::styled(
        format!("{}  Variables", theme::ICON_VARIABLE),
        theme::title_style(theme::MAUVE),
    )));
    lines.push(Line::from(vec![
        Span::styled(
            "  / ",
            theme::title_style(if variable_search_active {
                theme::TEAL
            } else {
                theme::SURFACE_ALT
            }),
        ),
        Span::styled(
            if variable_query.is_empty() {
                "type to search variables"
            } else {
                variable_query
            },
            if variable_search_active {
                Style::default().fg(theme::TEXT)
            } else {
                theme::muted_style()
            },
        ),
    ]));
    if variables.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no plottable fields",
            theme::muted_style(),
        )));
    } else {
        for (marker, name) in variables {
            let selected = marker == "▶";
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {marker} "),
                    theme::title_style(if selected {
                        theme::TEAL
                    } else {
                        theme::SURFACE_ALT
                    }),
                ),
                Span::styled(
                    name,
                    Style::default().fg(if selected {
                        theme::TEXT
                    } else {
                        theme::SUBTEXT
                    }),
                ),
            ]));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("{}  Dimensions", theme::ICON_DIMENSION),
        theme::title_style(theme::BLUE),
    )));
    for dimension in dimensions {
        lines.push(Line::from(vec![
            Span::styled("  • ", theme::muted_style()),
            Span::styled(dimension.name.as_str(), Style::default().fg(theme::TEXT)),
            Span::styled(format!(" = {}", dimension.length), theme::muted_style()),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("  ↕ ", Style::default().fg(theme::TEAL)),
        Span::styled(limit_text, theme::muted_style()),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  ▪ ", Style::default().fg(theme::RED)),
        Span::styled(
            filter_range.map_or_else(
                || "mask: off".into(),
                |(min, max)| format!("mask: {min:.4}..{max:.4}"),
            ),
            theme::muted_style(),
        ),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("{}  Grid", theme::ICON_GRID),
        theme::title_style(theme::TEAL),
    )));
    lines.push(Line::from(Span::styled(
        "  g  logical  •  projected   v  reverse   b  coastline   i  filter   e  export",
        theme::muted_style(),
    )));
    frame.render_widget(
        Paragraph::new(lines).block(theme::panel("Dataset", theme::BLUE)),
        area,
    );
}
