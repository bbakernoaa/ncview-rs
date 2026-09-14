//! D3-inspired geographic backdrop for the scientific raster.
//!
//! This is intentionally a presentation layer. It never classifies or masks
//! data; the NetCDF validity mask remains authoritative. The backdrop uses
//! the same Natural Earth rings as the coastline overlay, but fills them and
//! adds subtle graticules before the data raster is composited over the top.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, OnceLock},
};

use image::RgbImage;
use rayon::prelude::*;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

use crate::data::slice::CoordinateGrid;

use super::{
    colors::{MapOverlayColors, Palette},
    landmask::{self, Detail},
};

const DEFAULT_PALETTE: Palette = Palette::Viridis;

/// Render a D3-like base map for the geographic extent represented by a slice.
/// The output uses source-row orientation so it can be composited directly
/// beneath the aggregated data raster.
pub fn render(
    width: usize,
    height: usize,
    coordinates: Option<&CoordinateGrid>,
    detail: Detail,
) -> RgbImage {
    render_with_palette(width, height, coordinates, detail, &DEFAULT_PALETTE)
}

pub fn render_with_palette(
    width: usize,
    height: usize,
    coordinates: Option<&CoordinateGrid>,
    detail: Detail,
    palette: &Palette,
) -> RgbImage {
    render_with_palette_cached(width, height, coordinates, detail, palette)
        .as_ref()
        .clone()
}

pub fn render_with_palette_cached(
    width: usize,
    height: usize,
    coordinates: Option<&CoordinateGrid>,
    detail: Detail,
    palette: &Palette,
) -> Arc<RgbImage> {
    let width = width.max(1);
    let height = height.max(1);
    let colors = palette.map_overlay_colors();
    let extent = extent(coordinates);
    let key = BackdropKey::new(width, height, detail, colors, extent);
    if let Ok(mut cache) = backdrop_cache().lock()
        && let Some(position) = cache.iter().position(|(candidate, _)| *candidate == key)
    {
        let (_, image) = cache
            .remove(position)
            .expect("backdrop cache position exists");
        cache.push_back((key, Arc::clone(&image)));
        return image;
    }

    let mut pixmap = Pixmap::new(width as u32, height as u32)
        .expect("non-zero map backdrop dimensions should allocate");
    pixmap.fill(Color::from_rgba8(
        colors.ocean[0],
        colors.ocean[1],
        colors.ocean[2],
        255,
    ));

    draw_graticule(&mut pixmap, extent, colors);
    for polygon in landmask::polygons_for_detail(detail) {
        if polygon_intersects_extent(polygon, extent) {
            draw_polygon(&mut pixmap, polygon, extent, colors);
        }
    }

    let bytes = pixmap.data();
    let mut image = RgbImage::new(width as u32, height as u32);
    let (src_chunks, _) = bytes.as_chunks::<4>();
    let (dst_chunks, _) = image.as_mut().as_chunks_mut::<3>();
    dst_chunks
        .par_iter_mut()
        .zip(src_chunks.par_iter())
        .for_each(|(dst, src)| {
            *dst = [src[0], src[1], src[2]];
        });
    let image = Arc::new(image);
    if let Ok(mut cache) = backdrop_cache().lock() {
        cache.push_back((key, Arc::clone(&image)));
        while cache.len() > 8 {
            cache.pop_front();
        }
    }
    image
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExtentKey {
    min_lon: u64,
    max_lon: u64,
    min_lat: u64,
    max_lat: u64,
    lat_increases_down: bool,
    zero_to_360: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BackdropKey {
    width: usize,
    height: usize,
    detail: Detail,
    colors: super::colors::MapOverlayColors,
    extent: ExtentKey,
}

type BackdropCache = VecDeque<(BackdropKey, Arc<RgbImage>)>;

impl BackdropKey {
    fn new(
        width: usize,
        height: usize,
        detail: Detail,
        colors: super::colors::MapOverlayColors,
        extent: Extent,
    ) -> Self {
        Self {
            width,
            height,
            detail,
            colors,
            extent: ExtentKey {
                min_lon: extent.min_lon.to_bits(),
                max_lon: extent.max_lon.to_bits(),
                min_lat: extent.min_lat.to_bits(),
                max_lat: extent.max_lat.to_bits(),
                lat_increases_down: extent.lat_increases_down,
                zero_to_360: extent.zero_to_360,
            },
        }
    }
}

fn backdrop_cache() -> &'static Mutex<BackdropCache> {
    static CACHE: OnceLock<Mutex<BackdropCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(VecDeque::with_capacity(8)))
}

