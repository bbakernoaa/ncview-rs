use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::data::slice::Slice2D;
use crate::data::{DatasetMetadata, Variable};
use crate::ui::level::{LevelPanel, level_section, level_window};
use crate::{
    app::ColorScaleScope,
    render::colors::{Palette, ScaleMode},
};

use super::theme;

fn truncate_text(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    if max_chars <= 1 {
        return "…".chars().take(max_chars).collect();
    }
    let mut result = value.chars().take(max_chars - 1).collect::<String>();
    result.push('…');
    result
}

fn truncate_variable_name(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    if max_chars <= 1 {
        return "…".chars().take(max_chars).collect();
    }

    let characters = value.chars().collect::<Vec<_>>();
    let suffix_len = (max_chars - 1) / 2;
    let prefix_len = max_chars - 1 - suffix_len;
    let prefix = characters[..prefix_len].iter().collect::<String>();
    let suffix = characters[characters.len() - suffix_len..]
        .iter()
        .collect::<String>();
    format!("{prefix}…{suffix}")
}

fn truncate_path(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    if max_chars <= 1 {
        return "…".chars().take(max_chars).collect();
    }
    let tail = value
        .rsplit_once('/')
        .map_or(value, |(_, filename)| filename);
    format!("…/{}", truncate_text(tail, max_chars.saturating_sub(2)))
}

pub fn filter_variables<'a>(variables: &'a [Variable], query: &str) -> Vec<&'a Variable> {
    let query = query.to_ascii_lowercase();
    let mut matched: Vec<&'a Variable> = if query.is_empty() {
        variables.iter().collect()
    } else {
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
    };
    // Present variables in a stable alphabetical order rather than the
    // dataset's stored order, so the sidebar, browse popup, and the click
    // hit-test (which indexes into this same list) all agree. The comparison
    // is case-insensitive with a name tiebreak to stay deterministic.
    matched.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });
    matched
}

/// The bordered boxes the sidebar draws, in vertical order. Each `Rect` is a
/// complete panel area (border included) or `Rect::ZERO` when the box is
/// hidden. Shared by the renderer and the mouse hit-test so click targets can
/// never drift from what is drawn (the old flat layout used magic row offsets
/// for exactly this and they drifted constantly).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarBoxes {
    pub file: Rect,
    pub controls: Rect,
    pub variables: Rect,
    pub level: Rect,
    pub scale: Rect,
}

/// Split the sidebar `area` into stacked bordered boxes. Every box is fixed
/// height and a trailing spacer absorbs leftover terminal height, so the
/// boxes always stack compactly at the top. When the terminal is too short
/// the last boxes clip, matching the previous single-panel behaviour.
pub fn sidebar_boxes(area: Rect, show_level: bool) -> SidebarBoxes {
    use ratatui::layout::{Constraint, Direction, Layout};
    let file = Constraint::Length(5);
    let controls = Constraint::Length(6);
    let variables = Constraint::Length(11);
    let level = if show_level {
        Constraint::Length(6)
    } else {
        Constraint::Length(0)
    };
    let scale = Constraint::Length(7);
    let splits = Layout::default()
        .direction(Direction::Vertical)
        .constraints([file, controls, variables, level, scale, Constraint::Min(0)])
        .split(area);
    SidebarBoxes {
        file: splits[0],
        controls: splits[1],
        variables: splits[2],
        level: splits[3],
        scale: splits[4],
    }
}

/// Format a numeric value for the sidebar's color-scale readout. Small or
/// large magnitudes use scientific notation so the full range stays visible
/// without hiding precision behind a fixed decimal display.
fn format_scale_value(value: f64) -> String {
    let magnitude = value.abs();
    if value.is_finite() && magnitude > 0.0 && !(1.0e-3..1.0e5).contains(&magnitude) {
        format!("{value:.2e}")
    } else {
        format!("{value:.4}")
    }
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
    color_scale_scope: ColorScaleScope,
) {
    let plottable = metadata
        .variables
        .iter()
        .filter(|variable| variable.numeric && variable.dimensions.len() >= 2)
        .cloned()
        .collect::<Vec<_>>();
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
        color_scale_scope,
        "",
        false,
        &plottable,
        None,
        None,
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
    color_scale_scope: ColorScaleScope,
    variable_query: &str,
    variable_search_active: bool,
    plottable: &[Variable],
    slice: Option<&Slice2D>,
    level: Option<LevelPanel<'_>>,
) {
    let boxes = sidebar_boxes(area, level.is_some());
    let content_width = usize::from(area.width.saturating_sub(2));
    render_file_box(frame, boxes.file, filename, palette, scale);
    render_controls_box(frame, boxes.controls);
    render_variables_box(
        frame,
        boxes.variables,
        plottable,
        metadata,
        selected_variable,
        variable_query,
        variable_search_active,
        content_width,
    );
    if let Some(panel) = level {
        render_level_box(frame, boxes.level, panel, content_width);
    }
    render_scale_box(
        frame,
        boxes.scale,
        metadata,
        limits,
        filter_range,
        color_scale_scope,
        slice,
        content_width,
    );
}

