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
/// The output is geographic north-up so it can be composited directly beneath
/// the normalized scientific data raster.
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
    zero_to_360: bool,
}

fn extent(coordinates: Option<&CoordinateGrid>) -> Extent {
    let Some(grid) = coordinates else {
        return Extent {
            min_lon: -180.0,
            max_lon: 180.0,
            min_lat: -90.0,
            max_lat: 90.0,
            zero_to_360: false,
        };
    };
    let mut min_lon = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    let mut min_lat = f64::INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    if let Some(latitude) = grid.latitude.as_ref() {
        for value in latitude.iter().copied().filter(|value| value.is_finite()) {
            min_lat = min_lat.min(value);
            max_lat = max_lat.max(value);
        }
    } else if let Some(latitude) = grid.latitude_axis.as_ref() {
        for value in latitude.iter().copied().filter(|value| value.is_finite()) {
            min_lat = min_lat.min(value);
            max_lat = max_lat.max(value);
        }
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
        zero_to_360,
    }
}

fn project(lon: f64, lat: f64, extent: Extent, width: f32, height: f32) -> (f32, f32) {
    let lon_span = (extent.max_lon - extent.min_lon).max(f64::EPSILON);
    let lat_span = (extent.max_lat - extent.min_lat).max(f64::EPSILON);
    let x = ((lon - extent.min_lon) / lon_span).clamp(-2.0, 3.0) as f32 * width;
    let normalized_lat = ((lat - extent.min_lat) / lat_span).clamp(-2.0, 3.0);
    let y = (1.0 - normalized_lat) as f32 * height;
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

/// Unwrap a ring's longitudes into a continuous sequence anchored near the
/// extent center. Natural Earth rings use the [-180, 180] seam; a raw edge
/// from +180 to -180 must stay a short edge, not a world-spanning chord.
/// The unwrapped ring may extend past the extent window; callers draw the
/// ±360° copies whose bounding boxes intersect the window so both sides of
/// a seam are filled.
fn unwrapped_ring(polygon: &[(f64, f64)], extent: Extent) -> Vec<(f64, f64)> {
    let center = 0.5 * (extent.min_lon + extent.max_lon);
    let mut ring = Vec::with_capacity(polygon.len() + 2);
    let mut anchor = center;
    let mut min_lon = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    let mut lat_sum = 0.0;
    for &(lon, lat) in polygon {
        let lon = lon - 360.0 * ((lon - anchor) / 360.0).round();
        ring.push((lon, lat));
        anchor = lon;
        min_lon = min_lon.min(lon);
        max_lon = max_lon.max(lon);
        lat_sum += lat;
    }
    // Globe-spanning rings (Antarctica in the Natural Earth topology) close
    // with a seam jump instead of an explicit polar edge; spherical winders
    // like d3 fill the polar cap, but a planar fill would only paint the
    // coastal band. Route the closure via the pole so the cap is filled.
    if max_lon - min_lon >= 359.9
        && let Some(&first) = ring.first()
        && let Some(&last) = ring.last()
    {
        let pole = if lat_sum < 0.0 { -90.0 } else { 90.0 };
        ring.push((last.0, pole));
        ring.push((first.0, pole));
    }
    ring
}

/// Longitude offsets whose shifted copy of an unwrapped ring can intersect
/// the extent window.
fn visible_shifts(ring: &[(f64, f64)], extent: Extent) -> Vec<f64> {
    let mut min_lon = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    for &(lon, _) in ring {
        min_lon = min_lon.min(lon);
        max_lon = max_lon.max(lon);
    }
    [-360.0, 0.0, 360.0]
        .into_iter()
        .filter(|shift| min_lon + shift <= extent.max_lon && max_lon + shift >= extent.min_lon)
        .collect()
}

fn polygon_intersects_extent(polygon: &[(f64, f64)], extent: Extent) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut p_min_lat = f64::INFINITY;
    let mut p_max_lat = f64::NEG_INFINITY;
    for &(_, lat) in polygon {
        p_min_lat = p_min_lat.min(lat);
        p_max_lat = p_max_lat.max(lat);
    }
    if p_max_lat < extent.min_lat || p_min_lat > extent.max_lat {
        return false;
    }
    if (extent.max_lon - extent.min_lon) >= 350.0 {
        return true;
    }
    let ring = unwrapped_ring(polygon, extent);
    !visible_shifts(&ring, extent).is_empty()
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
    // Unwrap the ring continuously, then draw every ±360° copy that reaches
    // the extent window. Splitting on displayed-longitude jumps instead
    // would close subpaths with straight chords and smear land across the
    // antimeridian (image center in 0–360 mode).
    let ring = unwrapped_ring(polygon, extent);
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(
        colors.land[0],
        colors.land[1],
        colors.land[2],
        255,
    ));
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
    for shift in visible_shifts(&ring, extent) {
        let mut path = PathBuilder::new();
        for (index, &(lon, lat)) in ring.iter().enumerate() {
            let (x, y) = project(lon + shift, lat, extent, width, height);
            if index == 0 {
                path.move_to(x, y);
            } else {
                path.line_to(x, y);
            }
        }
        path.close();
        let Some(path) = path.finish() else { continue };
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::EvenOdd,
            Transform::identity(),
            None,
        );
        pixmap.stroke_path(&path, &coast, &stroke, Transform::identity(), None);
    }
}

