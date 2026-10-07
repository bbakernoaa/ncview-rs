use ncview_rs::data::slice::{Bounds, Slice2D, Validity};
use ncview_rs::data::slice::{PackedAttributes, classify_packed};
use ndarray::Array2;
use oxinetcdf::{NcFileWriter, NcType, VarOrGroup};
use std::io::Write;

fn regular_fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("regular.nc4");
    let mut writer = NcFileWriter::new();
    let lat = writer.def_dim("lat", 4).unwrap();
    let lon = writer.def_dim("lon", 5).unwrap();
    let field = writer
        .def_var("temperature", &[lat, lon], NcType::Float64)
        .unwrap();
    writer
        .put_att_str(VarOrGroup::Var(field), "units", "K")
        .unwrap();
    writer
        .put_var_f64(
            field,
            &(0..20)
                .map(|index| 250.0 + index as f64 * 0.25)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    writer.close(path).unwrap();
    directory
}

fn coards_fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("coards-float32.nc4");
    let mut writer = NcFileWriter::new();
    // The lightweight writer emits a dimension-scale dataset for every
    // dimension and cannot give that scale the same HDF5 name as a coordinate
    // variable. Keep the scales distinct while retaining explicit coordinate
    // metadata for the reader to resolve.
    let date = writer.def_dim("date_dim", 3).unwrap();
    let lat = writer.def_dim("lat_dim", 4).unwrap();
    let lon = writer.def_dim("lon_dim", 5).unwrap();

    let lon_var = writer.def_var("lon", &[lon], NcType::Float32).unwrap();
    writer
        .put_att_str(VarOrGroup::Var(lon_var), "long_name", "Longitude")
        .unwrap();
    writer
        .put_att_str(VarOrGroup::Var(lon_var), "standard_name", "longitude")
        .unwrap();
    writer
        .put_att_str(VarOrGroup::Var(lon_var), "units", "degrees_east")
        .unwrap();
    writer
        .put_var_f64(lon_var, &[-180.0, -90.0, 0.0, 90.0, 180.0])
        .unwrap();

    let lat_var = writer.def_var("lat", &[lat], NcType::Float32).unwrap();
    writer
        .put_att_str(VarOrGroup::Var(lat_var), "long_name", "Latitude")
        .unwrap();
    writer
        .put_att_str(VarOrGroup::Var(lat_var), "standard_name", "latitude")
        .unwrap();
    writer
        .put_att_str(VarOrGroup::Var(lat_var), "units", "degrees_north")
        .unwrap();
    writer
        .put_var_f64(lat_var, &[-90.0, -30.0, 30.0, 90.0])
        .unwrap();

    let date_var = writer.def_var("date", &[date], NcType::Int32).unwrap();
    writer
        .put_att_str(VarOrGroup::Var(date_var), "long_name", "Time")
        .unwrap();
    writer
        .put_att_str(VarOrGroup::Var(date_var), "standard_name", "time")
        .unwrap();
    writer
        .put_att_str(VarOrGroup::Var(date_var), "units", "days since 2000-01-01")
        .unwrap();
    writer.put_var_i32(date_var, &[0, 31, 60]).unwrap();

    let pixel_area = writer
        .def_var("Pixel_area", &[date, lat, lon], NcType::Float32)
        .unwrap();
    writer
        .put_att_str(VarOrGroup::Var(pixel_area), "long_name", "pixel area")
        .unwrap();
    writer
        .put_att_str(VarOrGroup::Var(pixel_area), "units", "kilometer2")
        .unwrap();
    writer
        .put_var_f64(pixel_area, &(101..161).map(f64::from).collect::<Vec<_>>())
        .unwrap();

    let field = writer
        .def_var("MACCity", &[date, lat, lon], NcType::Float32)
        .unwrap();
    writer
        .put_att_str(
            VarOrGroup::Var(field),
            "long_name",
            "synthetic COARDS float32 field",
        )
        .unwrap();
    writer
        .put_att_str(
            VarOrGroup::Var(field),
            "standard_name",
            "surface_test_field",
        )
        .unwrap();
    writer
        .put_att_str(VarOrGroup::Var(field), "units", "kg m-2 s-1")
        .unwrap();
    writer
        .put_att_str(VarOrGroup::Var(field), "coordinates", "lat lon")
        .unwrap();
    writer
        .put_var_f64(field, &(1..61).map(f64::from).collect::<Vec<_>>())
        .unwrap();
    writer.close(path).unwrap();
    directory
}

