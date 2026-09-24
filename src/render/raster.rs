use image::{Rgb, RgbImage};
use rayon::prelude::*;

use crate::{
    analysis::projection::ProjectionIndex,
    data::slice::Slice2D,
    render::{
        colors::{Palette, ScaleMode, color_for_with_limits},
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
    let flip_rows = slice
        .coordinates
        .as_ref()
        .is_some_and(|grid| grid.latitude_increases_with_source_row());
    let mut image = RgbImage::new(cols as u32, rows as u32);
    let stats = slice.statistics.unwrap_or(crate::data::slice::Statistics {
        min: 0.0,
        max: 1.0,
        mean: 0.5,
        finite_count: 0,
    });
    let mapper = crate::render::colors::ColorMapper::new(
        &palette,
        stats,
        limits,
        ScaleMode::Linear,
        slice.is_diff,
    );
    if let (Some(v_slice), Some(m_slice)) = (slice.values.as_slice(), slice.validity.as_slice()) {
        let (chunks, _) = image.as_mut().as_chunks_mut::<3>();
        chunks
            .par_chunks_mut(cols)
            .enumerate()
            .for_each(|(display_row, row_pixels)| {
                let source_row =
                    source_row_for_display_row(slice, display_row, rows, rows, flip_rows);
                let offset = source_row * cols;
                for (chunk, (&value, &mask)) in row_pixels.iter_mut().zip(
                    v_slice[offset..offset + cols]
                        .iter()
                        .zip(m_slice[offset..offset + cols].iter()),
                ) {
                    let rgb = if mask == crate::data::slice::Validity::Finite && value.is_finite() {
                        mapper.map_value(value)
                    } else {
                        [80, 80, 80]
                    };
                    *chunk = rgb;
                }
            });
    } else {
        let mut pixels = image.pixels_mut();
        for display_row in 0..rows {
            let row = source_row_for_display_row(slice, display_row, rows, rows, flip_rows);
            for col in 0..cols {
                let rgb = color_for_with_limits(slice, row, col, &palette, limits);
                if let Some(pixel) = pixels.next() {
                    *pixel = Rgb(rgb);
                }
            }
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
    let flip_rows = slice
        .coordinates
        .as_ref()
        .is_some_and(|grid| grid.latitude_increases_with_source_row());
    let land_detail = landmask::detail_for_grid(slice.coordinates.as_ref());
    let output_rows = output_rows.max(1).min(rows.max(1));
    let output_cols = output_cols.max(1).min(cols.max(1));
    let statistics = slice.statistics.unwrap_or(crate::data::slice::Statistics {
        min: 0.0,
        max: 1.0,
        mean: 0.5,
        finite_count: 0,
    });
    let mapper =
        crate::render::colors::ColorMapper::new(&palette, statistics, limits, scale, slice.is_diff);
    let mut image = RgbImage::new(output_cols as u32, output_rows as u32);
    let background = show_land_borders.then(|| {
        map_background::render_with_palette_cached(
            output_cols,
            output_rows,
            slice.coordinates.as_ref(),
            land_detail,
            &palette,
        )
    });
    let v_slice = slice.values.as_slice();
    let m_slice = slice.validity.as_slice();
    let raw_buf = image.as_mut();

    let row_bins: Vec<(usize, usize)> = (0..output_rows)
        .map(|display_row| {
            let (start, end) = bin_range(display_row, rows, output_rows);
            if flip_rows {
                (rows - end, rows - start)
            } else {
                (start, end)
            }
        })
        .collect();
    let col_bins: Vec<(usize, usize)> = (0..output_cols)
        .map(|c| bin_range(c, cols, output_cols))
        .collect();

    let background_ref = background.as_ref();

    raw_buf
        .par_chunks_exact_mut(output_cols * 3)
        .enumerate()
        .for_each(|(output_row, row_bytes)| {
            let (row_start, row_end) = row_bins[output_row];
            let (row_chunks, _) = row_bytes.as_chunks_mut::<3>();
            for (output_col, chunk) in row_chunks.iter_mut().enumerate() {
                let (col_start, col_end) = col_bins[output_col];
                let (sum, count, filtered) = aggregate_bin(
                    slice, v_slice, m_slice, row_start, row_end, col_start, col_end, filter,
                );
                let background_rgb =
                    background_ref.map(|bg| bg.get_pixel(output_col as u32, output_row as u32).0);
                let mut rgb = if count == 0 {
                    background_rgb.unwrap_or(if filtered { [30, 30, 46] } else { [80, 80, 80] })
                } else {
                    mapper.map_value(sum / count as f64)
                };
                if let Some(background_rgb) = background_rgb
                    && count > 0
                {
                    // Keep geographic context visible beneath global fields while
                    // preserving the scientific color ordering of the data layer.
                    rgb = blend_rgb(background_rgb, rgb, 0.82);
                }
                *chunk = rgb;
            }
        });
    if let Some(point) = selected_point {
        mark_point(&mut image, slice, point, [255, 230, 160]);
    }
    if let Some(point) = hover_point {
        mark_point(&mut image, slice, point, [255, 255, 255]);
    }
    image
}

#[allow(clippy::too_many_arguments)]
fn aggregate_bin(
    slice: &Slice2D,
    values: Option<&[f64]>,
    validity: Option<&[crate::data::slice::Validity]>,
    row_start: usize,
    row_end: usize,
    col_start: usize,
    col_end: usize,
    filter: Option<(f64, f64)>,
) -> (f64, usize, bool) {
    let (_, cols) = slice.values.dim();
    let v_slice = values;
    let m_slice = validity;
    let mut sum = 0.0;
    let mut count = 0_usize;
    let mut filtered = false;

    if let (Some(v_s), Some(m_s)) = (v_slice, m_slice) {
        if let Some((f_min, f_max)) = filter {
            for row in row_start..row_end {
                let row_offset = row * cols;
                let v_sub = &v_s[row_offset + col_start..row_offset + col_end];
                let m_sub = &m_s[row_offset + col_start..row_offset + col_end];
                for (&value, &mask) in v_sub.iter().zip(m_sub.iter()) {
                    if mask == crate::data::slice::Validity::Finite && value.is_finite() {
                        if value < f_min || value > f_max {
                            filtered = true;
                        } else {
                            sum += value;
                            count += 1;
                        }
                    }
                }
            }
        } else {
            for row in row_start..row_end {
                let row_offset = row * cols;
                let v_sub = &v_s[row_offset + col_start..row_offset + col_end];
                let m_sub = &m_s[row_offset + col_start..row_offset + col_end];
                for (&value, &mask) in v_sub.iter().zip(m_sub.iter()) {
                    if mask == crate::data::slice::Validity::Finite && value.is_finite() {
                        sum += value;
                        count += 1;
                    }
                }
            }
        }
    } else if let Some((f_min, f_max)) = filter {
        for row in row_start..row_end {
            for col in col_start..col_end {
                let value = slice.values[(row, col)];
                if slice.validity[(row, col)] != crate::data::slice::Validity::Finite
                    || !value.is_finite()
                {
                    continue;
                }
                if value < f_min || value > f_max {
                    filtered = true;
                    continue;
                }
                sum += value;
                count += 1;
            }
        }
    } else {
        for row in row_start..row_end {
            for col in col_start..col_end {
                let value = slice.values[(row, col)];
                if slice.validity[(row, col)] != crate::data::slice::Validity::Finite
                    || !value.is_finite()
                {
                    continue;
                }
                sum += value;
                count += 1;
            }
        }
    }

    (sum, count, filtered)
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
    let flip_rows = slice
        .coordinates
        .as_ref()
        .is_some_and(|grid| grid.latitude_increases_with_source_row());
    let row = slice.coordinates.as_ref().map_or_else(
        || row.saturating_mul(image.height() as usize) / source_rows.max(1),
        |grid| {
            grid.display_row_for_source_row_with_flip(
                row,
                source_rows,
                image.height() as usize,
                flip_rows,
            )
        },
    );
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

fn source_row_for_display_row(
    slice: &Slice2D,
    display_row: usize,
    display_rows: usize,
    source_rows: usize,
    flip_rows: bool,
) -> usize {
    slice.coordinates.as_ref().map_or_else(
        || {
            let row = display_row * source_rows / display_rows.max(1);
            row.min(source_rows.saturating_sub(1))
        },
        |grid| {
            grid.source_row_for_display_row_with_flip(
                display_row,
                display_rows,
                source_rows,
                flip_rows,
            )
        },
    )
}

pub fn projected_lookup(
    rows: usize,
    cols: usize,
    index: &ProjectionIndex,
) -> Vec<Option<(usize, usize)>> {
    (0..rows)
        .into_par_iter()
        .flat_map(|row| {
            (0..cols).into_par_iter().map(move |col| {
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

    use super::{rgb_raster, rgb_raster_with_options_for_view};
    use crate::data::slice::{Bounds, CoordinateGrid, Slice2D, Validity};
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

    #[test]
    fn ascending_latitude_is_rendered_north_up() {
        let values = Array2::from_shape_vec((2, 1), vec![10.0, 20.0]).unwrap();
        let validity = Array2::from_elem((2, 1), Validity::Finite);
        let coordinates = CoordinateGrid {
            latitude: None,
            longitude: None,
            latitude_axis: Some(vec![-45.0, 45.0]),
            longitude_axis: Some(vec![0.0]),
        };
        let slice = Slice2D::new(values, validity, Bounds::new(0, 2, 0, 1).unwrap())
            .unwrap()
            .with_coordinates(coordinates);
        let image = rgb_raster(&slice, Palette::Viridis);

        assert_ne!(image.get_pixel(0, 0), image.get_pixel(0, 1));
        let descending_values = Array2::from_shape_vec((2, 1), vec![20.0, 10.0]).unwrap();
        let descending = Slice2D::new(
            descending_values,
            Array2::from_elem((2, 1), Validity::Finite),
            Bounds::new(0, 2, 0, 1).unwrap(),
        )
        .unwrap();
        let expected = rgb_raster(&descending, Palette::Viridis);
        assert_eq!(image, expected);

        let viewport = rgb_raster_with_options_for_view(
            &slice,
            Palette::Viridis,
            None,
            None,
            false,
            ScaleMode::Linear,
            2,
            1,
            None,
        );
        let expected_viewport = rgb_raster_with_options_for_view(
            &descending,
            Palette::Viridis,
            None,
            None,
            false,
            ScaleMode::Linear,
            2,
            1,
            None,
        );
        assert_eq!(viewport, expected_viewport);
    }
}
