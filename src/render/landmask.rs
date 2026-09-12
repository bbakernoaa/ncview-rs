//! A presentation-only coastline overlay for the terminal map.
//!
//! This is intentionally a presentation aid, not a scientific land/sea mask.
//! The vendored 1:110m Natural Earth land topology from world-atlas is decoded
//! by `build.rs` into ordinary lon/lat polygons. It is deliberately not used
//! as a scientific land/sea mask: dataset-provided masks and coordinates remain
//! authoritative. A coarse fallback remains available if the generated asset
//! is unavailable during a development build.

use std::sync::OnceLock;

use crate::data::slice::CoordinateGrid;

mod vendored {
    include!(concat!(env!("OUT_DIR"), "/world_atlas_land.rs"));
}

const FALLBACK_POLYGONS: &[&[(f64, f64)]] = &[
    // North America, including Central America.
    &[
        (-168.0, 72.0),
        (-140.0, 72.0),
        (-125.0, 60.0),
        (-118.0, 48.0),
        (-105.0, 25.0),
        (-88.0, 15.0),
        (-80.0, 8.0),
        (-65.0, 18.0),
        (-78.0, 35.0),
        (-52.0, 55.0),
        (-65.0, 70.0),
        (-100.0, 83.0),
        (-145.0, 82.0),
    ],
    // South America.
    &[
        (-82.0, 13.0),
        (-60.0, 12.0),
        (-35.0, 4.0),
        (-42.0, -15.0),
        (-50.0, -30.0),
        (-68.0, -56.0),
        (-78.0, -42.0),
        (-74.0, -15.0),
    ],
    // Greenland.
    &[
        (-74.0, 59.0),
        (-18.0, 59.0),
        (-12.0, 83.0),
        (-45.0, 90.0),
        (-68.0, 80.0),
    ],
    // Europe and Asia (coarse mainland silhouette).
    &[
        (-12.0, 36.0),
        (10.0, 35.0),
        (30.0, 28.0),
        (45.0, 12.0),
        (65.0, 5.0),
        (95.0, 8.0),
        (115.0, 20.0),
        (140.0, 35.0),
        (170.0, 48.0),
        (170.0, 72.0),
        (120.0, 76.0),
        (70.0, 72.0),
        (35.0, 70.0),
        (12.0, 60.0),
        (-10.0, 55.0),
    ],
    // Africa and Madagascar.
    &[
        (-18.0, 35.0),
        (10.0, 37.0),
        (35.0, 30.0),
        (52.0, 10.0),
        (42.0, -18.0),
        (20.0, -35.0),
        (-5.0, -35.0),
        (-18.0, -5.0),
    ],
    &[(43.0, -12.0), (51.0, -13.0), (50.0, -25.0), (44.0, -26.0)],
    // Australia and nearby islands.
    &[
        (112.0, -10.0),
        (154.0, -10.0),
        (154.0, -39.0),
        (135.0, -44.0),
        (112.0, -34.0),
    ],
    &[(95.0, 6.0), (140.0, 8.0), (145.0, -10.0), (118.0, -12.0)],
    // Antarctica.
    &[
        (-180.0, -68.0),
        (180.0, -68.0),
        (180.0, -90.0),
        (-180.0, -90.0),
    ],
];

const POLYGONS: &[&[(f64, f64)]] = if vendored::POLYGONS.is_empty() {
    FALLBACK_POLYGONS
} else {
    vendored::POLYGONS
};

const LONGITUDE_BINS: usize = 72;
const LATITUDE_BINS: usize = 36;

/// Coarse candidate index for the coastline polygons. It only reduces the
/// number of point-in-polygon tests; the original polygon vertices remain the
/// source of truth for classification.
struct SpatialIndex {
    bins: Vec<Vec<usize>>,
}