fn zero_to_360_fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("zero-to-360.nc4");
    let mut writer = NcFileWriter::new();
    // Keep dimension scales distinct from coordinate variable names, matching
    // the constraint documented for the COARDS fixture above.
    let y = writer.def_dim("latitude_dim", 4).unwrap();
    let x = writer.def_dim("longitude_dim", 5).unwrap();
    let longitude = writer.def_var("longitude", &[x], NcType::Float64).unwrap();
    writer
        .put_att_str(VarOrGroup::Var(longitude), "standard_name", "longitude")
        .unwrap();
    writer
        .put_att_str(VarOrGroup::Var(longitude), "units", "degrees_east")
        .unwrap();
    writer
        .put_var_f64(longitude, &[0.0, 90.0, 180.0, 270.0, 360.0])
        .unwrap();
    let latitude = writer.def_var("latitude", &[y], NcType::Float64).unwrap();
    writer
        .put_att_str(VarOrGroup::Var(latitude), "standard_name", "latitude")
        .unwrap();
    writer
        .put_att_str(VarOrGroup::Var(latitude), "units", "degrees_north")
        .unwrap();
    writer
        .put_var_f64(latitude, &[-90.0, -30.0, 30.0, 90.0])
        .unwrap();
    let field = writer
        .def_var("temperature", &[y, x], NcType::Float64)
        .unwrap();
    writer
        .put_att_str(VarOrGroup::Var(field), "coordinates", "latitude longitude")
        .unwrap();
    writer
        .put_var_f64(field, &(0..20).map(f64::from).collect::<Vec<_>>())
        .unwrap();
    writer.close(path).unwrap();
    directory
}

fn curvilinear_fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/curvilinear.nc4")
}

#[test]
fn bounded_zoom_fixtures_expose_geographic_and_dimension_axes() {
    let signed = coards_fixture();
    let signed_source = ncview_rs::data::open(signed.path().join("coards-float32.nc4")).unwrap();
    assert_eq!(
        signed_source
            .dimension_values("MACCity", "lon_dim")
            .unwrap(),
        [-180.0, -90.0, 0.0, 90.0, 180.0]
    );

    let circular = zero_to_360_fixture();
    let circular_source = ncview_rs::data::open(circular.path().join("zero-to-360.nc4")).unwrap();
    assert_eq!(
        circular_source
            .dimension_values("temperature", "longitude_dim")
            .unwrap(),
        [0.0, 90.0, 180.0, 270.0, 360.0]
    );

    let dimensions = regular_fixture();
    let dimension_source = ncview_rs::data::open(dimensions.path().join("regular.nc4")).unwrap();
    assert_eq!(
        dimension_source
            .dimension_values("temperature", "lat")
            .unwrap(),
        [0.0, 1.0, 2.0, 3.0]
    );
    assert!(curvilinear_fixture_path().is_file());
}

#[test]
fn bounded_zoom_source_axes_resolve_to_expected_half_open_indices() {
    use ncview_rs::data::bounds::{AxisKind, axis_index_range};

    let signed = coards_fixture();
    let signed_source = ncview_rs::data::open(signed.path().join("coards-float32.nc4")).unwrap();
    let longitude = signed_source
        .dimension_values("MACCity", "lon_dim")
        .unwrap();
    let latitude = signed_source
        .dimension_values("MACCity", "lat_dim")
        .unwrap();
    let cols = axis_index_range(-90.0, 90.0, &longitude, AxisKind::Longitude).unwrap();
    let rows = axis_index_range(-30.0, 30.0, &latitude, AxisKind::Latitude).unwrap();
    assert_eq!((rows.start, rows.end, cols.start, cols.end), (1, 3, 1, 4));

    let circular = zero_to_360_fixture();
    let circular_source = ncview_rs::data::open(circular.path().join("zero-to-360.nc4")).unwrap();
    let longitude = circular_source
        .dimension_values("temperature", "longitude_dim")
        .unwrap();
    let latitude = circular_source
        .dimension_values("temperature", "latitude_dim")
        .unwrap();
    let cols = axis_index_range(-90.0, 0.0, &longitude, AxisKind::Longitude).unwrap();
    let rows = axis_index_range(-30.0, 30.0, &latitude, AxisKind::Latitude).unwrap();
    assert_eq!((rows.start, rows.end, cols.start, cols.end), (1, 3, 3, 5));
}

