use std::fs;
use tempfile::tempdir;

use ncview_rs::data::{
    AxisRole, DataSource, DatasetFormat,
    manifest::{ManifestSource, is_manifest_file},
    open,
    slice::{Bounds, SliceRequest},
};

#[test]
fn opens_virtualizarr_kerchunk_manifest() {
    let dir = tempdir().unwrap();
    let manifest_path = dir.path().join("virtualizarr_manifest.json");

    let json_content = serde_json::json!({
        "version": 1,
        "refs": {
            ".zgroup": { "zarr_format": 2 },
            ".zattrs": {
                "ncv_manifest_profile": "virtualizarr-kerchunk-v1",
                "Conventions": "CF-1.7"
            },
            "temperature/.zarray": {
                "zarr_format": 2,
                "shape": [2, 3],
                "chunks": [2, 3],
                "dtype": "<f4",
                "compressor": null,
                "filters": null,
                "fill_value": null
            },
            "temperature/.zattrs": {
                "_ARRAY_DIMENSIONS": ["latitude", "longitude"],
                "units": "K",
                "long_name": "Surface Temperature"
            },
            "temperature/0.0": "base64:A...AAAA" // 6 floats inline
        }
    });

    fs::write(&manifest_path, json_content.to_string()).unwrap();

    assert!(is_manifest_file(&manifest_path));

    let source = open(&manifest_path).unwrap();
    let metadata = source.metadata();

    assert_eq!(metadata.format, DatasetFormat::VirtualManifest);
    assert_eq!(metadata.variables.len(), 1);

    let var = &metadata.variables[0];
    assert_eq!(var.name, "temperature");
    assert_eq!(var.dimensions, vec!["latitude", "longitude"]);
    assert_eq!(var.units.as_deref(), Some("K"));
    assert_eq!(var.long_name.as_deref(), Some("Surface Temperature"));

    let lat_dim = metadata
        .dimensions
        .iter()
        .find(|d| d.name == "latitude")
        .unwrap();
    assert_eq!(lat_dim.length, 2);
    assert_eq!(lat_dim.role, AxisRole::Latitude);

    let lon_dim = metadata
        .dimensions
        .iter()
        .find(|d| d.name == "longitude")
        .unwrap();
    assert_eq!(lon_dim.length, 3);
    assert_eq!(lon_dim.role, AxisRole::Longitude);
}

#[test]
fn opens_icechunk_virtual_store_manifest() {
    let dir = tempdir().unwrap();
    let manifest_path = dir.path().join("icechunk_manifest.json");

    // Sample raw binary float data in a backing file
    let backing_file = dir.path().join("data.bin");
    let sample_floats: [f32; 6] = [10.0, 20.0, 30.0, 40.0, 50.0, 60.0];
    let bytes: Vec<u8> = sample_floats.iter().flat_map(|f| f.to_le_bytes()).collect();
    fs::write(&backing_file, &bytes).unwrap();

    let json_content = serde_json::json!({
        "type": "icechunk",
        "manifest_format": 1,
        "refs": {
            "tasmax/.zarray": {
                "zarr_format": 2,
                "shape": [2, 3],
                "chunks": [2, 3],
                "dtype": "<f4",
                "compressor": null,
                "filters": null,
                "fill_value": null
            },
            "tasmax/.zattrs": {
                "_ARRAY_DIMENSIONS": ["lat", "lon"],
                "units": "degC"
            },
            "tasmax/0/0": {
                "path": backing_file.file_name().unwrap().to_str().unwrap(),
                "offset": 0,
                "length": bytes.len()
            }
        }
    });

    fs::write(&manifest_path, json_content.to_string()).unwrap();

    let source = ManifestSource::open(&manifest_path).unwrap();
    let metadata = source.metadata();

    assert_eq!(metadata.variables.len(), 1);
    assert_eq!(metadata.variables[0].name, "tasmax");

    let request = SliceRequest {
        variable: "tasmax".into(),
        time: 0,
        depth: 0,
        bounds: Bounds::new(0, 2, 0, 3).unwrap(),
    };

    let slice = source.read_slice(&request).unwrap();
    assert_eq!(slice.values[(0, 0)], 10.0);
    assert_eq!(slice.values[(0, 1)], 20.0);
    assert_eq!(slice.values[(1, 2)], 60.0);

    let stats = slice.statistics.unwrap();
    assert_eq!(stats.min, 10.0);
    assert_eq!(stats.max, 60.0);
    assert_eq!(stats.finite_count, 6);
}

#[test]
fn handles_subregion_slicing_and_strides() {
    let dir = tempdir().unwrap();
    let manifest_path = dir.path().join("subregion_manifest.json");

    // 4x4 array of floats
    let backing_file = dir.path().join("grid.bin");
    let grid_data: Vec<f32> = (1..=16).map(|v| v as f32).collect();
    let bytes: Vec<u8> = grid_data.iter().flat_map(|f| f.to_le_bytes()).collect();
    fs::write(&backing_file, &bytes).unwrap();

    let json_content = serde_json::json!({
        "refs": {
            "field/.zarray": {
                "zarr_format": 2,
                "shape": [4, 4],
                "chunks": [4, 4],
                "dtype": "<f4"
            },
            "field/.zattrs": {
                "_ARRAY_DIMENSIONS": ["lat", "lon"]
            },
            "field/0.0": {
                "path": backing_file.file_name().unwrap().to_str().unwrap(),
                "offset": 0,
                "length": bytes.len()
            }
        }
    });
    fs::write(&manifest_path, json_content.to_string()).unwrap();

    let source = ManifestSource::open(&manifest_path).unwrap();

    // Request sub-region rows 1..3, cols 1..3 (2x2 subgrid)
    let request = SliceRequest {
        variable: "field".into(),
        time: 0,
        depth: 0,
        bounds: Bounds::new(1, 3, 1, 3).unwrap(),
    };

    let slice = source.read_slice(&request).unwrap();

    assert_eq!(slice.values[(0, 0)], 6.0);
    assert_eq!(slice.values[(0, 1)], 7.0);
    assert_eq!(slice.values[(1, 0)], 10.0);
    assert_eq!(slice.values[(1, 1)], 11.0);
}

