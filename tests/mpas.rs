use std::path::Path;

use ncview_rs::data::{
    self, AxisRole,
    bounds::{NumericBounds, resolve_source_bounds_with_feedback},
    slice::{Bounds, SliceRequest},
};
use oxinetcdf::{NcFileWriter, NcType};
use tempfile::tempdir;

#[test]
fn netcdf3_dimension_values_never_return_the_data_field() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mpas-small.nc");
    let source = data::netcdf3::NetCdf3Source::open(&path).expect("open NetCDF-3 fixture");
    assert_eq!(
        data::DataSource::dimension_values(&source, "temperature", "nCells"),
        None
    );
}

struct ClassicVar {
    name: &'static str,
    dimids: Vec<u32>,
    nc_type: u32,
    units: Option<&'static str>,
    vsize: u32,
    record: bool,
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn put_padded(out: &mut Vec<u8>, bytes: &[u8]) {
    put_u32(out, bytes.len() as u32);
    out.extend_from_slice(bytes);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

fn classic_header(vars: &[ClassicVar], begins: &[u32], numrecs: u32) -> Vec<u8> {
    let mut out = b"CDF\x01".to_vec();
    put_u32(&mut out, numrecs);
    put_u32(&mut out, 0x0A);
    put_u32(&mut out, 3);
    for (name, length) in [("Time", 0), ("StrLen", 20), ("nCells", 4)] {
        put_padded(&mut out, name.as_bytes());
        put_u32(&mut out, length);
    }
    put_u32(&mut out, 0);
    put_u32(&mut out, 0);
    put_u32(&mut out, 0x0B);
    put_u32(&mut out, vars.len() as u32);
    for (var, begin) in vars.iter().zip(begins) {
        put_padded(&mut out, var.name.as_bytes());
        put_u32(&mut out, var.dimids.len() as u32);
        for dimid in &var.dimids {
            put_u32(&mut out, *dimid);
        }
        if let Some(units) = var.units {
            put_u32(&mut out, 0x0C);
            put_u32(&mut out, 1);
            put_padded(&mut out, b"units");
            put_u32(&mut out, 2);
            put_padded(&mut out, units.as_bytes());
        } else {
            put_u32(&mut out, 0);
            put_u32(&mut out, 0);
        }
        put_u32(&mut out, var.nc_type);
        put_u32(&mut out, var.vsize);
        put_u32(&mut out, *begin);
    }
    out
}

/// Two-record CDF-1 MPAS file laid out like MPAS diag output (`xtime`, CF `Time`, `q2`).
fn classic_mpas_file(with_time: bool, with_xtime: bool) -> Vec<u8> {
    const CHAR: u32 = 2;
    const FLOAT: u32 = 5;
    const DOUBLE: u32 = 6;
    let mut vars = vec![
        ClassicVar {
            name: "latCell",
            dimids: vec![2],
            nc_type: DOUBLE,
            units: Some("rad"),
            vsize: 32,
            record: false,
        },
        ClassicVar {
            name: "lonCell",
            dimids: vec![2],
            nc_type: DOUBLE,
            units: Some("rad"),
            vsize: 32,
            record: false,
        },
    ];
    if with_xtime {
        vars.push(ClassicVar {
            name: "xtime",
            dimids: vec![0, 1],
            nc_type: CHAR,
            units: Some("YYYY-MM-DD_hh:mm:ss"),
            vsize: 20,
            record: true,
        });
    }
    if with_time {
        vars.push(ClassicVar {
            name: "Time",
            dimids: vec![0],
            nc_type: FLOAT,
            units: Some("seconds since 2010-05-01 00:00:00"),
            vsize: 4,
            record: true,
        });
    }
    vars.push(ClassicVar {
        name: "q2",
        dimids: vec![0, 2],
        nc_type: DOUBLE,
        units: None,
        vsize: 32,
        record: true,
    });

    let mut offset = classic_header(&vars, &vec![0; vars.len()], 2).len() as u32;
    let mut begins = Vec::new();
    for var in vars.iter().filter(|var| !var.record) {
        begins.push(offset);
        offset += var.vsize;
    }
    for var in vars.iter().filter(|var| var.record) {
        begins.push(offset);
        offset += var.vsize;
    }

    let mut out = classic_header(&vars, &begins, 2);
    for value in [-0.5_f64, -0.2, 0.2, 0.5, 0.0, 1.5, 3.0, 4.5] {
        out.extend_from_slice(&value.to_be_bytes());
    }
    for record in 0..2_u32 {
        if with_xtime {
            // xtime is three hours apart so tests can tell it from the CF Time labels.
            let mut text = format!("2010-06-01_{:02}:00:00", record * 3).into_bytes();
            text.resize(20, 0);
            out.extend(text);
        }
        if with_time {
            let seconds = 2_678_400.0_f32 + 21_600.0 * record as f32;
            out.extend_from_slice(&seconds.to_be_bytes());
        }
        for cell in 0..4_u32 {
            out.extend_from_slice(&f64::from(record * 4 + cell).to_be_bytes());
        }
    }
    out
}

fn open_classic_mpas(with_time: bool, with_xtime: bool) -> Box<dyn data::DataSource> {
    let temp_dir = tempdir().unwrap();
    let path = temp_dir.path().join("diag.nc");
    std::fs::write(&path, classic_mpas_file(with_time, with_xtime)).unwrap();
    data::open(&path).expect("open classic MPAS diag file")
}

#[test]
fn classic_fixture_reads_the_selected_record() {
    let source = open_classic_mpas(false, false);
    let slice = source
        .read_slice(&SliceRequest {
            variable: "q2".into(),
            time: 1,
            depth: 0,
            bounds: Bounds::new(0, 720, 0, 1440).unwrap(),
        })
        .expect("read second record");
    let statistics = slice.statistics.expect("finite values");
    assert_eq!((statistics.min, statistics.max), (4.0, 7.0));
}

#[test]
fn cf_time_variable_labels_mpas_time_steps_before_xtime() {
    let source = open_classic_mpas(true, true);
    assert_eq!(
        source.time_label(0).as_deref(),
        Some("2010-06-01T00:00:00Z")
    );
    assert_eq!(
        source.time_label_for_variable("q2", 1).as_deref(),
        Some("2010-06-01T06:00:00Z")
    );
    assert_eq!(source.time_label(2), None);
}

#[test]
fn xtime_labels_mpas_time_steps_without_cf_time() {
    let source = open_classic_mpas(false, true);
    assert_eq!(
        source.time_label(1).as_deref(),
        Some("2010-06-01T03:00:00Z")
    );
}

#[test]
fn mpas_time_steps_without_time_variables_have_no_label() {
    let source = open_classic_mpas(false, false);
    assert_eq!(source.time_label(0), None);
}

#[test]
fn opens_committed_netcdf3_mpas_fixture_and_resamples() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mpas-small.nc");
    let source = data::open(&path).expect("open committed MPAS NetCDF-3 fixture");

    assert_eq!(source.metadata().format, data::DatasetFormat::NetCdf3);
    let bounds = Bounds::new(0, 3, 0, 4).unwrap();
    let slice = source
        .read_slice(&SliceRequest {
            variable: "temperature".into(),
            time: 0,
            depth: 0,
            bounds,
        })
        .expect("resample the MPAS cell field");

    assert_eq!(slice.values.dim(), (3, 4));
    assert_eq!(slice.source_bounds, bounds);
    assert!(slice.values.iter().all(|value| value.is_finite()));
    assert!(slice.coordinates.is_some());
}

#[test]
#[ignore = "requires the downloaded MPAS x1.2562 mesh; set MPAS_MESH"]
fn opens_real_mpas_mesh_and_resamples_coordinate_field() {
    let path = std::env::var("MPAS_MESH").expect("set MPAS_MESH to x1.2562.grid.nc");
    let mesh_source = data::open(&path).expect("open MPAS mesh");
    let metadata = mesh_source.metadata();

    // The x1.2562 mesh has 2562 cells; nCells stays listed alongside the regular grid.
    let n_cells = 2562;
    let latitude = metadata
        .dimensions
        .iter()
        .find(|dimension| dimension.name == "latitude")
        .expect("virtual latitude dimension");
    assert_eq!((latitude.length, latitude.role), (720, AxisRole::Latitude));
    assert!(
        metadata
            .dimensions
            .iter()
            .any(|dimension| dimension.name == "nCells" && dimension.length == n_cells)
    );
    assert!(
        metadata
            .variables
            .iter()
            .any(|variable| variable.name == "latCell")
    );
    assert!(
        metadata
            .variables
            .iter()
            .any(|variable| variable.name == "lonCell")
    );

    let mesh_slice = mesh_source
        .read_slice(&SliceRequest {
            variable: "latCell".into(),
            time: 0,
            depth: 0,
            bounds: Bounds::new(0, 4, 0, 1).unwrap(),
        })
        .expect("read a mesh variable from the native NetCDF-3 source");
    assert!(mesh_slice.values.iter().all(|value| value.is_finite()));

    let temp_dir = tempdir().unwrap();
    let field_path = temp_dir.path().join("field.nc4");
    let mut writer = NcFileWriter::new();
    let n_cells_dimension = writer.def_dim("nCells", n_cells).unwrap();
    let temperature = writer
        .def_var("temperature", &[n_cells_dimension], NcType::Float64)
        .unwrap();
    let values = (0..n_cells)
        .map(|index| 280.0 + index as f64 * 0.01)
        .collect::<Vec<_>>();
    writer.put_var_f64(temperature, &values).unwrap();
    writer.close(&field_path).unwrap();

    let source = data::open_with_grid(&field_path, Some(Path::new(&path)))
        .expect("open MPAS field with external mesh coordinates");
    let field = source.metadata();
    let temperature = field
        .variables
        .iter()
        .find(|variable| variable.name == "temperature")
        .expect("temperature variable");
    assert_eq!(temperature.dimensions, ["latitude", "longitude"]);
    let corner = source.point_coordinates("temperature", 0, 0);
    assert!((corner.latitude.unwrap() + 89.875).abs() < 1e-9);
    assert!((corner.longitude.unwrap() + 179.875).abs() < 1e-9);

    let bounds = Bounds::new(0, 4, 0, 6).unwrap();
    let slice = source
        .read_slice(&SliceRequest {
            variable: "temperature".into(),
            time: 0,
            depth: 0,
            bounds,
        })
        .expect("resample latCell onto the requested window");

    assert_eq!(slice.values.dim(), (4, 6));
    assert_eq!(slice.source_bounds, bounds);
    let coordinates = slice.coordinates.expect("synthetic grid coordinates");
    assert_eq!(coordinates.latitude_axis.as_ref().unwrap().len(), 4);
    assert_eq!(coordinates.longitude_axis.as_ref().unwrap().len(), 6);
    assert!(slice.values.iter().all(|value| value.is_finite()));
    assert!(slice.values.iter().any(|value| *value != 0.0));
}

#[test]
fn offset_window_reports_the_cell_centres_it_samples() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mpas-small.nc");
    let source = data::open(&path).expect("open committed MPAS NetCDF-3 fixture");
    let slice = source
        .read_slice(&SliceRequest {
            variable: "temperature".into(),
            time: 0,
            depth: 0,
            bounds: Bounds::new(10, 13, 20, 24).unwrap(),
        })
        .expect("read offset window");
    let coordinates = slice.coordinates.expect("grid coordinates");
    let latitudes = coordinates.latitude_axis.as_ref().expect("latitude axis");
    let longitudes = coordinates.longitude_axis.as_ref().expect("longitude axis");
    assert_eq!((latitudes.len(), longitudes.len()), (3, 4));
    assert!((latitudes[0] + 87.375).abs() < 1e-9);
    assert!((longitudes[0] + 174.875).abs() < 1e-9);
}