#[test]
fn committed_curvilinear_fixture_produces_a_coordinate_envelope() {
    let source = ncview_rs::data::open(curvilinear_fixture_path()).unwrap();
    let variable = source
        .metadata()
        .variables
        .iter()
        .find(|variable| variable.name == "temperature")
        .unwrap();
    let row_name = variable.dimensions[variable.dimensions.len() - 2].as_str();
    let col_name = variable.dimensions[variable.dimensions.len() - 1].as_str();
    let rows = source
        .metadata()
        .dimensions
        .iter()
        .find(|dimension| dimension.name == row_name)
        .unwrap()
        .length;
    let cols = source
        .metadata()
        .dimensions
        .iter()
        .find(|dimension| dimension.name == col_name)
        .unwrap()
        .length;
    let slice = source
        .read_slice(&ncview_rs::data::slice::SliceRequest {
            variable: "temperature".into(),
            time: 0,
            depth: 0,
            bounds: Bounds::new(0, rows, 0, cols).unwrap(),
        })
        .unwrap();
    let coordinates = slice.coordinates.unwrap();
    let longitude = coordinates.longitude.as_ref().unwrap();
    let latitude = coordinates.latitude.as_ref().unwrap();
    let center = (rows / 2, cols / 2);
    let lon = longitude[center];
    let lat = latitude[center];
    assert!(lon.is_finite() && lat.is_finite());

    let (source_bounds, approximated) =
        ncview_rs::data::bounds::resolve_source_bounds_with_feedback(
            source.as_ref(),
            "temperature",
            col_name,
            row_name,
            ncview_rs::data::bounds::NumericBounds {
                min_x: lon - 0.5,
                max_x: lon + 0.5,
                min_y: lat - 0.5,
                max_y: lat + 0.5,
            },
        )
        .unwrap();
    assert!(approximated);
    assert!(source_bounds.row_start <= center.0 && center.0 < source_bounds.row_end);
    assert!(source_bounds.col_start <= center.1 && center.1 < source_bounds.col_end);

    let envelope =
        ncview_rs::data::bounds::curvilinear_index_bounds(lon, lon, lat, lat, longitude, latitude)
            .unwrap();
    assert!(envelope.row_start <= center.0 && center.0 < envelope.row_end);
    assert!(envelope.col_start <= center.1 && center.1 < envelope.col_end);
}

#[test]
fn bounds_reject_empty_and_overflow() {
    assert!(Bounds::new(0, 0, 0, 1).is_err());
    assert!(
        Bounds::new(0, usize::MAX, 0, 2)
            .unwrap()
            .element_count()
            .is_err()
    );
}

#[test]
fn slice_requires_matching_shapes_and_computes_statistics() {
    let bounds = Bounds::new(0, 2, 0, 2).unwrap();
    let values = Array2::from_shape_vec((2, 2), vec![1.0, 2.0, 9.0, f64::NAN]).unwrap();
    let validity = Array2::from_shape_vec(
        (2, 2),
        vec![
            Validity::Finite,
            Validity::Finite,
            Validity::Fill,
            Validity::NaN,
        ],
    )
    .unwrap();
    let slice = Slice2D::new(values, validity, bounds).unwrap();
    assert_eq!(slice.statistics.unwrap().min, 1.0);
    assert_eq!(slice.statistics.unwrap().max, 2.0);
    assert_eq!(slice.statistics.unwrap().mean, 1.5);
    assert_eq!(slice.value_at_source(0, 1), Some(2.0));
    assert_eq!(slice.value_at_source(1, 0), None);
}