impl SpatialIndex {
    fn from_bounds(bounds: &[(f64, f64, f64, f64)]) -> Self {
        let mut bins = vec![Vec::new(); LONGITUDE_BINS * LATITUDE_BINS];
        for (index, &(min_lon, max_lon, min_lat, max_lat)) in bounds.iter().enumerate() {
            if !min_lon.is_finite()
                || !max_lon.is_finite()
                || !min_lat.is_finite()
                || !max_lat.is_finite()
            {
                continue;
            }
            let first_lat = latitude_bin(min_lat);
            let last_lat = latitude_bin(max_lat);
            let span = max_lon - min_lon;
            let mut longitudes = Vec::new();
            if span >= 180.0 {
                longitudes.extend(0..LONGITUDE_BINS);
            } else {
                let first_lon = longitude_bin(min_lon);
                let last_lon = longitude_bin(max_lon);
                if first_lon <= last_lon {
                    longitudes.extend(first_lon..=last_lon);
                } else {
                    longitudes.extend(first_lon..LONGITUDE_BINS);
                    longitudes.extend(0..=last_lon);
                }
            }
            for lat in first_lat..=last_lat {
                for &lon in &longitudes {
                    bins[lat * LONGITUDE_BINS + lon].push(index);
                }
            }
        }
        Self { bins }
    }

    fn candidates(&self, latitude: f64, longitude: f64) -> &[usize] {
        &self.bins[latitude_bin(latitude) * LONGITUDE_BINS + longitude_bin(longitude)]
    }
}

static GLOBAL_INDEX: OnceLock<SpatialIndex> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
    Global,
    Regional,
    Local,
}

pub fn detail_for_grid(grid: Option<&CoordinateGrid>) -> Detail {
    match std::env::var("NCVIEW_LAND_DETAIL")
        .ok()
        .as_deref()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("10m") | Some("local") => return Detail::Local,
        Some("50m") | Some("regional") => return Detail::Regional,
        Some("auto") => {}
        _ => return Detail::Global,
    }
    detail_for_extent(grid)
}

fn detail_for_extent(grid: Option<&CoordinateGrid>) -> Detail {
    let Some(grid) = grid else {
        return Detail::Global;
    };
    let Some(latitude) = grid.latitude.as_ref() else {
        return Detail::Global;
    };
    let Some(longitude) = grid.longitude.as_ref() else {
        return Detail::Global;
    };
    let mut min_longitude = f64::INFINITY;
    let mut max_longitude = f64::NEG_INFINITY;
    for value in longitude.iter().copied().filter(|value| value.is_finite()) {
        min_longitude = min_longitude.min(value);
        max_longitude = max_longitude.max(value);
    }
    let mut min_latitude = f64::INFINITY;
    let mut max_latitude = f64::NEG_INFINITY;
    for value in latitude.iter().copied().filter(|value| value.is_finite()) {
        min_latitude = min_latitude.min(value);
        max_latitude = max_latitude.max(value);
    }
    if !min_longitude.is_finite()
        || !max_longitude.is_finite()
        || !min_latitude.is_finite()
        || !max_latitude.is_finite()
    {
        return Detail::Global;
    }
    let longitude_span = (max_longitude - min_longitude).abs();
    let latitude_span = (max_latitude - min_latitude).abs();
    let span = longitude_span.max(latitude_span);
    if span <= 20.0 {
        Detail::Local
    } else if span <= 120.0 {
        Detail::Regional
    } else {
        Detail::Global
    }
}

pub fn is_land(latitude: f64, longitude: f64) -> bool {
    is_land_with_detail(latitude, longitude, Detail::Global)
}

/// Return the lazily selected Natural Earth rings for a presentation layer.
/// The map renderer uses these filled rings; scientific masking still comes
/// exclusively from the dataset's validity values.
pub fn polygons_for_detail(detail: Detail) -> Vec<&'static [(f64, f64)]> {
    match detail {
        Detail::Global if !vendored::POLYGONS.is_empty() => vendored::POLYGONS.to_vec(),
        Detail::Global => FALLBACK_POLYGONS.to_vec(),
        Detail::Regional => regional_polygons()
            .polygons
            .iter()
            .map(Vec::as_slice)
            .collect(),
        Detail::Local => local_polygons()
            .polygons
            .iter()
            .map(Vec::as_slice)
            .collect(),
    }
}

