use image::{DynamicImage, RgbImage};
use plotters::prelude::{
    BitMapBackend, ChartBuilder as PlotChartBuilder, Circle, Color, IntoDrawingArea, IntoFont,
    LineSeries, PathElement, RGBColor, Rectangle,
};
use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Axis, Bar, BarChart, Chart, Dataset, GraphType, LegendPosition, Paragraph},
};

use crate::app::{PlotKind, PlotSeries, PlotXAxis, PlotYAxis};
use crate::render::protocol::GraphicsRenderer;

use super::theme;

pub fn render(frame: &mut Frame, area: Rect, data: &[(f64, f64)], labels: &[String]) {
    let series = [PlotSeries {
        point: (0, 0),
        label: "selected point".into(),
        data: data.to_vec(),
        labels: labels.to_vec(),
    }];
    render_plot(
        frame,
        area,
        PlotKind::TimeSeries,
        PlotXAxis::ValidTime,
        PlotYAxis::Value,
        None,
        &series,
        &[],
        None,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn render_plot(
    frame: &mut Frame,
    area: Rect,
    kind: PlotKind,
    x_axis: PlotXAxis,
    y_axis: PlotYAxis,
    x_dimension_name: Option<&str>,
    series: &[PlotSeries],
    histogram_values: &[f64],
    graphics: Option<&mut GraphicsRenderer>,
) {
    if let Some(renderer) = graphics
        && renderer.supports_graphics()
        && let Some(image) = make_plot_image(
            kind,
            x_axis,
            y_axis,
            x_dimension_name,
            series,
            histogram_values,
            area,
        )
        && renderer.render(frame, area, image)
    {
        return;
    }
    match kind {
        PlotKind::Histogram => render_histogram(frame, area, y_axis, histogram_values),
        PlotKind::Cdf => render_cdf(frame, area, histogram_values),
        PlotKind::TimeSeries | PlotKind::Scatter | PlotKind::VerticalProfile => {
            render_series(frame, area, kind, x_axis, x_dimension_name, series)
        }
    }
}

/// Render the popup chart as a high-resolution bitmap before handing it to
/// ratatui-image. This keeps the chart legible on Kitty/Sixel terminals while
/// leaving the existing cell chart available as a portable fallback.
fn make_plot_image(
    kind: PlotKind,
    x_axis: PlotXAxis,
    y_axis: PlotYAxis,
    x_dimension_name: Option<&str>,
    series: &[PlotSeries],
    histogram_values: &[f64],
    area: Rect,
) -> Option<DynamicImage> {
    let width = u32::from(area.width).saturating_mul(14).clamp(480, 2200);
    let height = u32::from(area.height).saturating_mul(28).clamp(300, 1300);
    let mut buffer = vec![0_u8; width as usize * height as usize * 3];
    {
        let root = BitMapBackend::with_buffer(&mut buffer, (width, height)).into_drawing_area();
        root.fill(&RGBColor(18, 18, 26)).ok()?;
        match kind {
            PlotKind::Histogram => draw_histogram_image(&root, y_axis, histogram_values)?,
            PlotKind::Cdf => draw_cdf_image(&root, histogram_values)?,
            PlotKind::TimeSeries | PlotKind::Scatter | PlotKind::VerticalProfile => {
                draw_series_image(&root, kind, x_axis, x_dimension_name, series)?
            }
        }
        root.present().ok()?;
    }
    Some(DynamicImage::ImageRgb8(RgbImage::from_raw(
        width, height, buffer,
    )?))
}

fn draw_series_image(
    root: &plotters::drawing::DrawingArea<BitMapBackend<'_>, plotters::coord::Shift>,
    kind: PlotKind,
    x_axis: PlotXAxis,
    x_dimension_name: Option<&str>,
    series: &[PlotSeries],
) -> Option<()> {
    let finite_series = series
        .iter()
        .map(|item| {
            (
                item.label.as_str(),
                item.data
                    .iter()
                    .copied()
                    .filter(|(x, y)| x.is_finite() && y.is_finite())
                    .map(|(x, y)| {
                        if kind == PlotKind::VerticalProfile {
                            (y, x)
                        } else {
                            (x, y)
                        }
                    })
                    .collect::<Vec<_>>(),
                item.labels.as_slice(),
            )
        })
        .filter(|(_, data, _)| !data.is_empty())
        .collect::<Vec<_>>();
    let (raw_x_min, raw_x_max, raw_y_min, raw_y_max) = finite_series.iter().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |(x_min, x_max, y_min, y_max), (_, data, _)| {
            data.iter().fold(
                (x_min, x_max, y_min, y_max),
                |(x_min, x_max, y_min, y_max), (x, y)| {
                    (x_min.min(*x), x_max.max(*x), y_min.min(*y), y_max.max(*y))
                },
            )
        },
    );
    if !raw_x_min.is_finite() {
        return None;
    }
    let (x_min, x_max) = padded_bounds(raw_x_min, raw_x_max, 0.0);
    let (y_min, y_max) = padded_bounds(raw_y_min, raw_y_max, 0.05);
    let title = if kind == PlotKind::Scatter {
        "Scatter"
    } else if kind == PlotKind::VerticalProfile {
        "Vertical profile"
    } else if matches!(
        x_axis,
        PlotXAxis::Longitude | PlotXAxis::Latitude | PlotXAxis::Dimension(_)
    ) {
        "Cross section"
    } else {
        "Time series"
    };
    let x_title = match x_axis {
        PlotXAxis::ValidTime => "valid time",
        PlotXAxis::SampleIndex => "sample index",
        PlotXAxis::Longitude => "longitude",
        PlotXAxis::Latitude => "latitude",
        PlotXAxis::Dimension(_) if kind == PlotKind::VerticalProfile => "value",
        PlotXAxis::Dimension(_) => x_dimension_name.unwrap_or("dimension"),
        PlotXAxis::Value => "value",
    };
    let y_title = if kind == PlotKind::VerticalProfile {
        x_dimension_name.unwrap_or("level")
    } else {
        "value"
    };
    let text = RGBColor(205, 214, 230);
    let grid = RGBColor(57, 63, 78);
    let mut chart = PlotChartBuilder::on(root)
        .margin(24)
        .caption(title, ("sans-serif", 32).into_font().color(&text))
        .x_label_area_size(72)
        .y_label_area_size(104)
        .build_cartesian_2d(
            x_min..x_max,
            if kind == PlotKind::VerticalProfile {
                y_max..y_min
            } else {
                y_min..y_max
            },
        )
        .ok()?;
    let labels = finite_series
        .first()
        .map(|(_, _, labels)| *labels)
        .unwrap_or(&[]);
    let mut mesh = chart.configure_mesh();
    mesh.x_desc(x_title)
        .y_desc(y_title)
        .x_labels(5)
        .y_labels(5)
        .label_style(("sans-serif", 22).into_font().color(&text))
        .axis_desc_style(("sans-serif", 25).into_font().color(&text))
        .axis_style(text)
        .bold_line_style(grid)
        .light_line_style(RGBColor(35, 39, 51));
    let time_formatter = |value: &f64| {
        labels
            .get(value.round().max(0.0) as usize)
            .map(|label| axis_label(label))
            .unwrap_or_else(|| format!("{value:.0}"))
    };
    let numeric_formatter = |value: &f64| compact_value(*value);
    if x_axis == PlotXAxis::ValidTime && !labels.is_empty() {
        mesh.x_label_formatter(&time_formatter);
    } else {
        mesh.x_label_formatter(&numeric_formatter);
    }
    mesh.y_label_formatter(&numeric_formatter);
    mesh.draw().ok()?;

    let colors = [
        RGBColor(99, 110, 250),
        RGBColor(239, 85, 59),
        RGBColor(0, 204, 150),
        RGBColor(171, 99, 250),
        RGBColor(255, 161, 90),
    ];
    for (index, (label, data, _)) in finite_series.iter().enumerate() {
        let color = colors[index % colors.len()];
        if kind == PlotKind::Scatter {
            chart
                .draw_series(
                    data.iter()
                        .copied()
                        .map(|point| Circle::new(point, 5, color.filled())),
                )
                .ok()?
                .label(*label)
                .legend(move |(x, y)| {
                    PathElement::new(vec![(x, y), (x + 18, y)], color.stroke_width(2))
                });
        } else {
            chart
                .draw_series(LineSeries::new(data.iter().copied(), color.stroke_width(4)))
                .ok()?
                .label(*label)
                .legend(move |(x, y)| {
                    PathElement::new(vec![(x, y), (x + 24, y)], color.stroke_width(4))
                });
        }
    }
    if finite_series.len() > 1 {
        chart
            .configure_series_labels()
            .background_style(RGBColor(28, 28, 40))
            .border_style(RGBColor(90, 98, 120))
            .label_font(("sans-serif", 22).into_font().color(&text))
            .draw()
            .ok()?;
    }
    Some(())
}

fn draw_histogram_image(
    root: &plotters::drawing::DrawingArea<BitMapBackend<'_>, plotters::coord::Shift>,
    y_axis: PlotYAxis,
    values: &[f64],
) -> Option<()> {
    let finite = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    if finite.is_empty() {
        return None;
    }
    let min = finite.iter().copied().fold(f64::INFINITY, f64::min);
    let max = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = max - min;
    let bin_count = if span > 0.0 {
        ((finite.len() as f64).log2().ceil() as usize + 1).clamp(1, 12)
    } else {
        1
    };
    let width = if span > 0.0 {
        span / bin_count as f64
    } else {
        1.0
    };
    let mut counts = vec![0_u64; bin_count];
    for value in finite.iter().copied() {
        let bin = if span > 0.0 {
            (((value - min) / width).floor() as usize).min(bin_count - 1)
        } else {
            0
        };
        counts[bin] += 1;
    }
    let total = finite.len() as f64;
    let heights = counts
        .iter()
        .map(|count| {
            if y_axis == PlotYAxis::Density {
                (*count as f64 / total) * 100.0
            } else {
                *count as f64
            }
        })
        .collect::<Vec<_>>();
    let y_max = heights.iter().copied().fold(0.0, f64::max).max(1.0);
    let (x_min, x_max) = if span > 0.0 {
        (min, max)
    } else {
        (min - 0.5, min + 0.5)
    };
    let y_name = if y_axis == PlotYAxis::Density {
        "density (%)"
    } else {
        "frequency"
    };
    let text = RGBColor(205, 214, 230);
    let mut chart = PlotChartBuilder::on(root)
        .margin(24)
        .caption(
            format!("Histogram · value bins · {y_name} · n={}", finite.len()),
            ("sans-serif", 32).into_font().color(&text),
        )
        .x_label_area_size(72)
        .y_label_area_size(104)
        .build_cartesian_2d(x_min..x_max, 0.0..y_max * 1.05)
        .ok()?;
    chart
        .configure_mesh()
        .x_desc("value")
        .y_desc(y_name)
        .x_labels(bin_count.min(8))
        .y_labels(5)
        .label_style(("sans-serif", 22).into_font().color(&text))
        .axis_desc_style(("sans-serif", 25).into_font().color(&text))
        .axis_style(text)
        .bold_line_style(RGBColor(57, 63, 78))
        .light_line_style(RGBColor(35, 39, 51))
        .x_label_formatter(&|value| compact_value(*value))
        .y_label_formatter(&|value| compact_value(*value))
        .draw()
        .ok()?;
    chart
        .draw_series(counts.iter().enumerate().map(|(index, _)| {
            let left = if span > 0.0 {
                min + index as f64 * width
            } else {
                min - 0.5
            };
            let right = if span > 0.0 {
                if index + 1 == bin_count {
                    max
                } else {
                    left + width
                }
            } else {
                min + 0.5
            };
            Rectangle::new(
                [(left, 0.0), (right, heights[index])],
                RGBColor(0, 204, 150).filled(),
            )
        }))
        .ok()?;
    Some(())
}

fn draw_cdf_image(
    root: &plotters::drawing::DrawingArea<BitMapBackend<'_>, plotters::coord::Shift>,
    values: &[f64],
) -> Option<()> {
    let mut finite = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    if finite.is_empty() {
        return None;
    }
    finite.sort_by(f64::total_cmp);
    let (x_min, x_max) = padded_bounds(finite.first().copied()?, finite.last().copied()?, 0.05);
    let text = RGBColor(205, 214, 230);
    let mut chart = PlotChartBuilder::on(root)
        .margin(24)
        .caption(
            format!("Cumulative distribution · n={}", finite.len()),
            ("sans-serif", 32).into_font().color(&text),
        )
        .x_label_area_size(72)
        .y_label_area_size(104)
        .build_cartesian_2d(x_min..x_max, 0.0..100.0)
        .ok()?;
    chart
        .configure_mesh()
        .x_desc("value")
        .y_desc("cumulative (%)")
        .x_labels(6)
        .y_labels(5)
        .label_style(("sans-serif", 22).into_font().color(&text))
        .axis_desc_style(("sans-serif", 25).into_font().color(&text))
        .axis_style(text)
        .bold_line_style(RGBColor(57, 63, 78))
        .light_line_style(RGBColor(35, 39, 51))
        .x_label_formatter(&|value| compact_value(*value))
        .y_label_formatter(&|value| format!("{value:.0}"))
        .draw()
        .ok()?;
    let count = finite.len() as f64;
    chart
        .draw_series(LineSeries::new(
            finite
                .iter()
                .enumerate()
                .map(|(index, value)| (*value, (index + 1) as f64 * 100.0 / count)),
            RGBColor(99, 110, 250).stroke_width(4),
        ))
        .ok()?;
    Some(())
}

fn padded_bounds(min: f64, max: f64, padding_fraction: f64) -> (f64, f64) {
    if (max - min).abs() < f64::EPSILON {
        let padding = min.abs().max(1.0) * 0.05;
        (min - padding, max + padding)
    } else {
        let padding = (max - min) * padding_fraction;
        (min - padding, max + padding)
    }
}

fn render_series(
    frame: &mut Frame,
    area: Rect,
    kind: PlotKind,
    x_axis: PlotXAxis,
    x_dimension_name: Option<&str>,
    series: &[PlotSeries],
) {
    let is_cross_section = matches!(
        x_axis,
        PlotXAxis::Longitude | PlotXAxis::Latitude | PlotXAxis::Dimension(_)
    );
    let finite_series = series
        .iter()
        .map(|item| {
            (
                item.label.clone(),
                item.data
                    .iter()
                    .copied()
                    .filter(|(x, y)| x.is_finite() && y.is_finite())
                    .map(|(x, y)| {
                        if kind == PlotKind::VerticalProfile {
                            (y, x)
                        } else {
                            (x, y)
                        }
                    })
                    .collect::<Vec<_>>(),
                item.labels.clone(),
            )
        })
        .filter(|(_, data, _)| !data.is_empty())
        .collect::<Vec<_>>();
    if finite_series.is_empty() {
        let title = if kind == PlotKind::Scatter {
            "󰘦  Scatter"
        } else if is_cross_section {
            "󰛓  Cross section"
        } else {
            "󰈈  Time series"
        };
        frame.render_widget(
            Paragraph::new("No finite samples at the selected point(s).")
                .block(theme::panel(title, theme::MAUVE)),
            area,
        );
        return;
    }

    let (raw_x_min, raw_x_max, mut y_min, mut y_max) = finite_series.iter().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |(x_min, x_max, y_min, y_max), (_, data, _)| {
            data.iter().fold(
                (x_min, x_max, y_min, y_max),
                |(x_min, x_max, y_min, y_max), (x, y)| {
                    (x_min.min(*x), x_max.max(*x), y_min.min(*y), y_max.max(*y))
                },
            )
        },
    );
    let (x_min, x_max) = if (raw_x_max - raw_x_min).abs() < f64::EPSILON {
        (raw_x_min - 0.5, raw_x_max + 0.5)
    } else {
        (raw_x_min, raw_x_max)
    };
    if (y_max - y_min).abs() < f64::EPSILON {
        let padding = y_min.abs().max(1.0) * 0.05;
        y_min -= padding;
        y_max += padding;
    } else {
        let padding = (y_max - y_min) * 0.05;
        y_min -= padding;
        y_max += padding;
    }

    let colors = [
        theme::TEAL,
        theme::BLUE,
        theme::PEACH,
        theme::MAUVE,
        theme::RED,
    ];
    let graph_type = if kind == PlotKind::Scatter {
        GraphType::Scatter
    } else {
        GraphType::Line
    };
    let datasets = finite_series
        .iter()
        .enumerate()
        .map(|(index, (label, data, _))| {
            Dataset::default()
                .name(Line::from(label.clone()))
                .marker(if kind == PlotKind::Scatter {
                    ratatui::symbols::Marker::Dot
                } else {
                    ratatui::symbols::Marker::Braille
                })
                .graph_type(graph_type)
                .style(Style::default().fg(colors[index % colors.len()]))
                .data(data)
        })
        .collect::<Vec<_>>();
    let labels = finite_series
        .first()
        .map(|(_, _, labels)| labels)
        .cloned()
        .unwrap_or_default();
    let x_labels = if x_axis == PlotXAxis::ValidTime && labels.len() >= 2 {
        vec![
            Span::raw(axis_label(
                labels.first().map(String::as_str).unwrap_or_default(),
            )),
            Span::raw(axis_label(
                labels
                    .get(labels.len() / 2)
                    .map(String::as_str)
                    .unwrap_or_default(),
            )),
            Span::raw(axis_label(
                labels.last().map(String::as_str).unwrap_or_default(),
            )),
        ]
    } else {
        vec![
            Span::raw(format!("{raw_x_min:.0}")),
            Span::raw(format!("{raw_x_max:.0}")),
        ]
    };
    let x_title = match x_axis {
        PlotXAxis::ValidTime => "valid time",
        PlotXAxis::SampleIndex => "sample index",
        PlotXAxis::Longitude => "longitude",
        PlotXAxis::Latitude => "latitude",
        PlotXAxis::Dimension(_) if kind == PlotKind::VerticalProfile => "value",
        PlotXAxis::Dimension(_) => x_dimension_name.unwrap_or("dimension"),
        PlotXAxis::Value => "value",
    };
    let y_title = if kind == PlotKind::VerticalProfile {
        x_dimension_name.unwrap_or("level")
    } else {
        "value"
    };
    let title = if kind == PlotKind::Scatter {
        "󰘦  Scatter"
    } else if kind == PlotKind::VerticalProfile {
        "󰛓  Vertical profile"
    } else if is_cross_section {
        "󰛓  Cross section"
    } else {
        "󰈈  Time series"
    };
    let y_labels = vec![
        Span::raw(compact_value(y_min)),
        Span::raw(compact_value((y_min + y_max) * 0.5)),
        Span::raw(compact_value(y_max)),
    ];
    let mut chart = Chart::new(datasets)
        .block(theme::panel(title, theme::MAUVE))
        .x_axis(
            Axis::default()
                .title(x_title)
                .bounds([x_min, x_max])
                .labels(x_labels)
                .style(Style::default().fg(theme::SUBTEXT)),
        )
        .y_axis(
            Axis::default()
                .title(y_title)
                .bounds(if kind == PlotKind::VerticalProfile {
                    [y_max, y_min]
                } else {
                    [y_min, y_max]
                })
                .labels(y_labels)
                .style(Style::default().fg(theme::SUBTEXT)),
        );
    if finite_series.len() > 1 {
        chart = chart
            .legend_position(Some(LegendPosition::TopRight))
            .hidden_legend_constraints((Constraint::Percentage(100), Constraint::Percentage(100)));
    } else {
        chart = chart.legend_position(None);
    }
    frame.render_widget(chart, area);
}

