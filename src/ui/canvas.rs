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
    let land_detail = landmask::detail_for_grid(slice.coordinates.as_ref());
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
                // coordinate/value, while the pinned point remains part of
                // the image and only changes on click.
                target_width,
                target_height,
                selected_point,
            )
            .into()
        }) {
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
            slice.coordinates.as_ref(),
            land_detail,
            &palette,
        )
    });
    let lines = (0..height)
        .map(|screen_row| {
            let source_row = screen_row * rows / height;
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
    selected_point: Option<(usize, usize)>,
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
    selected_point.hash(&mut hasher);
    hasher.finish()
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
