use image::{Rgb, RgbImage};

use crate::{
    analysis::projection::ProjectionIndex,
    data::slice::Slice2D,
    render::{
        colors::{
            Palette, ScaleMode, color_for_value_with_limits_and_filter_and_scale,
            color_for_with_limits,
        },
        landmask, map_background,
    },
};

pub fn rgb_raster(slice: &Slice2D, palette: Palette) -> RgbImage {
    rgb_raster_with_limits(slice, palette, None)
}

pub fn rgb_raster_with_limits(
    slice: &Slice2D,
    palette: Palette,
    limits: Option<(f64, f64)>,
) -> RgbImage {
    let (rows, cols) = slice.values.dim();
    let mut image = RgbImage::new(cols as u32, rows as u32);
    for row in 0..rows {
        for col in 0..cols {
            image.put_pixel(
                col as u32,
                row as u32,
                Rgb(color_for_with_limits(
                    slice,
                    row,
                    col,
                    palette.clone(),
                    limits,
                )),
            );
        }
    }
    image
}

#[allow(clippy::too_many_arguments)]
pub fn rgb_raster_with_options(
    slice: &Slice2D,
    palette: Palette,
    limits: Option<(f64, f64)>,
    filter: Option<(f64, f64)>,
    show_land_borders: bool,
    scale: ScaleMode,
    hover_point: Option<(usize, usize)>,
    selected_point: Option<(usize, usize)>,
) -> RgbImage {
    let (rows, cols) = slice.values.dim();
    rasterize(
        slice,
        palette,
        limits,
        filter,
        show_land_borders,
        scale,
        rows,
        cols,
        hover_point,
        selected_point,
    )
}

/// Rasterize a slice to a bounded viewport canvas. If the source is larger
/// than the target, each output pixel aggregates all source cells that fall
/// into its bin. This is the Datashader-style step that prevents a huge source
/// array from being blurred by a final image resize.
#[allow(clippy::too_many_arguments)]
pub fn rgb_raster_with_options_for_view(
    slice: &Slice2D,
    palette: Palette,
    limits: Option<(f64, f64)>,
    filter: Option<(f64, f64)>,
    show_land_borders: bool,
    scale: ScaleMode,
    target_width: usize,
    target_height: usize,
    selected_point: Option<(usize, usize)>,
) -> RgbImage {
    let (rows, cols) = slice.values.dim();
    rasterize(
        slice,
        palette,
        limits,
        filter,
        show_land_borders,
        scale,
        rows.min(target_height.max(1)),
        cols.min(target_width.max(1)),
        None,
        selected_point,
    )
}

#[allow(clippy::too_many_arguments)]
fn rasterize(
    slice: &Slice2D,
    palette: Palette,
    limits: Option<(f64, f64)>,
    filter: Option<(f64, f64)>,
    show_land_borders: bool,
    scale: ScaleMode,
    output_rows: usize,
    output_cols: usize,
    hover_point: Option<(usize, usize)>,
    selected_point: Option<(usize, usize)>,
) -> RgbImage {
    let (rows, cols) = slice.values.dim();
    let land_detail = landmask::detail_for_grid(slice.coordinates.as_ref());
    let output_rows = output_rows.max(1).min(rows.max(1));
    let output_cols = output_cols.max(1).min(cols.max(1));
    let statistics = slice.statistics.unwrap_or(crate::data::slice::Statistics {
        min: 0.0,
        max: 1.0,
        mean: 0.5,
        finite_count: 0,
    });
    let mut image = RgbImage::new(output_cols as u32, output_rows as u32);
    let background = show_land_borders.then(|| {
        map_background::render_with_palette(
            output_cols,
            output_rows,
            slice.coordinates.as_ref(),
            land_detail,
            &palette,
        )
    });
    for output_row in 0..output_rows {
        let (row_start, row_end) = bin_range(output_row, rows, output_rows);
        for output_col in 0..output_cols {
            let (col_start, col_end) = bin_range(output_col, cols, output_cols);
            let mut sum = 0.0;
            let mut count = 0_usize;
            let mut filtered = false;
            for row in row_start..row_end {
                for col in col_start..col_end {
                    let value = slice.values[(row, col)];
                    if slice.validity[(row, col)] != crate::data::slice::Validity::Finite
                        || !value.is_finite()
                    {
                        continue;
                    }
                    if filter.is_some_and(|(min, max)| value < min || value > max) {
                        filtered = true;
                        continue;
                    }
                    sum += value;
                    count += 1;
                }
            }
            let background_rgb = background
                .as_ref()
                .map(|background| background.get_pixel(output_col as u32, output_row as u32).0);
            let mut rgb = if count == 0 {
                background_rgb.unwrap_or(if filtered { [30, 30, 46] } else { [80, 80, 80] })
            } else {
                color_for_value_with_limits_and_filter_and_scale(
                    sum / count as f64,
                    crate::data::slice::Validity::Finite,
                    statistics,
                    palette.clone(),
                    limits,
                    None,
                    scale,
                )
            };
            if let Some(background_rgb) = background_rgb
                && count > 0
            {
                // Keep geographic context visible beneath global fields while
                // preserving the scientific color ordering of the data layer.
                rgb = blend_rgb(background_rgb, rgb, 0.82);
            }
            image.put_pixel(output_col as u32, output_row as u32, Rgb(rgb));
        }
    }
    if let Some(point) = selected_point {
        mark_point(&mut image, slice, point, [255, 230, 160]);
    }
    if let Some(point) = hover_point {
        mark_point(&mut image, slice, point, [255, 255, 255]);
    }
    image
}