pub fn is_land_with_detail(latitude: f64, longitude: f64, detail: Detail) -> bool {
    let longitude = normalize_longitude(longitude);
    match detail {
        Detail::Global if !vendored::POLYGONS.is_empty() => contains_any(
            POLYGONS,
            vendored::BOUNDS,
            GLOBAL_INDEX.get_or_init(|| SpatialIndex::from_bounds(vendored::BOUNDS)),
            latitude,
            longitude,
        ),
        Detail::Global => FALLBACK_POLYGONS
            .iter()
            .any(|polygon| contains(polygon, latitude, longitude)),
        Detail::Regional => {
            let polygons = regional_polygons();
            contains_any_vec(polygons, latitude, longitude)
        }
        Detail::Local => {
            let polygons = local_polygons();
            contains_any_vec(polygons, latitude, longitude)
        }
    }
}

pub fn cell_is_border(rows: usize, cols: usize, row: usize, col: usize) -> bool {
    cell_is_border_with_detail(rows, cols, row, col, Detail::Global)
}

pub fn cell_is_border_with_detail(
    rows: usize,
    cols: usize,
    row: usize,
    col: usize,
    detail: Detail,
) -> bool {
    let center = cell_is_land(rows, cols, row, col, detail);
    // Draw the coastline on the ocean-facing side only. Marking both sides
    // of every transition made the old overlay look like a two-cell-wide
    // beige stripe, especially after image upsampling.
    if center {
        return false;
    }
    [
        (row.wrapping_sub(1), col),
        (row + 1, col),
        (row, col.wrapping_sub(1)),
        (row, col + 1),
        (row.wrapping_sub(1), col.wrapping_sub(1)),
        (row.wrapping_sub(1), col + 1),
        (row + 1, col.wrapping_sub(1)),
        (row + 1, col + 1),
    ]
    .iter()
    .filter(|&&(neighbor_row, neighbor_col)| neighbor_row < rows && neighbor_col < cols)
    .any(|&(neighbor_row, neighbor_col)| {
        cell_is_land(rows, cols, neighbor_row, neighbor_col, detail)
    })
}

/// Return whether an ocean cell borders land using the displayed coordinate
/// grid. This preserves geographic framing for zoomed regular grids and for
/// curvilinear 2-D latitude/longitude coordinates.
pub fn cell_is_border_grid(grid: &CoordinateGrid, row: usize, col: usize) -> Option<bool> {
    cell_is_border_grid_with_detail(grid, row, col, detail_for_grid(Some(grid)))
}

pub fn cell_is_border_grid_with_detail(
    grid: &CoordinateGrid,
    row: usize,
    col: usize,
    detail: Detail,
) -> Option<bool> {
    let latitude = grid.latitude.as_ref()?;
    let longitude = grid.longitude.as_ref()?;
    let (rows, cols) = latitude.dim();
    if longitude.dim() != (rows, cols) || row >= rows || col >= cols {
        return None;
    }
    let at = |neighbor_row: usize, neighbor_col: usize| {
        Some(is_land_with_detail(
            *latitude.get((neighbor_row, neighbor_col))?,
            *longitude.get((neighbor_row, neighbor_col))?,
            detail,
        ))
    };
    let center = at(row, col)?;
    if center {
        return Some(false);
    }
    let neighbors = [
        (row.wrapping_sub(1), col),
        (row + 1, col),
        (row, col.wrapping_sub(1)),
        (row, col + 1),
        (row.wrapping_sub(1), col.wrapping_sub(1)),
        (row.wrapping_sub(1), col + 1),
        (row + 1, col.wrapping_sub(1)),
        (row + 1, col + 1),
    ];
    Some(
        neighbors
            .iter()
            .filter(|&&(neighbor_row, neighbor_col)| neighbor_row < rows && neighbor_col < cols)
            .any(|&(neighbor_row, neighbor_col)| at(neighbor_row, neighbor_col).unwrap_or(false)),
    )
}

