use std::hash::{Hash, Hasher};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::{
    data::slice::Slice2D,
    events::mouse::DragState,
    render::landmask,
    render::{
        colors::{Palette, ScaleMode, color_for_with_limits_and_filter_and_scale},
        map_background,
        protocol::GraphicsRenderer,
        raster::{blend_rgb, rgb_raster_with_options_for_view},
    },
};

use super::theme;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    loading: bool,
    constrained: bool,
    slice: Option<&Slice2D>,
    palette: Palette,
) {
    render_with_options(
        frame,
        area,
        loading,
        constrained,
        slice,
        palette,
        None,
        None,
        true,
        ScaleMode::Linear,
    );
}

pub fn render_with_limits(
    frame: &mut Frame,
    area: Rect,
    loading: bool,
    constrained: bool,
    slice: Option<&Slice2D>,
    palette: Palette,
    limits: Option<(f64, f64)>,
) {
    render_with_options(
        frame,
        area,
        loading,
        constrained,
        slice,
        palette,
        limits,
        None,
        true,
        ScaleMode::Linear,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn render_with_options(
    frame: &mut Frame,
    area: Rect,
    loading: bool,
    constrained: bool,
    slice: Option<&Slice2D>,
    palette: Palette,
    limits: Option<(f64, f64)>,
    filter: Option<(f64, f64)>,
    show_land_borders: bool,
    scale: ScaleMode,
) {
    render_with_points(
        frame,
        area,
        loading,
        constrained,
        slice,
        palette,
        limits,
        filter,
        show_land_borders,
        scale,
        None,
        None,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn render_with_points_and_image(
    frame: &mut Frame,
    area: Rect,
    loading: bool,
    constrained: bool,
    slice: Option<&Slice2D>,
    palette: Palette,
    limits: Option<(f64, f64)>,
    filter: Option<(f64, f64)>,
    show_land_borders: bool,
    scale: ScaleMode,
    hover_point: Option<(usize, usize)>,
    selected_point: Option<(usize, usize)>,
    drag: Option<DragState>,
    zoom_active: bool,
    render_generation: u64,
    graphics: Option<&mut GraphicsRenderer>,
) {
    render_content(
        frame,
        area,
        loading,
        constrained,
        slice,
        palette,
        limits,
        filter,
        show_land_borders,
        scale,
        hover_point,
        selected_point,
        drag,
        zoom_active,
        render_generation,
        graphics,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn render_with_points(
    frame: &mut Frame,
    area: Rect,
    loading: bool,
    constrained: bool,
    slice: Option<&Slice2D>,
    palette: Palette,
    limits: Option<(f64, f64)>,
    filter: Option<(f64, f64)>,
    show_land_borders: bool,
    scale: ScaleMode,
    hover_point: Option<(usize, usize)>,
    selected_point: Option<(usize, usize)>,
) {
    render_content(
        frame,
        area,
        loading,
        constrained,
        slice,
        palette,
        limits,
        filter,
        show_land_borders,
        scale,
        hover_point,
        selected_point,
        None,
        false,
        0,
        None,
    );
}

#[allow(clippy::too_many_arguments)]
fn render_content(
    frame: &mut Frame,
    area: Rect,
    loading: bool,
    constrained: bool,
    slice: Option<&Slice2D>,
    palette: Palette,
    limits: Option<(f64, f64)>,
    filter: Option<(f64, f64)>,
    show_land_borders: bool,
    scale: ScaleMode,
    hover_point: Option<(usize, usize)>,
    selected_point: Option<(usize, usize)>,
    drag: Option<DragState>,
    zoom_active: bool,
    render_generation: u64,
    graphics: Option<&mut GraphicsRenderer>,
) {
    let message = if constrained {
        Some(Paragraph::new("terminal too small; resize to view"))
    } else if slice.is_none() {
        Some(Paragraph::new("no plottable slice selected"))
    } else {
        None
    };
    let graphics_label = graphics
        .as_ref()
        .map(|renderer| renderer.mode_label())
        .unwrap_or("cell fallback");
    let filter_label = graphics
        .as_ref()
        .map(|renderer| renderer.filter_label())
        .unwrap_or("cells");
    let title = format!("󰉢  Map  •  {graphics_label}  •  {filter_label}");
    let block = theme::panel(&title, theme::TEAL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if constrained || slice.is_none() {
        let Some(message) = message else { return };
        frame.render_widget(message, inner);
        return;
    }
    let Some(slice) = slice else { return };
    let (rows, cols) = slice.values.dim();
    let land_detail = landmask::detail_for_grid(slice.coordinates.as_deref());
    if inner.width == 0 || inner.height == 0 || rows == 0 || cols == 0 {
        return;
    }
    if let Some(graphics) = graphics
        && graphics.supports_graphics()
    {
        // Approximate the terminal's graphics pixel canvas (roughly 4x8
        // addressable pixels per cell). The rasterizer aggregates source
        // cells into this bounded canvas before the protocol encoder runs.
        let target_width = usize::from(inner.width).saturating_mul(4).max(1);
        let target_height = usize::from(inner.height).saturating_mul(8).max(1);
        let image_palette = palette.clone();
        let render_key = map_render_key(
            render_generation,
            inner,
            rows,
            cols,
            &palette,
            limits,
            filter,
            show_land_borders,
            scale,
            selected_point,
        );
        let image_area = graphics.drawable_area(inner);
        if graphics.render_with_key(frame, inner, render_key, || {
            rgb_raster_with_options_for_view(
                slice,
                image_palette,
                limits,
                filter,
                show_land_borders,
                scale,
                // Keep the transient hover cursor out of the protocol image.
                // The status bar still reports the exact hovered
                // coordinate/value. The pinned point is a terminal-cell
                // overlay so selection changes do not rebuild the raster.
                target_width,
                target_height,
                None,
            )
            .into()
        }) {
            render_selected_marker(frame, image_area, slice, selected_point);
            render_drag_box(frame, image_area, drag, zoom_active);
            render_loading_badge(frame, inner, loading);
            return;
        }
    }
    let width = usize::from(inner.width).min(cols);
    let height = usize::from(inner.height).min(rows);
    let background = show_land_borders.then(|| {
        map_background::render_with_palette_cached(
            width,
            height,
            slice.coordinates.as_deref(),
            land_detail,
            &palette,
        )
    });
    let flip_rows = slice
        .coordinates
        .as_ref()
        .is_some_and(|grid| grid.latitude_increases_with_source_row());
    let lines = (0..height)
        .map(|screen_row| {
            let source_row = slice.coordinates.as_ref().map_or_else(
                || screen_row * rows / height,
                |grid| {
                    grid.source_row_for_display_row_with_flip(screen_row, height, rows, flip_rows)
                },
            );
            let spans = (0..width)
                .map(|screen_col| {
                    let source_col = screen_col * cols / width;
                    let data_rgb = color_for_with_limits_and_filter_and_scale(
                        slice, source_row, source_col, &palette, limits, filter, scale,
                    );
                    let background_rgb = background.as_ref().map(|background| {
                        background.get_pixel(screen_col as u32, screen_row as u32).0
                    });
                    let rgb = if let Some(background_rgb) = background_rgb {
                        if slice.validity[(source_row, source_col)]
                            == crate::data::slice::Validity::Finite
                        {
                            blend_rgb(background_rgb, data_rgb, 0.82)
                        } else {
                            background_rgb
                        }
                    } else {
                        data_rgb
                    };
                    let border = show_land_borders
                        && slice
                            .coordinates
                            .as_ref()
                            .and_then(|grid| {
                                landmask::cell_is_border_grid_with_detail(
                                    grid,
                                    source_row,
                                    source_col,
                                    land_detail,
                                )
                            })
                            .unwrap_or_else(|| {
                                landmask::cell_is_border_with_detail(
                                    rows,
                                    cols,
                                    source_row,
                                    source_col,
                                    land_detail,
                                )
                            });
                    let selected = selected_point
                        == Some((
                            slice.source_bounds.row_start + source_row.min(rows.saturating_sub(1)),
                            slice.source_bounds.col_start + source_col.min(cols.saturating_sub(1)),
                        ));
                    let hovered = hover_point
                        == Some((
                            slice.source_bounds.row_start + source_row.min(rows.saturating_sub(1)),
                            slice.source_bounds.col_start + source_col.min(cols.saturating_sub(1)),
                        ));
                    Span::styled(
                        if selected {
                            "◆"
                        } else if hovered {
                            "×"
                        } else if border {
                            "•"
                        } else {
                            " "
                        },
                        Style::default()
                            .fg(if selected {
                                Color::Rgb(255, 230, 160)
                            } else if hovered {
                                Color::Rgb(255, 255, 255)
                            } else if border {
                                Color::Rgb(0, 0, 0)
                            } else {
                                Color::Reset
                            })
                            .bg(Color::Rgb(rgb[0], rgb[1], rgb[2])),
                    )
                })
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), inner);
    render_drag_box(frame, inner, drag, zoom_active);
    render_loading_badge(frame, inner, loading);
}

#[allow(clippy::too_many_arguments)]
fn map_render_key(
    generation: u64,
    area: Rect,
    rows: usize,
    cols: usize,
    palette: &Palette,
    limits: Option<(f64, f64)>,
    filter: Option<(f64, f64)>,
    show_land_borders: bool,
    scale: ScaleMode,
    _selected_point: Option<(usize, usize)>,
) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    generation.hash(&mut hasher);
    area.width.hash(&mut hasher);
    area.height.hash(&mut hasher);
    rows.hash(&mut hasher);
    cols.hash(&mut hasher);
    palette.hash(&mut hasher);
    hash_limits(&mut hasher, limits);
    hash_limits(&mut hasher, filter);
    show_land_borders.hash(&mut hasher);
    scale.hash(&mut hasher);
    hasher.finish()
}

fn render_selected_marker(
    frame: &mut Frame,
    area: Rect,
    slice: &Slice2D,
    selected_point: Option<(usize, usize)>,
) {
    let Some((selected_row, selected_col)) = selected_point else {
        return;
    };
    let Some(row) = selected_row.checked_sub(slice.source_bounds.row_start) else {
        return;
    };
    let Some(col) = selected_col.checked_sub(slice.source_bounds.col_start) else {
        return;
    };
    let (rows, cols) = slice.values.dim();
    if row >= rows || col >= cols || rows == 0 || cols == 0 || area.is_empty() {
        return;
    }
    let flip_rows = slice
        .coordinates
        .as_ref()
        .is_some_and(|grid| grid.latitude_increases_with_source_row());
    let display_row = slice.coordinates.as_ref().map_or_else(
        || row.saturating_mul(usize::from(area.height)) / rows,
        |grid| {
            grid.display_row_for_source_row_with_flip(
                row,
                rows,
                usize::from(area.height),
                flip_rows,
            )
        },
    );
    let display_col = col.saturating_mul(usize::from(area.width)) / cols;
    if display_row >= usize::from(area.height) || display_col >= usize::from(area.width) {
        return;
    }
    let marker_area = Rect::new(
        area.x.saturating_add(display_col as u16),
        area.y.saturating_add(display_row as u16),
        1,
        1,
    );
    frame.render_widget(
        Paragraph::new("◆").style(Style::default().fg(Color::Rgb(255, 230, 160))),
        marker_area,
    );
}

fn hash_limits(hasher: &mut impl Hasher, limits: Option<(f64, f64)>) {
    limits
        .map(|(min, max)| (min.to_bits(), max.to_bits()))
        .hash(hasher);
}

fn render_loading_badge(frame: &mut Frame, area: Rect, loading: bool) {
    if !loading || area.width < 10 || area.height == 0 {
        return;
    }
    let badge = Rect::new(area.x.saturating_add(1), area.y, area.width.min(22), 1);
    frame.render_widget(
        Paragraph::new(" loading next frame… ").style(theme::muted_style()),
        badge,
    );
}

fn render_drag_box(frame: &mut Frame, area: Rect, drag: Option<DragState>, zoom_active: bool) {
    let Some(drag) = drag else { return };
    if zoom_active && !drag.zoom {
        return;
    }
    if drag.start == drag.current || area.width == 0 || area.height == 0 {
        return;
    }
    let x0 = drag
        .start
        .0
        .min(drag.current.0)
        .clamp(area.x, area.right() - 1);
    let x1 = drag
        .start
        .0
        .max(drag.current.0)
        .clamp(area.x, area.right() - 1);
    let y0 = drag
        .start
        .1
        .min(drag.current.1)
        .clamp(area.y, area.bottom() - 1);
    let y1 = drag
        .start
        .1
        .max(drag.current.1)
        .clamp(area.y, area.bottom() - 1);
    let rect = Rect::new(x0, y0, x1 - x0 + 1, y1 - y0 + 1);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .border_style(Style::default().fg(Color::Rgb(255, 230, 160))),
        rect,
    );
}

#[cfg(test)]
mod tests {
    use super::{map_render_key, render_selected_marker};
    use crate::data::fixtures::regular_values;
    use crate::render::colors::{Palette, ScaleMode};
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};

    #[test]
    fn selected_point_does_not_invalidate_base_raster_key() {
        let key = |selected_point| {
            map_render_key(
                7,
                Rect::new(2, 3, 80, 40),
                1024,
                2048,
                &Palette::Viridis,
                None,
                None,
                false,
                ScaleMode::Linear,
                selected_point,
            )
        };

        assert_eq!(key(None), key(Some((12, 34))));
        assert_eq!(key(Some((12, 34))), key(Some((15, 34))));
    }

    #[test]
    fn selected_point_overlay_maps_source_cell_to_graphics_cell() {
        let slice = regular_values(8, 8).expect("test dimensions are valid");
        let mut terminal = Terminal::new(TestBackend::new(10, 10)).expect("terminal builds");
        terminal
            .draw(|frame| {
                render_selected_marker(frame, Rect::new(1, 1, 4, 4), &slice, Some((4, 6)));
            })
            .expect("marker renders");

        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(4, 3)].symbol(), "◆");
    }
}