#[test]
fn packed_values_mask_before_unpacking_and_preserve_nonfinite_classes() {
    let (values, mask) = classify_packed(
        &[999.0, -1.0, 2.0, f64::NAN, f64::INFINITY],
        PackedAttributes {
            fill: Some(999.0),
            valid_min: Some(0.0),
            scale_factor: Some(2.0),
            add_offset: Some(1.0),
            ..PackedAttributes::default()
        },
    );
    assert_eq!(
        mask,
        [
            Validity::Fill,
            Validity::InvalidRange,
            Validity::Finite,
            Validity::NaN,
            Validity::PosInf
        ]
    );
    assert_eq!(values[0], 1999.0);
    assert_eq!(values[2], 5.0);
}

#[test]
fn format_boundary_rejects_netcdf3_and_non_hdf5_before_terminal_entry() {
    let directory = tempfile::tempdir().unwrap();
    let netcdf3 = directory.path().join("legacy.nc");
    std::fs::write(&netcdf3, b"CDF\x01unsupported").unwrap();
    let error = match ncview_rs::data::open(&netcdf3) {
        Ok(_) => panic!("legacy file opened"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("NetCDF-3 is unsupported"));

    let hdf5 = directory.path().join("ordinary.h5");
    let mut file = std::fs::File::create(&hdf5).unwrap();
    file.write_all(b"\x89HDF\r\n\x1a\nnot-netcdf").unwrap();
    let error = match ncview_rs::data::open(&hdf5) {
        Ok(_) => panic!("arbitrary HDF5 opened"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains("NetCDF-4") || error.to_string().contains("invalid dataset")
    );
}

#[test]
fn generated_netcdf4_fixture_exposes_metadata_and_slice_values() {
    let fixture = regular_fixture();
    let source = ncview_rs::data::open(fixture.path().join("regular.nc4")).unwrap();
    assert_eq!(
        source.metadata().format,
        ncview_rs::data::DatasetFormat::NetCdf4
    );
    assert!(
        source
            .metadata()
            .variables
            .iter()
            .any(|variable| variable.name == "temperature")
    );
    let request = ncview_rs::data::slice::SliceRequest {
        variable: "temperature".into(),
        time: 0,
        depth: 0,
        bounds: Bounds::new(1, 3, 2, 5).unwrap(),
    };
    let slice = source.read_slice(&request).unwrap();
    assert_eq!(slice.values.shape(), &[2, 3]);
    assert_eq!(slice.values[(0, 0)], 251.75);
}

#[test]
fn coards_float32_time_lat_lon_dataset_reads_a_2d_slice() {
    let fixture = coards_fixture();
    let source = ncview_rs::data::open(fixture.path().join("coards-float32.nc4")).unwrap();
    assert_eq!(
        source.time_label(0).as_deref(),
        Some("2000-01-01T00:00:00Z")
    );
    assert_eq!(
        source.time_label(1).as_deref(),
        Some("2000-02-01T00:00:00Z")
    );
    for name in ["lon", "lat", "date", "MACCity"] {
        assert!(
            source
                .metadata()
                .variables
                .iter()
                .any(|variable| variable.name == name),
            "missing generated COARDS variable {name}"
        );
    }
    let variable = source
        .metadata()
        .variables
        .iter()
        .find(|variable| variable.name == "MACCity")
        .unwrap();
    assert_eq!(variable.dimensions, ["date", "lat_dim", "lon_dim"]);
    assert_eq!(variable.units.as_deref(), Some("kg m-2 s-1"));
    assert_eq!(
        variable.long_name.as_deref(),
        Some("synthetic COARDS float32 field")
    );
    assert_eq!(
        variable.standard_name.as_deref(),
        Some("surface_test_field")
    );
    assert_eq!(
        source
            .metadata()
            .dimensions
            .iter()
            .find(|dimension| dimension.name == "lat_dim")
            .map(|dimension| dimension.role),
        Some(ncview_rs::data::AxisRole::Latitude)
    );
    assert_eq!(
        source
            .metadata()
            .dimensions
            .iter()
            .find(|dimension| dimension.name == "lon_dim")
            .map(|dimension| dimension.role),
        Some(ncview_rs::data::AxisRole::Longitude)
    );
    assert_eq!(variable.dimensions.len(), 3);
    let request = ncview_rs::data::slice::SliceRequest {
        variable: "MACCity".into(),
        time: 0,
        depth: 0,
        bounds: Bounds::new(0, 4, 0, 5).unwrap(),
    };
    let slice = source.read_slice(&request).unwrap();
    assert_eq!(slice.values.shape(), &[4, 5]);
    assert_eq!(slice.values[(0, 0)], 1.0);
    assert_eq!(slice.values[(3, 4)], 20.0);
    assert!(slice.statistics.is_some());
    let coordinates = slice.coordinates.as_ref().expect("COARDS coordinates");
    assert_eq!(
        coordinates.latitude.as_ref().map(|grid| grid[(0, 0)]),
        Some(-90.0)
    );
    assert_eq!(
        coordinates.longitude.as_ref().map(|grid| grid[(0, 4)]),
        Some(180.0)
    );

    let later = source
        .read_slice(&ncview_rs::data::slice::SliceRequest {
            variable: "MACCity".into(),
            time: 2,
            depth: 0,
            bounds: Bounds::new(0, 4, 0, 5).unwrap(),
        })
        .unwrap();
    assert_eq!(later.values[(0, 0)], 41.0);
    assert_eq!(later.values[(3, 4)], 60.0);

    let pixel_area = source
        .read_slice(&ncview_rs::data::slice::SliceRequest {
            variable: "Pixel_area".into(),
            time: 0,
            depth: 0,
            bounds: Bounds::new(0, 4, 0, 5).unwrap(),
        })
        .unwrap();
    assert_eq!(pixel_area.values[(0, 0)], 101.0);
    assert_eq!(pixel_area.values[(3, 4)], 120.0);

    let coordinates = source.point_coordinates("MACCity", 2, 4);
    assert_eq!(coordinates.latitude, Some(30.0));
    assert_eq!(coordinates.longitude, Some(-180.0));

    let hovmoller = source
        .read_slice_on_axes(
            &ncview_rs::data::slice::SliceRequest {
                variable: "MACCity".into(),
                time: 0,
                depth: 0,
                bounds: Bounds::new(0, 4, 0, 3).unwrap(),
            },
            Some("lat_dim"),
            Some("date"),
            &[("lon_dim".into(), 2)],
        )
        .unwrap();
    assert_eq!(hovmoller.values.shape(), &[4, 3]);
    assert_eq!(hovmoller.values[(2, 2)], 43.0);
}

#[test]
fn vertical_labels_are_prefetched_for_every_depth_index() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("levels.nc4");
    let mut writer = NcFileWriter::new();
    let lev = writer.def_dim("lev_dim", 3).unwrap();
    let lat = writer.def_dim("lat_dim", 2).unwrap();
    let lon = writer.def_dim("lon_dim", 2).unwrap();
    let lev_var = writer.def_var("lev", &[lev], NcType::Float64).unwrap();
    writer
        .put_att_str(VarOrGroup::Var(lev_var), "standard_name", "depth")
        .unwrap();
    writer
        .put_att_str(VarOrGroup::Var(lev_var), "units", "m")
        .unwrap();
    writer.put_var_f64(lev_var, &[5.0, 10.0, 20.0]).unwrap();
    let field = writer
        .def_var("temp", &[lev, lat, lon], NcType::Float64)
        .unwrap();
    writer
        .put_att_str(VarOrGroup::Var(field), "coordinates", "lev")
        .unwrap();
    writer
        .put_var_f64(field, &(0..12).map(|i| i as f64).collect::<Vec<_>>())
        .unwrap();
    let mask = writer
        .def_var("mask", &[lat, lon], NcType::Float64)
        .unwrap();
    writer
        .put_var_f64(mask, &(0..4).map(|i| i as f64).collect::<Vec<_>>())
        .unwrap();
    writer.close(&path).unwrap();

    let source = ncview_rs::data::open(&path).unwrap();
    let labels = source.vertical_labels("temp");
    assert_eq!(labels.len(), 3, "got {labels:?}");
    assert!(labels[0].contains('5'), "got {:?}", labels[0]);
    assert!(labels[2].contains("20"), "got {:?}", labels[2]);
    // A variable with no vertical axis yields no labels.
    assert!(source.vertical_labels("mask").is_empty());
}