fn render_cdf(frame: &mut Frame, area: Rect, values: &[f64]) {
    let mut finite = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    if finite.is_empty() {
        frame.render_widget(
            Paragraph::new("No finite samples available for a CDF.")
                .block(theme::panel("󰄰  CDF", theme::MAUVE)),
            area,
        );
        return;
    }
    finite.sort_by(f64::total_cmp);
    let count = finite.len() as f64;
    let data = finite
        .iter()
        .enumerate()
        .map(|(index, value)| (*value, (index + 1) as f64 * 100.0 / count))
        .collect::<Vec<_>>();
    let (raw_x_min, raw_x_max) = (
        finite.first().copied().unwrap_or_default(),
        finite.last().copied().unwrap_or_default(),
    );
    let (x_min, x_max) = if (raw_x_max - raw_x_min).abs() < f64::EPSILON {
        (raw_x_min - 0.5, raw_x_max + 0.5)
    } else {
        padded_bounds(raw_x_min, raw_x_max, 0.05)
    };
    let title = format!("󰄰  Cumulative distribution  n={}", finite.len());
    let dataset = Dataset::default()
        .name(Line::from("cumulative"))
        .marker(ratatui::symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(theme::BLUE))
        .data(&data);
    let chart = Chart::new(vec![dataset])
        .block(theme::panel(&title, theme::MAUVE))
        .x_axis(
            Axis::default()
                .title("value")
                .bounds([x_min, x_max])
                .labels(vec![
                    Span::raw(compact_value(raw_x_min)),
                    Span::raw(compact_value(raw_x_max)),
                ])
                .style(Style::default().fg(theme::SUBTEXT)),
        )
        .y_axis(
            Axis::default()
                .title("cumulative (%)")
                .bounds([0.0, 100.0])
                .labels(vec![Span::raw("0"), Span::raw("50"), Span::raw("100")])
                .style(Style::default().fg(theme::SUBTEXT)),
        );
    frame.render_widget(chart, area);
}

