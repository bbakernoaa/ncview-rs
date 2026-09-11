use ncview_rs::data::slice::{Bounds, Slice2D, Validity};
use ncview_rs::data::slice::{PackedAttributes, classify_packed};
use ndarray::Array2;
use std::io::Write;

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
    let source = ncview_rs::data::open("tests/fixtures/regular.nc4").unwrap();
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
    let source = ncview_rs::data::open("tests/fixtures/coards-float32.nc4").unwrap();
    assert_eq!(source.time_label(0).as_deref(), Some("t=0"));
    assert_eq!(source.time_label(1).as_deref(), Some("t=31"));
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
    assert_eq!(variable.dimensions, ["date", "lat", "lon"]);
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
            .find(|dimension| dimension.name == "lat")
            .map(|dimension| dimension.role),
        Some(ncview_rs::data::AxisRole::Latitude)
    );
    assert_eq!(
        source
            .metadata()
            .dimensions
            .iter()
            .find(|dimension| dimension.name == "lon")
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
            Some("lat"),
            Some("date"),
            &[("lon".into(), 2)],
        )
        .unwrap();
    assert_eq!(hovmoller.values.shape(), &[4, 3]);
    assert_eq!(hovmoller.values[(2, 2)], 43.0);
}
