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
    pub dimensions: Rect,
}

/// Split the sidebar `area` into stacked bordered boxes. Every box is fixed
/// height and a trailing spacer absorbs leftover terminal height, so the
/// boxes always stack compactly at the top. When the terminal is too short
/// the last boxes clip, matching the previous single-panel behaviour.
pub fn sidebar_boxes(area: Rect, show_level: bool) -> SidebarBoxes {
    use ratatui::layout::{Constraint, Direction, Layout};
    let file = Constraint::Length(5);
    let controls = Constraint::Length(6);
    let variables = Constraint::Length(13);
    let dimensions = Constraint::Length(7);
    let level = if show_level {
        Constraint::Length(6)
    } else {
        Constraint::Length(0)
    };
    // The Scale box keeps a fixed row layout (limits, mask, stats, scope) so
    // the mouse hit-test rows never shift with the content.
    let scale = Constraint::Length(9);
    let splits = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            file,
            controls,
            variables,
            dimensions,
            level,
            scale,
            Constraint::Min(0),
        ])
        .split(area);
    SidebarBoxes {
        file: splits[0],
        controls: splits[1],
        variables: splits[2],
        level: splits[4],
        scale: splits[5],
        dimensions: splits[3],
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

/// Render one `label  value` row inside a sidebar box. The sidebar is only
/// ~30 columns wide, so every readout is laid out as a label in a fixed
/// column with the value truncated to whatever room remains, instead of
/// cramming label and value onto one line that gets clipped by the border.
fn labeled_row(label: &str, value: &str, content_width: usize) -> Line<'static> {
    // All labels share one column so a box's values line up vertically.
    const LABEL_COLUMN: usize = 10;
    let label_width = LABEL_COLUMN.min(content_width.saturating_sub(4));
    let value_width = content_width.saturating_sub(label_width + 4);
    let label_text = format!("{:<label_width$}", truncate_text(label, label_width));
    Line::from(vec![
        Span::raw("  "),
        Span::styled(label_text, theme::muted_style()),
        Span::raw("  "),
        Span::styled(
            truncate_text(value, value_width),
            Style::default().fg(theme::TEXT),
        ),
    ])
}

/// Break a long string into lines no wider than `width`, preferring word
/// boundaries and hard-breaking words that are themselves too long. Used so
/// metadata (descriptions, dimension shapes) wraps instead of being clipped
/// by the sidebar's narrow right border.
fn wrap_text(value: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in value.split_whitespace() {
        let word_len = word.chars().count();
        if word_len > width {
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            let mut chunk = String::new();
            let mut chunk_len = 0;
            for ch in word.chars() {
                if chunk_len == width {
                    lines.push(std::mem::take(&mut chunk));
                    chunk_len = 0;
                }
                chunk.push(ch);
                chunk_len += 1;
            }
            if !chunk.is_empty() {
                lines.push(chunk);
            }
            continue;
        }
        let extra = if current.is_empty() {
            word_len
        } else {
            word_len + 1
        };
        if current.chars().count() + extra > width {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
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
    render_dimensions_box(frame, boxes.dimensions, metadata);
    render_scale_box(
        frame,
        boxes.scale,
        limits,
        filter_range,
        color_scale_scope,
        slice,
        content_width,
    );
}

fn render_file_box(
    frame: &mut Frame,
    area: Rect,
    filename: &str,
    palette: &Palette,
    scale: ScaleMode,
) {
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
            Span::styled(
                format!("{} ", theme::ICON_FILE),
                theme::title_style(theme::BLUE),
            ),
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
                truncate_text(
                    &palette_label,
                    usize::from(inner.width.saturating_sub(6)).max(1),
                ),
                Style::default().fg(theme::TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled(format!("  scale: {}  ", scale.name()), theme::muted_style()),
            Span::styled("[s]", theme::title_style(theme::TEAL)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::SURFACE)),
        inner,
    );
}

fn render_controls_box(frame: &mut Frame, area: Rect) {
    let block = theme::panel("Controls", theme::TEAL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if area.height < 2 || area.width < 2 {
        return;
    }
    let button = |label: &str| {
        Span::styled(
            label.to_string(),
            Style::default().fg(theme::TEXT).bg(theme::SURFACE_ALT),
        )
    };
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
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::SURFACE)),
        inner,
    );
}

