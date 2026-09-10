use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::render::colors::{Palette, ScaleMode, colorbar};

use super::theme;

#[derive(Debug, Clone, PartialEq)]
struct Tick {
    value: f64,
    label: String,
}

/// Render a right-hand, vertical colorbar similar to a traditional Ncview
/// legend. The bar is intentionally cell-based so it remains useful on
/// terminals without sixel/kitty image support.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    palette: &Palette,
    limits: Option<(f64, f64)>,
    scale: ScaleMode,
    variable: Option<&str>,
) {
    render_with_units(frame, area, palette, limits, scale, variable, None);
}

pub fn render_with_units(
    frame: &mut Frame,
    area: Rect,
    palette: &Palette,
    limits: Option<(f64, f64)>,
    scale: ScaleMode,
    variable: Option<&str>,
    units: Option<&str>,
) {
    render_with_metadata(
        frame, area, palette, limits, scale, variable, units, None, None,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn render_with_metadata(
    frame: &mut Frame,
    area: Rect,
    palette: &Palette,
    limits: Option<(f64, f64)>,
    scale: ScaleMode,
    variable: Option<&str>,
    units: Option<&str>,
    long_name: Option<&str>,
    standard_name: Option<&str>,
) {
    let title = format!("{} Legend", theme::ICON_PALETTE);
    let block = theme::panel(&title, theme::PEACH);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some((min, max)) =
        limits.filter(|(min, max)| min.is_finite() && max.is_finite() && max > min)
    else {
        frame.render_widget(Paragraph::new("no finite range"), inner);
        return;
    };
    if inner.height < 4 || inner.width < 10 {
        frame.render_widget(Paragraph::new("resize for legend"), inner);
        return;
    }

    let width = usize::from(inner.width).max(1);
    let mut metadata_lines = Vec::new();
    if let Some(variable) = variable {
        push_metadata_lines(&mut metadata_lines, variable, true, width);
    }
    if let Some(units) = units {
        push_metadata_lines(
            &mut metadata_lines,
            &format!("units: {units}"),
            false,
            width,
        );
    }
    if let Some(long_name) = long_name {
        push_metadata_lines(
            &mut metadata_lines,
            &format!("long: {long_name}"),
            false,
            width,
        );
    }
    if let Some(standard_name) = standard_name {
        push_metadata_lines(
            &mut metadata_lines,
            &format!("standard: {standard_name}"),
            false,
            width,
        );
    }
    let header_rows = metadata_lines
        .len()
        .min(usize::from(inner.height.saturating_sub(4)));
    if header_rows > 0 {
        let header = Rect::new(inner.x, inner.y, inner.width, header_rows as u16);
        let lines = metadata_lines
            .into_iter()
            .take(header_rows)
            .map(|(line, title)| {
                Line::from(Span::styled(
                    line,
                    if title {
                        theme::title_style(theme::TEXT)
                    } else {
                        theme::muted_style()
                    },
                ))
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines), header);
    }
    let legend = Rect::new(
        inner.x,
        inner.y.saturating_add(header_rows as u16),
        inner.width,
        inner.height.saturating_sub(header_rows as u16),
    );
    let ticks = ticks(min, max, scale, 7);
    let height = usize::from(legend.height);
    let colors = colorbar(palette, height);
    let label_width = usize::from(legend.width.saturating_sub(8)).clamp(5, 11);
    let bar_width = usize::from(legend.width)
        .saturating_sub(label_width + 4)
        .clamp(2, 5);
    let lines = (0..height)
        .map(|row| {
            let rgb = colors[height - row - 1];
            let tick = ticks.iter().find(|tick| {
                let tick_fraction = fraction_for(tick.value, min, max, scale);
                let tick_row = ((1.0 - tick_fraction) * (height - 1) as f64).round() as usize;
                tick_row == row
            });
            let tick_mark = if tick.is_some() { "┤" } else { "│" };
            let label = tick.map_or_else(String::new, |tick| tick.label.clone());
            let label = if label.chars().count() > label_width {
                label.chars().take(label_width).collect::<String>()
            } else {
                format!("{label:<label_width$}")
            };
            let bar = " ".repeat(bar_width);
            Line::from(vec![
                Span::styled(bar, Style::default().bg(Color::Rgb(rgb[0], rgb[1], rgb[2]))),
                Span::styled(
                    format!(" {tick_mark} "),
                    Style::default().fg(if tick.is_some() {
                        theme::TEXT
                    } else {
                        theme::SUBTEXT
                    }),
                ),
                Span::styled(label, theme::muted_style()),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), legend);
}

fn push_metadata_lines(lines: &mut Vec<(String, bool)>, value: &str, title: bool, width: usize) {
    for (index, line) in wrap_metadata(value, width).into_iter().enumerate() {
        lines.push((line, title && index == 0));
    }
}

fn wrap_metadata(value: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut remaining = value.trim().to_string();
    let mut lines = Vec::new();
    while !remaining.is_empty() {
        let characters = remaining.chars().collect::<Vec<_>>();
        if characters.len() <= width {
            lines.push(remaining);
            break;
        }
        let split = characters[..width]
            .iter()
            .rposition(|character| character.is_whitespace())
            .unwrap_or(width);
        lines.push(characters[..split].iter().collect::<String>());
        remaining = characters[split..].iter().collect::<String>();
        remaining = remaining.trim_start().to_string();
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn fraction_for(value: f64, min: f64, max: f64, scale: ScaleMode) -> f64 {
    match scale {
        ScaleMode::Linear => ((value - min) / (max - min)).clamp(0.0, 1.0),
        ScaleMode::Log if min > 0.0 && max > 0.0 && value > 0.0 => {
            ((value.log10() - min.log10()) / (max.log10() - min.log10())).clamp(0.0, 1.0)
        }
        ScaleMode::Log => 0.0,
    }
}

fn ticks(min: f64, max: f64, scale: ScaleMode, target: usize) -> Vec<Tick> {
    let mut values = match scale {
        ScaleMode::Linear => linear_ticks(min, max, target),
        ScaleMode::Log if min > 0.0 && max > 0.0 => log_ticks(min, max, target),
        ScaleMode::Log => vec![min, max],
    };
    values.push(min);
    values.push(max);
    values.sort_by(f64::total_cmp);
    values.dedup_by(|left, right| {
        (*left - *right).abs() <= f64::EPSILON * left.abs().max(right.abs()).max(1.0)
    });
    values
        .into_iter()
        .map(|value| Tick {
            value,
            label: format_tick(value, tick_step(min, max, scale, target)),
        })
        .collect()
}

/// Return the same labeled ticks used by the on-screen legend for exporters
/// and other non-ratatui renderers.
pub fn legend_ticks(min: f64, max: f64, scale: ScaleMode) -> Vec<(f64, String)> {
    ticks(min, max, scale, 7)
        .into_iter()
        .map(|tick| (tick.value, tick.label))
        .collect()
}

fn linear_ticks(min: f64, max: f64, target: usize) -> Vec<f64> {
    let raw = (max - min) / target.saturating_sub(1).max(1) as f64;
    let magnitude = 10.0_f64.powf(raw.log10().floor());
    let normalized = raw / magnitude;
    let multiple = if normalized <= 1.5 {
        1.0
    } else if normalized <= 3.0 {
        2.0
    } else if normalized <= 7.0 {
        5.0
    } else {
        10.0
    };
    let step = multiple * magnitude;
    let first = (min / step).ceil() * step;
    let last = (max / step).floor() * step;
    let mut values = Vec::new();
    let mut value = first;
    while value <= last + step * 1e-9 && values.len() < 20 {
        values.push(value);
        value += step;
    }
    values
}

fn log_ticks(min: f64, max: f64, target: usize) -> Vec<f64> {
    let low = min.log10().floor() as i32;
    let high = max.log10().ceil() as i32;
    let span = high.saturating_sub(low);
    let multipliers: &[f64] = if span <= 2 && target >= 6 {
        &[1.0, 2.0, 5.0]
    } else {
        &[1.0]
    };
    (low..=high)
        .flat_map(|exponent| {
            let base = 10.0_f64.powi(exponent);
            multipliers.iter().map(move |multiplier| base * multiplier)
        })
        .filter(|value| *value >= min && *value <= max)
        .collect()
}

fn tick_step(min: f64, max: f64, scale: ScaleMode, target: usize) -> f64 {
    match scale {
        ScaleMode::Linear => {
            let raw = (max - min) / target.saturating_sub(1).max(1) as f64;
            let magnitude = 10.0_f64.powf(raw.log10().floor());
            let normalized = raw / magnitude;
            let multiple = if normalized <= 1.5 {
                1.0
            } else if normalized <= 3.0 {
                2.0
            } else if normalized <= 7.0 {
                5.0
            } else {
                10.0
            };
            multiple * magnitude
        }
        ScaleMode::Log => 1.0,
    }
}

fn format_tick(value: f64, step: f64) -> String {
    if value.abs() >= 1.0e6 || (value != 0.0 && value.abs() < 1.0e-4) {
        return format!("{value:.2e}");
    }
    if step >= 1.0 && value.fract().abs() > 1.0e-9 {
        return format!("{value:.3}");
    }
    let decimals = if step >= 1.0 {
        0
    } else {
        (-step.log10().floor()).max(0.0) as usize
    };
    format!("{value:.decimals$}")
}

#[cfg(test)]
mod tests {
    use super::{ScaleMode, format_tick, ticks, wrap_metadata};

    #[test]
    fn linear_ticks_are_nice_and_include_bounds() {
        let values = ticks(0.13, 9.87, ScaleMode::Linear, 5);
        assert_eq!(values.first().unwrap().label, "0.130");
        assert_eq!(values.last().unwrap().label, "9.870");
        assert!(values.iter().any(|tick| tick.value == 2.0));
    }

    #[test]
    fn logarithmic_ticks_follow_decades() {
        let values = ticks(1.0, 1000.0, ScaleMode::Log, 7);
        assert!(values.iter().any(|tick| tick.value == 10.0));
        assert!(values.iter().any(|tick| tick.value == 100.0));
        assert_eq!(format_tick(0.001, 1.0), "0.001");
    }

    #[test]
    fn metadata_wrap_preserves_long_names() {
        let lines = wrap_metadata("standard: very_long_scientific_variable_name", 16);
        assert_eq!(
            lines.concat(),
            "standard: very_long_scientific_variable_name"
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect::<String>()
        );
        assert!(lines.len() > 1);
    }
}
