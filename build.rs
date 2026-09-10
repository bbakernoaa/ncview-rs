use std::{env, fmt::Write as _, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=assets/ncview-scientific-colour-maps-master");
    println!("cargo:rerun-if-changed=assets/world-atlas/land-110m.json");
    let root = PathBuf::from("assets/ncview-scientific-colour-maps-master");
    let mut maps = fs::read_dir(&root)
        .expect("vendored scientific colour-map directory is missing")
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension().and_then(|extension| extension.to_str()) == Some("ncmap"))
                .then_some(path)
        })
        .collect::<Vec<_>>();
    maps.sort();

    let mut generated = String::from("pub const VENDORED: &[(&str, &str)] = &[\n");
    for path in maps {
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("vendored colour-map filename is not UTF-8");
        let contents = fs::read_to_string(&path).expect("vendored colour-map is not UTF-8");
        writeln!(generated, "    ({name:?}, {contents:?}),").expect("generated map write failed");
    }
    generated.push_str("];\n");

    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is not set"))
        .join("vendored_colormaps.rs");
    fs::write(output, generated).expect("generated colour-map file cannot be written");

    generate_world_atlas();
}

fn generate_world_atlas() {
    let path = PathBuf::from("assets/world-atlas/land-110m.json");
    let topology: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&path).expect("vendored world-atlas file is not UTF-8"),
    )
    .expect("vendored world-atlas file is not valid JSON");
    let transform = topology
        .get("transform")
        .and_then(serde_json::Value::as_object)
        .expect("world-atlas transform is missing");
    let scale = pair(transform.get("scale"), "scale");
    let translate = pair(transform.get("translate"), "translate");
    let arcs = topology
        .get("arcs")
        .and_then(serde_json::Value::as_array)
        .expect("world-atlas arcs are missing");
    let decoded_arcs = arcs
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
        let Some(rings) = geometry.get("arcs").and_then(serde_json::Value::as_array) else {
            continue;
        };
        collect_rings(rings, &decoded_arcs, &mut polygons);
    }

    let mut generated = String::from("pub const POLYGONS: &[&[(f64, f64)]] = &[\n");
    let mut bounds = Vec::with_capacity(polygons.len());
    for polygon in polygons {
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
        bounds.push((min_longitude, max_longitude, min_latitude, max_latitude));
        generated.push_str("    &[\n");
        for (longitude, latitude) in polygon {
            writeln!(generated, "        ({longitude:?}, {latitude:?}),")
                .expect("generated coastline write failed");
        }
        generated.push_str("    ],\n");
    }
    generated.push_str("];\n");
    generated.push_str("pub const BOUNDS: &[(f64, f64, f64, f64)] = &[\n");
    for (min_longitude, max_longitude, min_latitude, max_latitude) in bounds {
        writeln!(
            generated,
            "    ({min_longitude:?}, {max_longitude:?}, {min_latitude:?}, {max_latitude:?}),"
        )
        .expect("generated coastline bounds write failed");
    }
    generated.push_str("];\n");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is not set"))
        .join("world_atlas_land.rs");
    fs::write(output, generated).expect("generated coastline file cannot be written");
}

fn pair(value: Option<&serde_json::Value>, name: &str) -> [f64; 2] {
    let values = value
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("world-atlas {name} is missing"));
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
    let mut x = 0i64;
    let mut y = 0i64;
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
                let Some(arc) = arcs.get(index) else { continue };
                let points = if reverse {
                    arc.iter().rev().copied().collect::<Vec<_>>()
                } else {
                    arc.clone()
                };
                let skip = usize::from(part > 0);
                polygon.extend(points.into_iter().skip(skip));
            }
            if polygon.len() >= 3 {
                close_ring(&mut polygon);
                polygons.push(polygon);
            }
        }
    } else {
        for polygon in rings {
            let Some(rings) = polygon.as_array() else {
                continue;
            };
            collect_rings(rings, arcs, polygons);
        }
    }
}

/// TopoJSON rings are implicitly closed.  Keep the explicit closing vertex in
/// the generated data as well, so consumers that draw segments directly do
/// not leave a visible gap between the final and first vertices.
fn close_ring(polygon: &mut Vec<(f64, f64)>) {
    if polygon.first() != polygon.last()
        && let Some(first) = polygon.first().copied()
    {
        polygon.push(first);
    }
}