#[test]
fn point_coordinates_name_the_cell_centre_of_a_row_and_column() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mpas-small.nc");
    let source = data::open(&path).expect("open committed MPAS NetCDF-3 fixture");
    let corner = source.point_coordinates("temperature", 0, 0);
    assert!((corner.latitude.unwrap() + 89.875).abs() < 1e-9);
    assert!((corner.longitude.unwrap() + 179.875).abs() < 1e-9);
}

#[test]
fn bounds_menu_resolves_a_lat_lon_box_to_grid_indices() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mpas-small.nc");
    let source = data::open(&path).expect("open committed MPAS NetCDF-3 fixture");
    let (bounds, approximate) = resolve_source_bounds_with_feedback(
        source.as_ref(),
        "temperature",
        "longitude",
        "latitude",
        NumericBounds {
            min_x: 10.0,
            max_x: 20.0,
            min_y: -10.0,
            max_y: 10.0,
        },
    )
    .expect("resolve lat/lon box");
    assert!(!approximate);
    assert_eq!(
        (
            bounds.row_start,
            bounds.row_end,
            bounds.col_start,
            bounds.col_end
        ),
        (320, 400, 760, 800)
    );
}

#[test]
fn fixed_axis_selects_one_plane_of_an_extra_dimension() {
    let temp_dir = tempdir().unwrap();
    let grid_path = temp_dir.path().join("grid.nc4");
    let field_path = temp_dir.path().join("field.nc4");
    let cells = 6;

    let mut grid = NcFileWriter::new();
    let grid_cells = grid.def_dim("nCells", cells).unwrap();
    let lat = grid
        .def_var("latCell", &[grid_cells], NcType::Float64)
        .unwrap();
    let lon = grid
        .def_var("lonCell", &[grid_cells], NcType::Float64)
        .unwrap();
    let latitudes = (0..cells)
        .map(|cell| (-60.0 + 30.0 * cell as f64).to_radians())
        .collect::<Vec<_>>();
    let longitudes = (0..cells)
        .map(|cell| (60.0 * cell as f64).to_radians())
        .collect::<Vec<_>>();
    grid.put_var_f64(lat, &latitudes).unwrap();
    grid.put_var_f64(lon, &longitudes).unwrap();
    grid.close(&grid_path).unwrap();

    let mut field = NcFileWriter::new();
    let kernel = field.def_dim("kernel", 3).unwrap();
    let field_cells = field.def_dim("nCells", cells).unwrap();
    let field_var = field
        .def_var("field", &[kernel, field_cells], NcType::Float64)
        .unwrap();
    let values = (0..3)
        .flat_map(|plane| (0..cells).map(move |cell| 100.0 * plane as f64 + cell as f64))
        .collect::<Vec<_>>();
    field.put_var_f64(field_var, &values).unwrap();
    field.close(&field_path).unwrap();

    let source = data::open_with_grid(&field_path, Some(&grid_path))
        .expect("open field with external mesh coordinates");
    let request = SliceRequest {
        variable: "field".into(),
        time: 0,
        depth: 0,
        bounds: Bounds::new(0, 4, 0, 6).unwrap(),
    };
    assert!(source.read_slice(&request).is_err());
    let slice = source
        .read_slice_on_axes(&request, None, None, &[("kernel".into(), 2)])
        .expect("read the fixed kernel plane");
    let finite = slice
        .values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    assert!(!finite.is_empty());
    assert!(finite.iter().all(|value| (200.0..=205.0).contains(value)));
}