fn cell_is_land(rows: usize, cols: usize, row: usize, col: usize, detail: Detail) -> bool {
    if rows <= 1 || cols <= 1 {
        return false;
    }
    let latitude = 90.0 - 180.0 * row as f64 / (rows - 1) as f64;
    let longitude = -180.0 + 360.0 * col as f64 / (cols - 1) as f64;
    is_land_with_detail(latitude, longitude, detail)
}

struct PolygonSet {
    polygons: Vec<Vec<(f64, f64)>>,
    bounds: Vec<(f64, f64, f64, f64)>,
    index: SpatialIndex,
}

static REGIONAL_POLYGONS: OnceLock<PolygonSet> = OnceLock::new();
static LOCAL_POLYGONS: OnceLock<PolygonSet> = OnceLock::new();

fn regional_polygons() -> &'static PolygonSet {
    REGIONAL_POLYGONS.get_or_init(|| {
        decode_topology(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/world-atlas/land-50m.json"
        )))
    })
}

fn local_polygons() -> &'static PolygonSet {
    LOCAL_POLYGONS.get_or_init(|| {
        decode_topology(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/world-atlas/land-10m.json"
        )))
    })
}

fn contains_any(
    polygons: &[&[(f64, f64)]],
    bounds: &[(f64, f64, f64, f64)],
    index: &SpatialIndex,
    latitude: f64,
    longitude: f64,
) -> bool {
    index
        .candidates(latitude, longitude)
        .iter()
        .copied()
        .any(|index| {
            let Some(polygon) = polygons.get(index) else {
                return false;
            };
            let Some(&(min_longitude, max_longitude, min_latitude, max_latitude)) =
                bounds.get(index)
            else {
                return false;
            };
            bounds_contain(
                min_longitude,
                max_longitude,
                min_latitude,
                max_latitude,
                latitude,
                longitude,
            ) && contains(polygon, latitude, longitude)
        })
}

fn contains_any_vec(polygons: &PolygonSet, latitude: f64, longitude: f64) -> bool {
    polygons
        .index
        .candidates(latitude, longitude)
        .iter()
        .copied()
        .any(|index| {
            let Some(polygon) = polygons.polygons.get(index) else {
                return false;
            };
            let Some(&(min_longitude, max_longitude, min_latitude, max_latitude)) =
                polygons.bounds.get(index)
            else {
                return false;
            };
            bounds_contain(
                min_longitude,
                max_longitude,
                min_latitude,
                max_latitude,
                latitude,
                longitude,
            ) && contains(polygon, latitude, longitude)
        })
}

fn decode_topology(json: &str) -> PolygonSet {
    let topology: serde_json::Value =
        serde_json::from_str(json).expect("vendored world-atlas file is not valid JSON");
    let transform = topology
        .get("transform")
        .and_then(serde_json::Value::as_object)
        .expect("world-atlas transform is missing");
    let scale = pair(transform.get("scale"));
    let translate = pair(transform.get("translate"));
    let arcs = topology
        .get("arcs")
        .and_then(serde_json::Value::as_array)
        .expect("world-atlas arcs are missing")
        .iter()
        .map(|arc| decode_arc(arc, scale, translate))
        .collect::<Vec<_>>();
    let geometries = topology
        .get("objects")
        .and_then(|objects| objects.get("land"))
        .and_then(|land| land.get("geometries"))
        .and_then(serde_json::Value::as_array)
        .expect("world-atlas land geometries are missing");
    let mut polygons = Vec::new();
    for geometry in geometries {
        if let Some(rings) = geometry.get("arcs").and_then(serde_json::Value::as_array) {
            collect_rings(rings, &arcs, &mut polygons);
        }
    }
    let bounds: Vec<(f64, f64, f64, f64)> = polygons
        .iter()
        .map(|polygon| {
            let min_longitude = polygon
                .iter()
                .map(|(longitude, _)| *longitude)
                .fold(f64::INFINITY, f64::min);
            let max_longitude = polygon
                .iter()
                .map(|(longitude, _)| *longitude)
                .fold(f64::NEG_INFINITY, f64::max);
            let min_latitude = polygon
                .iter()
                .map(|(_, latitude)| *latitude)
                .fold(f64::INFINITY, f64::min);
            let max_latitude = polygon
                .iter()
                .map(|(_, latitude)| *latitude)
                .fold(f64::NEG_INFINITY, f64::max);
            (min_longitude, max_longitude, min_latitude, max_latitude)
        })
        .collect();
    let index = SpatialIndex::from_bounds(&bounds);
    PolygonSet {
        polygons,
        bounds,
        index,
    }
}