#[derive(Debug, Clone, Copy)]
struct Extent {
    min_lon: f64,
    max_lon: f64,
    min_lat: f64,
    max_lat: f64,
    lat_increases_down: bool,
    zero_to_360: bool,
}

fn extent(coordinates: Option<&CoordinateGrid>) -> Extent {
    let Some(grid) = coordinates else {
        return Extent {
            min_lon: -180.0,
            max_lon: 180.0,
            min_lat: -90.0,
            max_lat: 90.0,
            lat_increases_down: false,
            zero_to_360: false,
        };
    };
    let mut min_lon = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    let mut min_lat = f64::INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    let mut first_lat = None;
    let mut last_lat = None;
    if let Some(latitude) = grid.latitude.as_ref() {
        for value in latitude.iter().copied().filter(|value| value.is_finite()) {
            min_lat = min_lat.min(value);
            max_lat = max_lat.max(value);
        }
        let (_, cols) = latitude.dim();
        first_lat = latitude
            .get((0, cols / 2))
            .copied()
            .filter(|v| v.is_finite());
        let (rows, _) = latitude.dim();
        last_lat = rows
            .checked_sub(1)
            .and_then(|row| latitude.get((row, cols / 2)))
            .copied()
            .filter(|v| v.is_finite());
    } else if let Some(latitude) = grid.latitude_axis.as_ref() {
        for value in latitude.iter().copied().filter(|value| value.is_finite()) {
            min_lat = min_lat.min(value);
            max_lat = max_lat.max(value);
        }
        first_lat = latitude.first().copied().filter(|v| v.is_finite());
        last_lat = latitude.last().copied().filter(|v| v.is_finite());
    }
    if let Some(longitude) = grid.longitude.as_ref() {
        for value in longitude.iter().copied().filter(|value| value.is_finite()) {
            min_lon = min_lon.min(value);
            max_lon = max_lon.max(value);
        }
    } else if let Some(longitude) = grid.longitude_axis.as_ref() {
        for value in longitude.iter().copied().filter(|value| value.is_finite()) {
            min_lon = min_lon.min(value);
            max_lon = max_lon.max(value);
        }
    }
    if !min_lon.is_finite()
        || !max_lon.is_finite()
        || !min_lat.is_finite()
        || !max_lat.is_finite()
        || (max_lon - min_lon).abs() < f64::EPSILON
        || (max_lat - min_lat).abs() < f64::EPSILON
    {
        return Extent {
            min_lon: -180.0,
            max_lon: 180.0,
            min_lat: -90.0,
            max_lat: 90.0,
            lat_increases_down: false,
            zero_to_360: false,
        };
    }
    let zero_to_360 = min_lon >= -1.0e-6 && max_lon > 180.0 + 1.0e-6;
    let span = (max_lon - min_lon).abs();
    if span >= 350.0 {
        if zero_to_360 {
            min_lon = 0.0;
            max_lon = 360.0;
        } else {
            min_lon = -180.0;
            max_lon = 180.0;
        }
    }
    Extent {
        min_lon,
        max_lon,
        min_lat,
        max_lat,
        lat_increases_down: first_lat
            .zip(last_lat)
            .is_some_and(|(first, last)| last > first),
        zero_to_360,
    }
}