fn render_file_box(frame: &mut Frame, area: Rect, filename: &str, palette: &Palette, scale: ScaleMode) {
    let block = theme::panel("File", theme::BLUE);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if area.height < 2 || area.width < 2 {
        return;
    }
    let palette_label = if palette.is_reversed() {
        format!("{} (reversed)", palette.name())
    } else {
        palette.name().to_string()
    };
    let lines = vec![
        Line::from(vec![
            Span::styled(format!("{} ", theme::ICON_FILE), theme::title_style(theme::BLUE)),
            Span::styled(
                truncate_path(filename, usize::from(inner.width.saturating_sub(2))),
                theme::muted_style(),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{} ● ", theme::ICON_PALETTE),
                Style::default().fg(theme::PEACH),
            ),
            Span::styled(
                truncate_text(&palette_label, usize::from(inner.width.saturating_sub(6)).max(1)),
                Style::default().fg(theme::TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("  scale: {}  ", scale.name()),
                theme::muted_style(),
            ),
            Span::styled("[s]", theme::title_style(theme::TEAL)),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines).style(Style::default().bg(theme::SURFACE)), inner);
}

fn render_controls_box(frame: &mut Frame, area: Rect) {
    let block = theme::panel("Controls", theme::TEAL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if area.height < 2 || area.width < 2 {
        return;
    }
    let button = |label: &str| Span::styled(label.to_string(), Style::default().fg(theme::TEXT).bg(theme::SURFACE_ALT));
    let lines = vec![
        Line::from(vec![
            button("[c] MAP"),
            Span::raw(" "),
            button("[v] REV"),
            Span::raw(" "),
            button("[a] AUTO"),
        ]),
        Line::from(vec![
            button("[l] LIM"),
            Span::raw(" "),
            button("[f]MASK"),
            Span::raw(" "),
            button("[x] AXES"),
        ]),
        Line::from(vec![
            button("[r] RESET"),
            Span::raw(" "),
            button("[<] DATE"),
            Span::raw(" "),
            button("[>] DATE"),
        ]),
        Line::from(vec![
            button("[-]SPD"),
            Span::raw(" "),
            button("[+]SPD"),
            Span::raw(" "),
            button("[z]SCALE"),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines).style(Style::default().bg(theme::SURFACE)), inner);
}

fn render_variables_box(
    frame: &mut Frame,
    area: Rect,
    plottable: &[Variable],
    metadata: &DatasetMetadata,
    selected_variable: Option<&str>,
    variable_query: &str,
    variable_search_active: bool,
    content_width: usize,
) {
    let title = "Variables";
    let block = theme::panel(title, theme::MAUVE);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if area.height < 2 || area.width < 2 {
        return;
    }
    let muted = |text: String| {
        Line::from(Span::styled(
            truncate_text(&text, content_width),
            theme::muted_style(),
        ))
    };
    let mut lines = Vec::new();
    let selected = selected_variable
        .and_then(|name| plottable.iter().find(|variable| variable.name == name));
    if let Some(variable) = selected {
        lines.push(Line::from(vec![
            Span::styled("  ▶ ", theme::title_style(theme::TEAL)),
            Span::styled(
                truncate_variable_name(&variable.name, content_width.saturating_sub(4)),
                Style::default().fg(theme::TEXT),
            ),
        ]));
        // Description line: prefer the CF long name, fall back to the standard
        // name so the field is still recognisable when a producer omits it.
        if let Some(description) = variable
            .long_name
            .as_deref()
            .or(variable.standard_name.as_deref())
        {
            lines.push(muted(format!("  {description}")));
        }
        let units = variable.units.as_deref().unwrap_or("-");
        lines.push(muted(format!("  units: {units}")));
        // Dimension shape with lengths, e.g. "date(11) lat(48) lon(64)".
        let shape = variable
            .dimensions
            .iter()
            .map(|name| {
                metadata
                    .dimensions
                    .iter()
                    .find(|dimension| &dimension.name == name)
                    .map_or_else(
                        || name.clone(),
                        |dimension| format!("{name}({})", dimension.length),
                    )
            })
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(muted(format!("  {shape}")));
    } else if plottable.is_empty() {
        lines.push(Line::from(Span::styled("  no fields", theme::muted_style())));
    } else {
        lines.push(Line::from(Span::styled(
            "  none selected",
            theme::muted_style(),
        )));
    }
    // Separate the current field from the dataset-wide shape summary so the
    // two don't visually run together.
    lines.push(Line::from(""));
    // Explicit per-rank counts. Ranks 2..=4 are always listed (even when
    // empty) so the breakdown reads unambiguously; higher ranks appear only
    // when the dataset actually has them.
    let mut counts: std::collections::BTreeMap<usize, usize> = Default::default();
    for variable in plottable {
        *counts.entry(variable.dimensions.len()).or_default() += 1;
    }
    let max_rank = counts.keys().next_back().copied().unwrap_or(2).max(4);
    for rank in 2..=max_rank {
        lines.push(muted(format!(
            "  {rank}D variables: {}",
            counts.get(&rank).copied().unwrap_or(0)
        )));
    }
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
                "browse all variables…"
            } else {
                "filtering…"
            },
            if variable_search_active {
                Style::default().fg(theme::TEXT)
            } else {
                theme::muted_style()
            },
        ),
    ]));
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::SURFACE)),
        inner,
    );
}