#[test]
fn handles_big_endian_and_integer_dtypes() {
    let dir = tempdir().unwrap();
    let manifest_path = dir.path().join("dtypes_manifest.json");

    let backing_file = dir.path().join("be_integers.bin");
    let sample_ints: [i16; 4] = [100, 200, 300, 400];
    let bytes: Vec<u8> = sample_ints.iter().flat_map(|i| i.to_be_bytes()).collect();
    fs::write(&backing_file, &bytes).unwrap();

    let json_content = serde_json::json!({
        "refs": {
            "pressure/.zarray": {
                "zarr_format": 2,
                "shape": [2, 2],
                "chunks": [2, 2],
                "dtype": ">i2"
            },
            "pressure/.zattrs": {
                "_ARRAY_DIMENSIONS": ["y", "x"]
            },
            "pressure/0.0": {
                "path": backing_file.file_name().unwrap().to_str().unwrap(),
                "offset": 0,
                "length": bytes.len()
            }
        }
    });
    fs::write(&manifest_path, json_content.to_string()).unwrap();

    let source = ManifestSource::open(&manifest_path).unwrap();
    let request = SliceRequest {
        variable: "pressure".into(),
        time: 0,
        depth: 0,
        bounds: Bounds::new(0, 2, 0, 2).unwrap(),
    };

    let slice = source.read_slice(&request).unwrap();
    assert_eq!(slice.values[(0, 0)], 100.0);
    assert_eq!(slice.values[(0, 1)], 200.0);
    assert_eq!(slice.values[(1, 0)], 300.0);
    assert_eq!(slice.values[(1, 1)], 400.0);
}

#[test]
fn handles_multi_chunk_spanning_and_missing_chunks() {
    let dir = tempdir().unwrap();
    let manifest_path = dir.path().join("multichunk_manifest.json");

    let chunk00 = [1.0f32, 2.0, 3.0, 4.0];
    let bytes00: Vec<u8> = chunk00.iter().flat_map(|f| f.to_le_bytes()).collect();
    let file00 = dir.path().join("c00.bin");
    fs::write(&file00, &bytes00).unwrap();

    let chunk01 = [5.0f32, 6.0, 7.0, 8.0];
    let bytes01: Vec<u8> = chunk01.iter().flat_map(|f| f.to_le_bytes()).collect();
    let file01 = dir.path().join("c01.bin");
    fs::write(&file01, &bytes01).unwrap();

    let chunk11 = [13.0f32, 14.0, 15.0, 16.0];
    let bytes11: Vec<u8> = chunk11.iter().flat_map(|f| f.to_le_bytes()).collect();
    let file11 = dir.path().join("c11.bin");
    fs::write(&file11, &bytes11).unwrap();

    let json_content = serde_json::json!({
        "refs": {
            "grid/.zarray": {
                "zarr_format": 2,
                "shape": [4, 4],
                "chunks": [2, 2],
                "dtype": "<f4",
                "fill_value": -999.0
            },
            "grid/.zattrs": {
                "_ARRAY_DIMENSIONS": ["lat", "lon"]
            },
            "grid/0.0": { "path": file00.file_name().unwrap().to_str().unwrap(), "offset": 0, "length": bytes00.len() },
            "grid/0.1": { "path": file01.file_name().unwrap().to_str().unwrap(), "offset": 0, "length": bytes01.len() },
            "grid/1.1": { "path": file11.file_name().unwrap().to_str().unwrap(), "offset": 0, "length": bytes11.len() }
        }
    });
    fs::write(&manifest_path, json_content.to_string()).unwrap();

    let source = ManifestSource::open(&manifest_path).unwrap();

    let request = SliceRequest {
        variable: "grid".into(),
        time: 0,
        depth: 0,
        bounds: Bounds::new(0, 4, 0, 4).unwrap(),
    };

    let slice = source.read_slice(&request).unwrap();

    assert_eq!(slice.values[(0, 0)], 1.0);
    assert_eq!(slice.values[(0, 1)], 2.0);
    assert_eq!(slice.values[(0, 2)], 5.0);
    assert_eq!(slice.values[(0, 3)], 6.0);

    assert_eq!(slice.values[(1, 0)], 3.0);
    assert_eq!(slice.values[(1, 1)], 4.0);
    assert_eq!(slice.values[(1, 2)], 7.0);
    assert_eq!(slice.values[(1, 3)], 8.0);

    assert_eq!(slice.values[(2, 0)], -999.0);
    assert_eq!(slice.values[(2, 1)], -999.0);
    assert_eq!(slice.values[(2, 2)], 13.0);
    assert_eq!(slice.values[(2, 3)], 14.0);

    assert_eq!(slice.values[(3, 0)], -999.0);
    assert_eq!(slice.values[(3, 1)], -999.0);
    assert_eq!(slice.values[(3, 2)], 15.0);
    assert_eq!(slice.values[(3, 3)], 16.0);
}