fn project(lon: f64, lat: f64, extent: Extent, width: f32, height: f32) -> (f32, f32) {
    let lon_span = (extent.max_lon - extent.min_lon).max(f64::EPSILON);
    let lat_span = (extent.max_lat - extent.min_lat).max(f64::EPSILON);
    let x = ((lon - extent.min_lon) / lon_span).clamp(-2.0, 3.0) as f32 * width;
    let normalized_lat = ((lat - extent.min_lat) / lat_span).clamp(-2.0, 3.0);
    let y = if extent.lat_increases_down {
        normalized_lat as f32 * height
    } else {
        (1.0 - normalized_lat) as f32 * height
    };
    (x, y)
}

fn draw_graticule(pixmap: &mut Pixmap, extent: Extent, colors: MapOverlayColors) {
    let width = pixmap.width() as f32;
    let height = pixmap.height() as f32;
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(
        colors.grid[0],
        colors.grid[1],
        colors.grid[2],
        colors.grid[3],
    ));
    let stroke = Stroke {
        width: 1.0,
        ..Stroke::default()
    };
    let lon_step = if extent.max_lon - extent.min_lon > 180.0 {
        30.0
    } else {
        15.0
    };
    let lat_step = if extent.max_lat - extent.min_lat > 90.0 {
        30.0
    } else {
        15.0
    };
    let mut lon = (extent.min_lon / lon_step).ceil() * lon_step;
    while lon <= extent.max_lon {
        let (x, _) = project(lon, extent.min_lat, extent, width, height);
        let mut path = PathBuilder::new();
        path.move_to(x, 0.0);
        path.line_to(x, height);
        if let Some(path) = path.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
        lon += lon_step;
    }
    let mut lat = (extent.min_lat / lat_step).ceil() * lat_step;
    while lat <= extent.max_lat {
        let (_, y) = project(extent.min_lon, lat, extent, width, height);
        let mut path = PathBuilder::new();
        path.move_to(0.0, y);
        path.line_to(width, y);
        if let Some(path) = path.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
        lat += lat_step;
    }
}

fn polygon_intersects_extent(polygon: &[(f64, f64)], extent: Extent) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut p_min_lat = f64::INFINITY;
    let mut p_max_lat = f64::NEG_INFINITY;
    let mut p_min_lon = f64::INFINITY;
    let mut p_max_lon = f64::NEG_INFINITY;

    for &(raw_lon, lat) in polygon {
        p_min_lat = p_min_lat.min(lat);
        p_max_lat = p_max_lat.max(lat);
        let lon = display_longitude(raw_lon, extent.zero_to_360);
        p_min_lon = p_min_lon.min(lon);
        p_max_lon = p_max_lon.max(lon);
    }

    if p_max_lat < extent.min_lat || p_min_lat > extent.max_lat {
        return false;
    }

    if (extent.max_lon - extent.min_lon) >= 350.0 {
        return true;
    }

    if (p_max_lon - p_min_lon) > 180.0 {
        return true;
    }

    if p_max_lon < extent.min_lon || p_min_lon > extent.max_lon {
        return false;
    }

    true
}

fn draw_polygon(
    pixmap: &mut Pixmap,
    polygon: &[(f64, f64)],
    extent: Extent,
    colors: MapOverlayColors,
) {
    if polygon.len() < 3 {
        return;
    }
    let width = pixmap.width() as f32;
    let height = pixmap.height() as f32;
    let mut path = PathBuilder::new();
    let mut started = false;
    let mut previous_raw_lon: Option<f64> = None;
    for &(raw_lon, lat) in polygon {
        let lon = display_longitude(raw_lon, extent.zero_to_360);
        if let Some(previous) = previous_raw_lon
            && (lon - previous).abs() > 180.0
        {
            if started {
                path.close();
            }
            let (x, y) = project(lon, lat, extent, width, height);
            path.move_to(x, y);
            started = true;
            previous_raw_lon = Some(lon);
            continue;
        }
        let (x, y) = project(lon, lat, extent, width, height);
        if !started {
            path.move_to(x, y);
            started = true;
        } else {
            path.line_to(x, y);
        }
        previous_raw_lon = Some(lon);
    }
    if started {
        path.close();
    }
    let Some(path) = path.finish() else { return };
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(
        colors.land[0],
        colors.land[1],
        colors.land[2],
        255,
    ));
    pixmap.fill_path(
        &path,
        &paint,
        FillRule::EvenOdd,
        Transform::identity(),
        None,
    );
    let mut coast = Paint::default();
    coast.set_color(Color::from_rgba8(
        colors.coast[0],
        colors.coast[1],
        colors.coast[2],
        colors.coast[3],
    ));
    let stroke = Stroke {
        width: 0.8,
        ..Stroke::default()
    };
    pixmap.stroke_path(&path, &coast, &stroke, Transform::identity(), None);
}