#[allow(clippy::too_many_arguments)]
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
    let selected =
        selected_variable.and_then(|name| plottable.iter().find(|variable| variable.name == name));
    if let Some(variable) = selected {
        lines.push(Line::from(vec![
            Span::styled("  ▶ ", theme::title_style(theme::TEAL)),
            Span::styled(
                truncate_variable_name(&variable.name, content_width.saturating_sub(4)),
                Style::default().fg(theme::TEXT),
            ),
        ]));
        // Description: prefer the CF long name, fall back to the standard
        // name so the field is still recognisable when a producer omits it.
        // Wrapped (max two lines) rather than truncated: the sidebar is far
        // too narrow for a full long_name on one line.
        if let Some(description) = variable
            .long_name
            .as_deref()
            .or(variable.standard_name.as_deref())
        {
            for chunk in wrap_text(description, content_width.saturating_sub(2))
                .into_iter()
                .take(2)
            {
                lines.push(muted(format!("  {chunk}")));
            }
        }
        let units = variable.units.as_deref().unwrap_or("-");
        lines.push(labeled_row("units", units, content_width));
        // Dimension shape with lengths, e.g. "date(11) lat(48) lon(64)".
        // Long shapes wrap onto continuation lines aligned under the value
        // column instead of running off the border.
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
        let mut shape_lines = wrap_text(&shape, content_width.saturating_sub(15)).into_iter();
        if let Some(first) = shape_lines.next() {
            lines.push(labeled_row("shape", &first, content_width));
        }
        for chunk in shape_lines.take(1) {
            lines.push(muted(format!("{}{chunk}", " ".repeat(14))));
        }
    } else if plottable.is_empty() {
        lines.push(Line::from(Span::styled(
            "  no fields",
            theme::muted_style(),
        )));
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
    let accent = if panel.focused {
        theme::TEAL
    } else {
        theme::BLUE
    };
    let block = theme::panel("Level", accent);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let stepper_span = (content_width / 2) as u16;
    let Some(section) = level_section(area, panel.labels.len(), stepper_span) else {
        return;
    };
    let (top, len) = level_window(panel.labels.len(), section.list_rows, panel.cursor);
    let button = |label: &str| {
        Span::styled(
            label.to_string(),
            Style::default().fg(theme::TEXT).bg(theme::SURFACE_ALT),
        )
    };
    let readout = panel
        .labels
        .get(panel.selected)
        .cloned()
        .unwrap_or_else(|| format!("index {}", panel.selected));
    let mut lines = vec![
        Line::from(Span::styled(
            truncate_text(
                &format!(
                    "{} [{} / {}]",
                    readout,
                    panel.selected + 1,
                    panel.labels.len()
                ),
                content_width,
            ),
            Style::default().fg(theme::TEXT),
        )),
        Line::from(vec![button("◂ Prev"), Span::raw("  "), button("Next ▸")]),
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
                theme::title_style(if is_cursor || is_selected {
                    theme::TEAL
                } else {
                    theme::SURFACE_ALT
                }),
            ),
            Span::styled(
                truncate_text(&label, content_width.saturating_sub(2)),
                Style::default().fg(if is_selected {
                    theme::TEXT
                } else {
                    theme::SUBTEXT
                }),
            ),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::SURFACE)),
        inner,
    );
}