fn pair(value: Option<&serde_json::Value>) -> [f64; 2] {
    let values = value
        .and_then(serde_json::Value::as_array)
        .expect("world-atlas transform is missing");
    [
        values[0]
            .as_f64()
            .expect("world-atlas transform is not numeric"),
        values[1]
            .as_f64()
            .expect("world-atlas transform is not numeric"),
    ]
}

fn decode_arc(arc: &serde_json::Value, scale: [f64; 2], translate: [f64; 2]) -> Vec<(f64, f64)> {
    let mut x = 0_i64;
    let mut y = 0_i64;
    arc.as_array()
        .expect("world-atlas arc is not an array")
        .iter()
        .map(|point| {
            let values = point.as_array().expect("world-atlas point is not an array");
            x += values[0]
                .as_i64()
                .expect("world-atlas x coordinate is not an integer");
            y += values[1]
                .as_i64()
                .expect("world-atlas y coordinate is not an integer");
            (
                x as f64 * scale[0] + translate[0],
                y as f64 * scale[1] + translate[1],
            )
        })
        .collect()
}

fn collect_rings(
    rings: &[serde_json::Value],
    arcs: &[Vec<(f64, f64)>],
    polygons: &mut Vec<Vec<(f64, f64)>>,
) {
    if rings
        .first()
        .and_then(serde_json::Value::as_array)
        .is_some_and(|value| value.first().and_then(serde_json::Value::as_i64).is_some())
    {
        for ring in rings {
            let Some(arc_indices) = ring.as_array() else {
                continue;
            };
            let mut polygon = Vec::new();
            for (part, arc_index) in arc_indices.iter().enumerate() {
                let encoded = arc_index
                    .as_i64()
                    .expect("world-atlas arc index is not an integer");
                let reverse = encoded < 0;
                let index = if reverse {
                    (-encoded - 1) as usize
                } else {
                    encoded as usize
                };
                let Some(arc) = arcs.get(index) else {
                    continue;
                };
                let points = if reverse {
                    arc.iter().rev().copied().collect::<Vec<_>>()
                } else {
                    arc.clone()
                };
                polygon.extend(points.into_iter().skip(usize::from(part > 0)));
            }
            if polygon.len() >= 3 {
                close_ring(&mut polygon);
                polygons.push(polygon);
            }
        }
    } else {
        for polygon in rings {
            if let Some(rings) = polygon.as_array() {
                collect_rings(rings, arcs, polygons);
            }
        }
    }
}

/// TopoJSON rings are implicitly closed. Keep an explicit closing vertex so
/// any future line renderer cannot accidentally leave a gap at the join.
fn close_ring(polygon: &mut Vec<(f64, f64)>) {
    if polygon.first() != polygon.last()
        && let Some(first) = polygon.first().copied()
    {
        polygon.push(first);
    }
}

fn normalize_longitude(longitude: f64) -> f64 {
    (longitude + 180.0).rem_euclid(360.0) - 180.0
}

fn longitude_bin(longitude: f64) -> usize {
    (((normalize_longitude(longitude) + 180.0) / 360.0 * LONGITUDE_BINS as f64).floor() as usize)
        .min(LONGITUDE_BINS - 1)
}

fn latitude_bin(latitude: f64) -> usize {
    (((latitude.clamp(-90.0, 90.0) + 90.0) / 180.0 * LATITUDE_BINS as f64).floor() as usize)
        .min(LATITUDE_BINS - 1)
}