fn normalize_longitude(longitude: f64) -> f64 {
    let mut longitude = longitude;
    while longitude < -180.0 {
        longitude += 360.0;
    }
    while longitude > 180.0 {
        longitude -= 360.0;
    }
    longitude
}

fn display_longitude(longitude: f64, zero_to_360: bool) -> f64 {
    let longitude = normalize_longitude(longitude);
    if zero_to_360 && longitude < 0.0 {
        longitude + 360.0
    } else {
        longitude
    }
}

#[cfg(test)]
mod tests {
    use super::{Extent, display_longitude, extent, render};
    use crate::data::slice::CoordinateGrid;
    use crate::render::colors::Palette;
    use crate::render::landmask::Detail;
    use ndarray::array;

    #[test]
    fn polygon_intersects_extent_skips_offscreen_polygons() {
        let extent = Extent {
            min_lon: 0.0,
            max_lon: 10.0,
            min_lat: 0.0,
            max_lat: 10.0,
            lat_increases_down: false,
            zero_to_360: false,
        };
        // Inside
        let inside = vec![(2.0, 2.0), (8.0, 2.0), (5.0, 8.0)];
        assert!(super::polygon_intersects_extent(&inside, extent));

        // Completely outside in latitude
        let far_north = vec![(2.0, 20.0), (8.0, 20.0), (5.0, 25.0)];
        assert!(!super::polygon_intersects_extent(&far_north, extent));

        // Completely outside in longitude
        let far_east = vec![(50.0, 2.0), (58.0, 2.0), (55.0, 8.0)];
        assert!(!super::polygon_intersects_extent(&far_east, extent));
    }

    #[test]
    fn backdrop_has_requested_dimensions_and_layers() {
        let image = render(360, 180, None, Detail::Global);
        assert_eq!(image.dimensions(), (360, 180));
        assert!(image.pixels().any(|pixel| pixel.0 != [15, 22, 36]));
        // Regression checks for the large seam-crossing ring that contains
        // Europe and Africa in the vendored 110m topology.
        assert_ne!(image.get_pixel(190, 40).0, [15, 22, 36]); // 10°E, 50°N
        assert_ne!(image.get_pixel(200, 90).0, [15, 22, 36]); // 20°E, 0°N
    }

    #[test]
    fn zero_to_360_extent_keeps_backdrop_aligned_with_source_columns() {
        let coordinates = CoordinateGrid {
            latitude: Some(array![
                [-90.0, -90.0, -90.0, -90.0],
                [90.0, 90.0, 90.0, 90.0]
            ]),
            longitude: Some(array![[0.0, 90.0, 180.0, 270.0], [0.0, 90.0, 180.0, 270.0]]),
            latitude_axis: None,
            longitude_axis: None,
        };
        let actual = extent(Some(&coordinates));
        let expected = Extent {
            min_lon: 0.0,
            max_lon: 270.0,
            min_lat: -90.0,
            max_lat: 90.0,
            lat_increases_down: true,
            zero_to_360: true,
        };
        assert_eq!(actual.min_lon, expected.min_lon);
        assert_eq!(actual.max_lon, expected.max_lon);
        assert!(actual.zero_to_360);
        assert_eq!(display_longitude(-90.0, true), 270.0);
        assert_eq!(display_longitude(90.0, true), 90.0);
        assert_eq!(Palette::Viridis.map_overlay_colors().land, [232, 238, 244]);
    }
}