#[cfg(test)]
mod tests {
    use super::{Extent, extent, render};
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

    fn global_zero_to_360_grid() -> CoordinateGrid {
        CoordinateGrid {
            latitude: None,
            longitude: None,
            latitude_axis: Some((-89..=89).map(f64::from).collect()),
            longitude_axis: Some((0..360).map(f64::from).collect()),
        }
    }

    #[test]
    fn zero_to_360_backdrop_keeps_antarctica_across_the_antimeridian() {
        // A 0–360 global grid puts the antimeridian at the image center.
        // Antarctica spans every longitude, so land must be continuous there.
        let image = render(360, 180, Some(&global_zero_to_360_grid()), Detail::Global);
        let ocean = [15, 22, 36];
        // Latitude ~-81.6 (row 172): land away from the Ross Sea sector.
        for x in [40, 100, 260, 330] {
            assert_ne!(
                image.get_pixel(x, 172).0,
                ocean,
                "Antarctica missing at x={x} (lon {x}°)"
            );
        }
        // The Ross Sea stays ocean north of the ice-shelf front.
        assert_eq!(image.get_pixel(175, 172).0, ocean);
        // Latitude ~-86.5 (row 177): the polar cap must be filled on both
        // sides of the antimeridian (image center), not cut off by the
        // ring's seam-closing chord.
        for x in [178, 180, 182] {
            assert_ne!(
                image.get_pixel(x, 177).0,
                ocean,
                "polar cap missing at x={x} (lon {x}°)"
            );
        }
        // The equatorial Pacific at the antimeridian stays ocean: no chord
        // smears from seam-split rings may reach it. Sample beside the 180°
        // graticule line, which legitimately tints the exact center column.
        assert_eq!(image.get_pixel(181, 84).0, ocean);
        assert_ne!(image.get_pixel(180, 84).0, [232, 238, 244]);
    }

    #[test]
    fn seam_crossing_ring_fills_on_both_edges_of_a_zero_to_360_backdrop() {
        // A ring crossing the prime meridian (raw -10..10) must fill along
        // both the 360° and 0° image edges in 0–360 mode, not collapse into
        // chord-closed degenerate subpaths.
        let extent = Extent {
            min_lon: 0.0,
            max_lon: 360.0,
            min_lat: -90.0,
            max_lat: 90.0,
            zero_to_360: true,
        };
        let colors = Palette::Viridis.map_overlay_colors();
        let mut pixmap = tiny_skia::Pixmap::new(360, 180).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(15, 22, 36, 255));
        let ring = [
            (-10.0, -10.0),
            (10.0, -10.0),
            (10.0, 10.0),
            (-10.0, 10.0),
            (-10.0, -10.0),
        ];
        super::draw_polygon(&mut pixmap, &ring, extent, colors);
        let pixel = |x: u32, y: u32| {
            let index = (y as usize * 360 + x as usize) * 4;
            let data = pixmap.data();
            [data[index], data[index + 1], data[index + 2]]
        };
        let ocean = [15, 22, 36];
        assert_ne!(pixel(355, 90), ocean, "land missing near the 360° edge");
        assert_ne!(pixel(5, 90), ocean, "land missing near the 0° edge");
        assert_eq!(pixel(180, 90), ocean, "chord smear at the antimeridian");
    }

    #[test]
    fn antimeridian_crossing_ring_fills_across_the_center_of_a_zero_to_360_backdrop() {
        // A ring crossing 180° (raw 179..-179) must fill continuously across
        // the image center in 0–360 mode.
        let extent = Extent {
            min_lon: 0.0,
            max_lon: 360.0,
            min_lat: -90.0,
            max_lat: 90.0,
            zero_to_360: true,
        };
        let colors = Palette::Viridis.map_overlay_colors();
        let mut pixmap = tiny_skia::Pixmap::new(360, 180).unwrap();
        pixmap.fill(tiny_skia::Color::from_rgba8(15, 22, 36, 255));
        let ring = [
            (179.0, -1.0),
            (-179.0, -1.0),
            (-179.0, 1.0),
            (179.0, 1.0),
            (179.0, -1.0),
        ];
        super::draw_polygon(&mut pixmap, &ring, extent, colors);
        let pixel = |x: u32, y: u32| {
            let index = (y as usize * 360 + x as usize) * 4;
            let data = pixmap.data();
            [data[index], data[index + 1], data[index + 2]]
        };
        let ocean = [15, 22, 36];
        for x in [179, 180, 181] {
            assert_ne!(pixel(x, 90), ocean, "land missing at x={x}");
        }
        assert_eq!(pixel(90, 90), ocean);
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
            zero_to_360: true,
        };
        assert_eq!(actual.min_lon, expected.min_lon);
        assert_eq!(actual.max_lon, expected.max_lon);
        assert!(actual.zero_to_360);
        assert_eq!(Palette::Viridis.map_overlay_colors().land, [232, 238, 244]);
    }
}