pub fn blend_rgb(background: [u8; 3], foreground: [u8; 3], opacity: f32) -> [u8; 3] {
    let opacity = opacity.clamp(0.0, 1.0);
    std::array::from_fn(|index| {
        (background[index] as f32 * (1.0 - opacity) + foreground[index] as f32 * opacity).round()
            as u8
    })
}

fn bin_range(index: usize, source_len: usize, output_len: usize) -> (usize, usize) {
    let start = index * source_len / output_len;
    let end = ((index + 1) * source_len / output_len).max(start + 1);
    (start, end.min(source_len))
}

fn mark_point(image: &mut RgbImage, slice: &Slice2D, point: (usize, usize), color: [u8; 3]) {
    let Some(row) = point.0.checked_sub(slice.source_bounds.row_start) else {
        return;
    };
    let Some(col) = point.1.checked_sub(slice.source_bounds.col_start) else {
        return;
    };
    let (source_rows, source_cols) = slice.values.dim();
    let row = row.saturating_mul(image.height() as usize) / source_rows.max(1);
    let col = col.saturating_mul(image.width() as usize) / source_cols.max(1);
    if row >= image.height() as usize || col >= image.width() as usize {
        return;
    }
    let radius = 1_isize;
    for delta_row in -radius..=radius {
        for delta_col in -radius..=radius {
            let Some(mark_row) = row.checked_add_signed(delta_row) else {
                continue;
            };
            let Some(mark_col) = col.checked_add_signed(delta_col) else {
                continue;
            };
            if mark_row < image.height() as usize && mark_col < image.width() as usize {
                image.put_pixel(mark_col as u32, mark_row as u32, Rgb(color));
            }
        }
    }
}

pub fn projected_lookup(
    rows: usize,
    cols: usize,
    index: &ProjectionIndex,
) -> Vec<Option<(usize, usize)>> {
    (0..rows)
        .flat_map(|row| {
            (0..cols).map(move |col| {
                let latitude = -90.0 + 180.0 * row as f64 / rows.max(1) as f64;
                let longitude = -180.0 + 360.0 * col as f64 / cols.max(1) as f64;
                index
                    .nearest(latitude, longitude)
                    .map(|source| (source.row, source.col))
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use ndarray::Array2;

    use super::rgb_raster_with_options_for_view;
    use crate::data::slice::{Bounds, Slice2D, Validity};
    use crate::render::colors::{Palette, ScaleMode};

    #[test]
    fn viewport_raster_aggregates_to_display_canvas() {
        let values = Array2::from_shape_fn((4, 4), |(row, col)| (row * 4 + col) as f64);
        let validity = Array2::from_elem((4, 4), Validity::Finite);
        let slice = Slice2D::new(values, validity, Bounds::new(0, 4, 0, 4).unwrap()).unwrap();
        let image = rgb_raster_with_options_for_view(
            &slice,
            Palette::Viridis,
            None,
            None,
            false,
            ScaleMode::Linear,
            2,
            2,
            None,
        );
        assert_eq!(image.dimensions(), (2, 2));
    }
}