/// Dataset dimensions (`name = length`), restored to its own box so long
/// dimension lists scroll-free and never crowd the color-scale readout.
fn render_dimensions_box(frame: &mut Frame, area: Rect, metadata: &DatasetMetadata) {
    let block = theme::panel("Dimensions", theme::BLUE);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if area.height < 2 || area.width < 2 {
        return;
    }
    let content_width = usize::from(inner.width);
    let lines: Vec<Line> = if metadata.dimensions.is_empty() {
        vec![Line::from(Span::styled(
            "  no dimensions",
            theme::muted_style(),
        ))]
    } else {
        let name_width = content_width.saturating_sub(8).clamp(4, 18);
        let rest = content_width.saturating_sub(name_width + 2);
        metadata
            .dimensions
            .iter()
            .take(usize::from(inner.height).max(1))
            .map(|dimension| {
                Line::from(Span::styled(
                    format!(
                        "  {:<name_width$}{:>rest$}",
                        truncate_text(&dimension.name, name_width),
                        dimension.length
                    ),
                    Style::default().fg(theme::TEXT),
                ))
            })
            .collect()
    };
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::SURFACE)),
        inner,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_scale_box(
    frame: &mut Frame,
    area: Rect,
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
    // Fixed row layout so the mouse hit-test offsets never shift with the
    // content. Limits and mask fit on one line each; the whole-image
    // statistics are the long readout, so each figure gets its own line
    // instead of being clipped by the sidebar's narrow right border.
    //   row  0  : colour-scale limits (min … max)
    //   row  1  : value mask (min … max)
    //   rows 2-5: whole-image statistics (min, max, mean, count)
    //   row  6  : colour-scale scope
    let mut lines = Vec::new();
    let limit_text = limits.map_or_else(
        || "auto".to_string(),
        |(min, max)| format!("{} … {}", format_scale_value(min), format_scale_value(max)),
    );
    lines.push(labeled_row("limits", &limit_text, content_width));
    let mask_text = filter_range.map_or_else(
        || "off".to_string(),
        |(min, max)| format!("{} … {}", format_scale_value(min), format_scale_value(max)),
    );
    lines.push(labeled_row("mask", &mask_text, content_width));
    match slice.and_then(|slice| slice.statistics) {
        Some(stats) => {
            lines.push(labeled_row(
                "stats min",
                &format_scale_value(stats.min),
                content_width,
            ));
            lines.push(labeled_row(
                "stats max",
                &format_scale_value(stats.max),
                content_width,
            ));
            lines.push(labeled_row(
                "stats mean",
                &format_scale_value(stats.mean),
                content_width,
            ));
            lines.push(labeled_row(
                "stats n",
                &stats.finite_count.to_string(),
                content_width,
            ));
        }
        None => {
            lines.push(labeled_row("stats", "no data", content_width));
            for _ in 0..3 {
                lines.push(labeled_row("", "", content_width));
            }
        }
    }
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{:<10}", "scope"), theme::muted_style()),
        Span::raw("  "),
        Span::styled(
            truncate_text(color_scale_scope.name(), content_width.saturating_sub(19)),
            Style::default().fg(theme::TEXT),
        ),
        Span::styled("  [z]", theme::title_style(theme::TEAL)),
    ]));
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::SURFACE)),
        inner,
    );
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
        render_sidebar_with(level, &empty_metadata(), None)
    }

    /// Render the sidebar into a string with custom metadata and an optional
    /// loaded slice, so tests can assert on the Dimensions and Scale boxes.
    fn render_sidebar_with(
        level: Option<crate::ui::level::LevelPanel<'_>>,
        metadata: &crate::data::DatasetMetadata,
        slice: Option<&crate::data::slice::Slice2D>,
    ) -> String {
        use ratatui::{Terminal, backend::TestBackend};
        // Tall enough that every box keeps its natural height; ratatui
        // squeezes all fixed-height boxes down when their total exceeds the
        // area, which would hide the rows these tests check for.
        let mut terminal = Terminal::new(TestBackend::new(32, 46)).unwrap();
        terminal
            .draw(|frame| {
                super::render_with_search(
                    frame,
                    frame.area(),
                    "f.nc",
                    metadata,
                    None,
                    &crate::render::colors::Palette::Viridis,
                    None,
                    None,
                    crate::render::colors::ScaleMode::Linear,
                    crate::app::ColorScaleScope::CurrentView,
                    "",
                    false,
                    &[],
                    slice,
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
    fn dimensions_box_lists_each_dataset_dimension() {
        let metadata = crate::data::DatasetMetadata {
            path: "f.nc".into(),
            format: crate::data::DatasetFormat::NetCdf4,
            dimensions: vec![
                crate::data::Dimension {
                    name: "time".into(),
                    length: 11,
                    role: crate::data::AxisRole::Time,
                },
                crate::data::Dimension {
                    name: "latitude".into(),
                    length: 48,
                    role: crate::data::AxisRole::Latitude,
                },
                crate::data::Dimension {
                    name: "longitude".into(),
                    length: 64,
                    role: crate::data::AxisRole::Longitude,
                },
            ],
            variables: Vec::new(),
        };
        let rendered = render_sidebar_with(None, &metadata, None);
        assert!(rendered.contains("Dimensions"), "{rendered}");
        assert!(rendered.contains("latitude"), "{rendered}");
        assert!(rendered.contains("48"), "{rendered}");
        assert!(rendered.contains("longitude"), "{rendered}");
        assert!(rendered.contains("64"), "{rendered}");
    }

    #[test]
    fn scale_box_stacks_statistics_on_separate_lines() {
        let slice = crate::data::slice::Slice2D {
            values: ndarray::Array2::from_elem((2, 2), 1.0),
            validity: ndarray::Array2::from_elem((2, 2), crate::data::slice::Validity::Finite),
            source_bounds: crate::data::slice::Bounds {
                row_start: 0,
                row_end: 2,
                col_start: 0,
                col_end: 2,
            },
            statistics: Some(crate::data::slice::Statistics {
                min: -12.5,
                max: 345.678,
                mean: 100.25,
                finite_count: 4,
            }),
            coordinates: None,
            is_diff: false,
        };
        let rendered = render_sidebar_with(None, &empty_metadata(), Some(&slice));
        // Each statistic is labelled on its own line rather than packed onto
        // one long line that the narrow sidebar would clip.
        assert!(rendered.contains("stats min"), "{rendered}");
        assert!(rendered.contains("stats max"), "{rendered}");
        assert!(rendered.contains("stats mean"), "{rendered}");
        assert!(rendered.contains("stats n"), "{rendered}");
        assert!(rendered.contains("-12.5"), "{rendered}");
        assert!(rendered.contains("345.678"), "{rendered}");
        assert!(rendered.contains("100.25"), "{rendered}");
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