fn render_level_box(frame: &mut Frame, area: Rect, panel: LevelPanel<'_>, content_width: usize) {
    let accent = if panel.focused { theme::TEAL } else { theme::BLUE };
    let block = theme::panel("Level", accent);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let stepper_span = (content_width / 2) as u16;
    let Some(section) = level_section(area, panel.labels.len(), stepper_span) else {
        return;
    };
    let (top, len) = level_window(panel.labels.len(), section.list_rows, panel.cursor);
    let button = |label: &str| Span::styled(label.to_string(), Style::default().fg(theme::TEXT).bg(theme::SURFACE_ALT));
    let readout = panel
        .labels
        .get(panel.selected)
        .cloned()
        .unwrap_or_else(|| format!("index {}", panel.selected));
    let mut lines = vec![
        Line::from(Span::styled(
            truncate_text(
                &format!("{} [{} / {}]", readout, panel.selected + 1, panel.labels.len()),
                content_width,
            ),
            Style::default().fg(theme::TEXT),
        )),
        Line::from(vec![
            button("◂ Prev"),
            Span::raw("  "),
            button("Next ▸"),
        ]),
    ];
    for index in top..top + len {
        let label = panel
            .labels
            .get(index)
            .map_or_else(|| format!("index {index}"), String::clone);
        let is_selected = index == panel.selected;
        let is_cursor = panel.focused && index == panel.cursor;
        let marker = if is_cursor {
            "▸"
        } else if is_selected {
            "●"
        } else {
            " "
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{marker} "),
                theme::title_style(if is_cursor || is_selected { theme::TEAL } else { theme::SURFACE_ALT }),
            ),
            Span::styled(
                truncate_text(&label, content_width.saturating_sub(2)),
                Style::default().fg(if is_selected { theme::TEXT } else { theme::SUBTEXT }),
            ),
        ]));
    }
    frame.render_widget(Paragraph::new(lines).style(Style::default().bg(theme::SURFACE)), inner);
}