fn render_histogram(frame: &mut Frame, area: Rect, y_axis: PlotYAxis, values: &[f64]) {
    let finite = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    if finite.is_empty() {
        frame.render_widget(
            Paragraph::new("No finite samples available for a histogram.")
                .block(theme::panel("󰋼  Histogram", theme::MAUVE)),
            area,
        );
        return;
    }
    let min = finite.iter().copied().fold(f64::INFINITY, f64::min);
    let max = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = max - min;
    let bin_count = if span > 0.0 {
        // Sturges' rule keeps small samples readable without allowing a large
        // field to turn the terminal chart into a wall of narrow bars.
        ((finite.len() as f64).log2().ceil() as usize + 1).clamp(1, 12)
    } else {
        1
    };
    let width = if span > 0.0 {
        span / bin_count as f64
    } else {
        1.0
    };
    let mut counts = vec![0_u64; bin_count];
    for value in finite.iter().copied() {
        let bin = if span > 0.0 {
            (((value - min) / width).floor() as usize).min(bin_count - 1)
        } else {
            0
        };
        counts[bin] += 1;
    }
    let total = finite.len() as f64;
    let labels = counts
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let left = min + index as f64 * width;
            let right = if index + 1 == bin_count {
                max
            } else {
                left + width
            };
            format!("{}–{}", compact_value(left), compact_value(right))
        })
        .collect::<Vec<_>>();
    let heights = counts
        .iter()
        .copied()
        .map(|count| {
            if y_axis == PlotYAxis::Density {
                ((count as f64 / total) * 100.0).round() as u64
            } else {
                count
            }
        })
        .collect::<Vec<_>>();
    let max_value = heights.iter().copied().max().unwrap_or(1);
    let bars = labels
        .iter()
        .zip(heights)
        .map(|(label, height)| Bar::with_label(label.clone(), height))
        .collect::<Vec<_>>();
    let y_name = match y_axis {
        PlotYAxis::Density => "density (%)",
        _ => "frequency",
    };
    frame.render_widget(
        BarChart::new(bars)
            .block(theme::panel(
                &format!(
                    "󰋼  Histogram  x: value bins  y: {y_name}  n={}",
                    finite.len()
                ),
                theme::MAUVE,
            ))
            .bar_width(3)
            .bar_gap(1)
            .max(max_value.max(1))
            .bar_style(Style::default().fg(theme::TEAL))
            .value_style(Style::default().fg(theme::TEXT))
            .label_style(Style::default().fg(theme::SUBTEXT)),
        area,
    );
}

fn compact_value(value: f64) -> String {
    if value.abs() >= 1000.0 || (value != 0.0 && value.abs() < 0.01) {
        format!("{value:.2e}")
    } else {
        format!("{value:.3}")
    }
}

fn axis_label(label: &str) -> String {
    if label.len() <= 14 {
        label.to_owned()
    } else if let Some((date, time)) = label.split_once('T') {
        format!("{} {}", date, time.get(..5).unwrap_or(time))
    } else {
        label.chars().take(14).collect()
    }
}
