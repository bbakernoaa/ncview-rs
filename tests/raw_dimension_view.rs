use ncview_rs::data::slice::{Bounds, SliceRequest};
use oxinetcdf::{NcFileWriter, NcType};

fn raw_2d_fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("raw-2d.nc4");
    let mut writer = NcFileWriter::new();
    let rows = writer.def_dim("SAT_Tile_Height", 3).unwrap();
    let columns = writer.def_dim("SAT_Tile_Width", 4).unwrap();
    let field = writer
        .def_var("surface_value", &[rows, columns], NcType::Float64)
        .unwrap();
    writer
        .put_var_f64(
            field,
            &(0..12)
                .map(|index| ((index / 4) * 10 + index % 4) as f64)
                .collect::<Vec<_>>(),
        )
        .unwrap();
    writer.close(&path).unwrap();
    directory
}

fn raw_3d_fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("raw-3d.nc4");
    let mut writer = NcFileWriter::new();
    let rows = writer.def_dim("SAT_Tile_Height", 3).unwrap();
    let columns = writer.def_dim("SAT_Tile_Width", 4).unwrap();
    let kernels = writer.def_dim("Kernel_Num", 3).unwrap();
    let field = writer
        .def_var(
            "BRDF_Parameter_Band1",
            &[rows, columns, kernels],
            NcType::Float64,
        )
        .unwrap();
    writer
        .put_var_f64(
            field,
            &(0..3)
                .flat_map(|row| {
                    (0..4).flat_map(move |col| {
                        (0..3).map(move |kernel| (row * 100 + col * 10 + kernel) as f64)
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
    writer.close(&path).unwrap();
    directory
}

fn raw_4d_fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("raw-4d.nc4");
    let mut writer = NcFileWriter::new();
    let rows = writer.def_dim("SAT_Tile_Height", 2).unwrap();
    let columns = writer.def_dim("SAT_Tile_Width", 3).unwrap();
    let first = writer.def_dim("Retrieval", 2).unwrap();
    let second = writer.def_dim("Kernel_Num", 3).unwrap();
    let field = writer
        .def_var("sample", &[rows, columns, first, second], NcType::Float64)
        .unwrap();
    writer
        .put_var_f64(
            field,
            &(0..2)
                .flat_map(|row| {
                    (0..3).flat_map(move |col| {
                        (0..2).flat_map(move |retrieval| {
                            (0..3).map(move |kernel| {
                                (row * 1000 + col * 100 + retrieval * 10 + kernel) as f64
                            })
                        })
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
    writer.close(&path).unwrap();
    directory
}

fn unsupported_layout_fixture() -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("unsupported-layouts.nc4");
    let mut writer = NcFileWriter::new();
    let pressure = writer.def_dim("pressure", 3).unwrap();
    let empty = writer.def_dim("empty_row", 0).unwrap();
    let _scalar = writer.def_var("scalar", &[], NcType::Float64).unwrap();
    let profile = writer
        .def_var("profile", &[pressure], NcType::Float64)
        .unwrap();
    writer.put_var_f64(profile, &[1.0, 2.0, 3.0]).unwrap();
    let empty_field = writer
        .def_var("empty_field", &[empty, pressure], NcType::Float64)
        .unwrap();
    writer.put_var_f64(empty_field, &[]).unwrap();
    writer.close(&path).unwrap();
    directory
}

#[test]
fn two_dimensional_raw_variable_preserves_named_index_axes_and_values() {
    let fixture = raw_2d_fixture();
    let source = ncview_rs::data::open(fixture.path().join("raw-2d.nc4")).unwrap();
    let variable = source
        .metadata()
        .variables
        .iter()
        .find(|variable| variable.name == "surface_value")
        .unwrap();
    assert_eq!(variable.dimensions, ["SAT_Tile_Height", "SAT_Tile_Width"]);

    let slice = source
        .read_slice(&SliceRequest {
            variable: "surface_value".into(),
            time: 0,
            depth: 0,
            bounds: Bounds::new(0, 3, 0, 4).unwrap(),
        })
        .unwrap();

    assert_eq!(slice.values.shape(), &[3, 4]);
    assert_eq!(slice.values[(0, 0)], 0.0);
    assert_eq!(slice.values[(1, 2)], 12.0);
    assert_eq!(slice.values[(2, 3)], 23.0);
    assert_eq!(slice.value_at_source(2, 3), Some(23.0));
}

#[test]
fn viirs_like_default_plane_reads_height_by_width_and_fixed_kernel_indices() {
    let fixture = raw_3d_fixture();
    let source = ncview_rs::data::open(fixture.path().join("raw-3d.nc4")).unwrap();
    let request = SliceRequest {
        variable: "BRDF_Parameter_Band1".into(),
        time: 0,
        depth: 0,
        bounds: Bounds::new(0, 3, 0, 4).unwrap(),
    };
    let first = source.read_slice(&request).unwrap();
    assert_eq!(first.values.shape(), &[3, 4]);
    assert_eq!(first.value_at_source(2, 3), Some(230.0));

    let final_kernel = source
        .read_slice_on_axes(
            &request,
            Some("SAT_Tile_Height"),
            Some("SAT_Tile_Width"),
            &[("Kernel_Num".into(), 2)],
        )
        .unwrap();
    assert_eq!(final_kernel.value_at_source(0, 0), Some(2.0));
    assert_eq!(final_kernel.value_at_source(2, 3), Some(232.0));
}

#[test]
fn multiple_other_dimensions_are_fixed_independently_and_bounds_checked() {
    let fixture = raw_4d_fixture();
    let source = ncview_rs::data::open(fixture.path().join("raw-4d.nc4")).unwrap();
    let request = SliceRequest {
        variable: "sample".into(),
        time: 0,
        depth: 0,
        bounds: Bounds::new(0, 2, 0, 3).unwrap(),
    };
    let selected = source
        .read_slice_on_axes(
            &request,
            Some("SAT_Tile_Height"),
            Some("SAT_Tile_Width"),
            &[("Retrieval".into(), 1), ("Kernel_Num".into(), 2)],
        )
        .unwrap();
    assert_eq!(selected.value_at_source(0, 0), Some(12.0));
    assert_eq!(selected.value_at_source(1, 2), Some(1212.0));

    let out_of_range = source.read_slice_on_axes(
        &request,
        Some("SAT_Tile_Height"),
        Some("SAT_Tile_Width"),
        &[("Retrieval".into(), 2), ("Kernel_Num".into(), 0)],
    );
    assert!(out_of_range.is_err());
}

#[test]
fn scalar_one_dimensional_and_empty_variables_fail_with_recoverable_errors() {
    let fixture = unsupported_layout_fixture();
    let source = ncview_rs::data::open(fixture.path().join("unsupported-layouts.nc4")).unwrap();
    for name in ["scalar", "profile", "empty_field"] {
        let result = source.read_slice(&SliceRequest {
            variable: name.into(),
            time: 0,
            depth: 0,
            bounds: Bounds::new(0, 1, 0, 1).unwrap(),
        });
        assert!(result.is_err(), "{name} unexpectedly formed a plane");
    }
    let empty = source
        .metadata()
        .dimensions
        .iter()
        .find(|dimension| dimension.name == "empty_row")
        .unwrap();
    assert_eq!(empty.length, 0);
}

#[test]
fn supplied_viirs_fixture_keeps_its_netcdf_dimension_scale_names() {
    let path = std::path::Path::new("VIIRS_BRDF_LSA_NBAR_2025057_h19v19.nc");
    if !path.exists() {
        return;
    }
    let source = ncview_rs::data::open(path).unwrap();
    let expected = ["SAT_Tile_Height", "SAT_Tile_Width", "Kernel_Num"];
    for name in expected {
        assert!(
            source
                .metadata()
                .dimensions
                .iter()
                .any(|dimension| dimension.name == name),
            "missing {name}: {:?}",
            source.metadata().dimensions
        );
    }
    assert!(
        source
            .metadata()
            .dimensions
            .iter()
            .all(|dimension| !dimension.name.starts_with("phony_dim_")),
        "dimensions={:?}; phony_variables={:?}",
        source.metadata().dimensions,
        source
            .metadata()
            .variables
            .iter()
            .filter(|variable| variable
                .dimensions
                .iter()
                .any(|name| name.starts_with("phony_dim_")))
            .map(|variable| (&variable.name, &variable.dimensions))
            .collect::<Vec<_>>()
    );
    let variable = source
        .metadata()
        .variables
        .iter()
        .find(|variable| variable.name == "BRDF_Parameter_Band1")
        .unwrap();
    assert_eq!(variable.dimensions, expected);
    let selected = source
        .read_slice_on_axes(
            &SliceRequest {
                variable: variable.name.clone(),
                time: 0,
                depth: 0,
                bounds: Bounds::new(0, 2, 0, 3).unwrap(),
            },
            Some("SAT_Tile_Height"),
            Some("SAT_Tile_Width"),
            &[("Kernel_Num".into(), 2)],
        )
        .unwrap();
    assert_eq!(selected.values.shape(), &[2, 3]);
}
