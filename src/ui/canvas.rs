use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    data::slice::Slice2D,
    render::landmask,
    render::{
        colors::{Palette, ScaleMode, color_for_with_limits_and_filter_and_scale},
        protocol::GraphicsRenderer,
        raster::rgb_raster_with_options_for_view,
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
    graphics: Option<&mut GraphicsRenderer>,
) {
    let message = if constrained {
        Some(Paragraph::new("terminal too small; resize to view"))
    } else if loading {
        Some(Paragraph::new("loading slice…"))
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
    if let Some(message) = message {
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
        let image = rgb_raster_with_options_for_view(
            slice,
            palette.clone(),
            limits,
            filter,
            show_land_borders,
            scale,
            // Keep the transient hover cursor out of the protocol image.
            // iTerm2 encodes the complete PNG whenever the image bytes
            // change; embedding the hover point therefore retransmitted the
            // whole map for every mouse-motion event. The status bar still
            // reports the exact hovered coordinate/value, while the pinned
            // point remains part of the image and only changes on click.
            target_width,
            target_height,
            selected_point,
        );
        if graphics.render(frame, inner, image.into()) {
            return;
        }
    }
    let width = usize::from(inner.width).min(cols);
    let height = usize::from(inner.height).min(rows);
    let lines = (0..height)
        .map(|screen_row| {
            let source_row = screen_row * rows / height;
            let spans = (0..width)
                .map(|screen_col| {
                    let source_col = screen_col * cols / width;
                    let rgb = color_for_with_limits_and_filter_and_scale(
                        slice,
                        source_row,
                        source_col,
                        palette.clone(),
                        limits,
                        filter,
                        scale,
                    );
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
}