#[allow(clippy::too_many_arguments)]
fn render_scale_box(
    frame: &mut Frame,
    area: Rect,
    metadata: &DatasetMetadata,
    limits: Option<(f64, f64)>,
    filter_range: Option<(f64, f64)>,
    color_scale_scope: ColorScaleScope,
    slice: Option<&Slice2D>,
    content_width: usize,
) {
    let block = theme::panel("Scale", theme::PEACH);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if area.height < 2 || area.width < 2 {
        return;
    }
    let limit_text = limits.map_or_else(
        || "  ↕ limits  auto".to_string(),
        |(min, max)| {
            format!(
                "  ↕ limits  {} … {}",
                format_scale_value(min),
                format_scale_value(max)
            )
        },
    );
    let mask_text = filter_range.map_or_else(
        || "  ▪ mask    off".to_string(),
        |(min, max)| {
            format!(
                "  ▪ mask    {} … {}",
                format_scale_value(min),
                format_scale_value(max)
            )
        },
    );
    let dims_summary = if metadata.dimensions.is_empty() {
        "no dimensions".to_string()
    } else {
        metadata
            .dimensions
            .iter()
            .map(|dimension| format!("{}={}", dimension.name, dimension.length))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let stats_text = slice.and_then(|slice| slice.statistics).map_or_else(
        || "  stats   no data".to_string(),
        |stats| {
            format!(
                "  stats   mean {}  n {}",
                format_scale_value(stats.mean),
                stats.finite_count
            )
        },
    );
    let lines = vec![
        Line::from(Span::styled(
            format!("{}  {}", theme::ICON_DIMENSION, dims_summary),
            theme::muted_style(),
        )),
        Line::from(Span::styled(
            truncate_text(&limit_text, content_width),
            theme::muted_style(),
        )),
        Line::from(Span::styled(
            truncate_text(&mask_text, content_width),
            theme::muted_style(),
        )),
        Line::from(Span::styled(
            truncate_text(&stats_text, content_width),
            theme::muted_style(),
        )),
        Line::from(Span::styled(
            format!("  scope: {}  [z]", color_scale_scope.name()),
            theme::muted_style(),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).style(Style::default().bg(theme::SURFACE)), inner);
}

#[cfg(test)]
mod tests {
    use super::{filter_variables, truncate_variable_name};

    fn empty_metadata() -> crate::data::DatasetMetadata {
        crate::data::DatasetMetadata {
            path: "f.nc".into(),
            format: crate::data::DatasetFormat::NetCdf4,
            dimensions: Vec::new(),
            variables: Vec::new(),
        }
    }

    fn render_sidebar(level: Option<crate::ui::level::LevelPanel<'_>>) -> String {
        use ratatui::{Terminal, backend::TestBackend};
        let mut terminal = Terminal::new(TestBackend::new(32, 30)).unwrap();
        terminal
            .draw(|frame| {
                super::render_with_search(
                    frame,
                    frame.area(),
                    "f.nc",
                    &empty_metadata(),
                    None,
                    &crate::render::colors::Palette::Viridis,
                    None,
                    None,
                    crate::render::colors::ScaleMode::Linear,
                    crate::app::ColorScaleScope::CurrentView,
                    "",
                    false,
                    &[],
                    None,
                    level,
                )
            })
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
    fn level_section_renders_header_stepper_and_list() {
        use crate::ui::level::LevelPanel;
        let labels: Vec<String> = (0..6).map(|i| format!("L{i}")).collect();
        let rendered = render_sidebar(Some(LevelPanel {
            labels: &labels,
            selected: 2,
            cursor: 2,
            focused: false,
        }));
        assert!(rendered.contains("Level"), "{rendered}");
        assert!(rendered.contains("L2 [3 / 6]"), "{rendered}");
        assert!(rendered.contains("Prev"), "{rendered}");
        assert!(rendered.contains("Next"), "{rendered}");
    }

    #[test]
    fn level_section_is_absent_without_levels() {
        let rendered = render_sidebar(None);
        assert!(!rendered.contains("Level"), "{rendered}");
    }

    #[test]
    fn variable_truncation_preserves_both_ends() {
        let name = "dust_dry_gt0um_aerosol_optical_thickness_545nm-565nm";
        let displayed = truncate_variable_name(name, 26);

        assert_eq!(displayed.chars().count(), 26);
        assert!(displayed.starts_with("dust_dry_gt0"));
        assert!(displayed.ends_with("545nm-565nm"));
        assert!(displayed.contains('…'));
    }

    #[test]
    fn short_variable_names_are_unchanged() {
        assert_eq!(truncate_variable_name("temperature", 26), "temperature");
    }

    fn variable(name: &str) -> crate::data::Variable {
        crate::data::Variable {
            name: name.into(),
            dimensions: vec!["lat".into(), "lon".into()],
            numeric: true,
            units: None,
            long_name: None,
            standard_name: None,
        }
    }

    #[test]
    fn filter_variables_sorts_alphabetically() {
        let variables = [
            variable("total_ozone"),
            variable("air"),
            variable("Pressure"),
            variable("beta"),
        ];
        let order: Vec<&str> = filter_variables(&variables, "")
            .into_iter()
            .map(|variable| variable.name.as_str())
            .collect();
        // Case-insensitive alphabetical, not the stored order.
        assert_eq!(order, vec!["air", "beta", "Pressure", "total_ozone"]);

        // The ordering is preserved when a fuzzy query narrows the list.
        let filtered: Vec<&str> = filter_variables(&variables, "e")
            .into_iter()
            .map(|variable| variable.name.as_str())
            .collect();
        assert_eq!(filtered, vec!["beta", "Pressure", "total_ozone"]);
    }
}