fn bounds_contain(
    min_longitude: f64,
    max_longitude: f64,
    min_latitude: f64,
    max_latitude: f64,
    latitude: f64,
    longitude: f64,
) -> bool {
    latitude >= min_latitude
        && latitude <= max_latitude
        // A raw TopoJSON bounding box wider than half the globe is treated as
        // seam-spanning; the polygon test performs the exact classification.
        && (max_longitude - min_longitude >= 180.0
            || (longitude >= min_longitude && longitude <= max_longitude))
}

fn contains(polygon: &[(f64, f64)], latitude: f64, longitude: f64) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    // TopoJSON coordinates use the [-180, 180] seam. Unwrap each edge around
    // the first vertex before ray casting; otherwise an edge from +180 to
    // -180 is interpreted as a world-spanning horizontal coastline.
    let anchor = polygon[0].0;
    let longitude = unwrap_near(longitude, anchor);
    let mut inside = false;
    let mut previous = (
        unwrap_near(polygon[polygon.len() - 1].0, anchor),
        polygon[polygon.len() - 1].1,
    );
    for &(raw_longitude, latitude_value) in polygon {
        let current = (unwrap_near(raw_longitude, anchor), latitude_value);
        let crosses = (current.1 > latitude) != (previous.1 > latitude);
        if crosses {
            let edge_longitude = (previous.0 - current.0) * (latitude - current.1)
                / (previous.1 - current.1)
                + current.0;
            if longitude < edge_longitude {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

fn unwrap_near(longitude: f64, anchor: f64) -> f64 {
    let diff = longitude - anchor;
    if diff.abs() <= 180.0 {
        longitude
    } else {
        longitude - 360.0 * (diff / 360.0).round()
    }
}

#[cfg(test)]
mod tests {
    use super::{Detail, cell_is_border, contains, is_land, is_land_with_detail};

    #[test]
    fn classifies_representative_land_and_ocean_points() {
        assert!(is_land(40.7, -74.1));
        assert!(is_land(-25.0, 133.0));
        assert!(!is_land(0.0, -150.0));
    }

    #[test]
    fn detects_a_coastline_transition() {
        let found = (0..180).any(|row| (0..360).any(|col| cell_is_border(180, 360, row, col)));
        assert!(found);
    }

    #[test]
    fn selects_detail_from_coordinate_extent() {
        use crate::data::slice::CoordinateGrid;
        use ndarray::array;

        let local = CoordinateGrid {
            latitude: Some(array![[40.0, 40.0], [41.0, 41.0]]),
            longitude: Some(array![[-75.0, -74.0], [-75.0, -74.0]]),
        };
        let regional = CoordinateGrid {
            latitude: Some(array![[0.0, 0.0], [50.0, 50.0]]),
            longitude: Some(array![[-40.0, 40.0], [-40.0, 40.0]]),
        };
        assert_eq!(super::detail_for_extent(Some(&local)), Detail::Local);
        assert_eq!(super::detail_for_extent(Some(&regional)), Detail::Regional);
        assert_eq!(super::detail_for_extent(None), Detail::Global);
        assert!(is_land_with_detail(40.7, -74.1, Detail::Regional));
        assert!(is_land_with_detail(40.7, -74.1, Detail::Local));
    }

    #[test]
    fn seam_crossing_rings_do_not_create_world_spanning_edges() {
        let ring = [
            (179.0, -1.0),
            (-179.0, -1.0),
            (-179.0, 1.0),
            (179.0, 1.0),
            (179.0, -1.0),
        ];
        assert!(contains(&ring, 0.0, 179.8));
        assert!(contains(&ring, 0.0, -179.8));
        assert!(!contains(&ring, 0.0, 170.0));
    }

    #[test]
    fn vendored_110m_rings_are_explicitly_closed() {
        assert!(!super::vendored::POLYGONS.is_empty());
        assert!(
            super::vendored::POLYGONS
                .iter()
                .all(|polygon| { polygon.len() >= 4 && polygon.first() == polygon.last() })
        );
    }
}
