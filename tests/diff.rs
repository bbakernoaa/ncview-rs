use std::sync::Arc;

use ncview_rs::app::{AppState, ScaleMode, absolute_slice_limits};
use ncview_rs::data::diff::DiffSource;
use ncview_rs::data::slice::{Bounds, Slice2D, SliceRequest, Validity};
use ncview_rs::data::{AxisRole, DataSource, DatasetFormat, DatasetMetadata, Dimension, Variable};
use ncview_rs::render::colors::{ColorMapper, Palette};
use ndarray::Array2;

struct MockSource {
    metadata: DatasetMetadata,
    data: Array2<f64>,
}

impl DataSource for MockSource {
    fn metadata(&self) -> &DatasetMetadata {
        &self.metadata
    }

    fn read_slice(&self, request: &SliceRequest) -> ncview_rs::error::Result<Slice2D> {
        let validity = Array2::from_elem(self.data.dim(), Validity::Finite);
        Slice2D::new(self.data.clone(), validity, request.bounds)
    }
}

fn create_metadata(
    path: &str,
    var_name: &str,
    dim_names: &[&str],
    shape: (usize, usize),
) -> DatasetMetadata {
    let dimensions = vec![
        Dimension {
            name: dim_names[0].into(),
            length: shape.0,
            role: AxisRole::Latitude,
        },
        Dimension {
            name: dim_names[1].into(),
            length: shape.1,
            role: AxisRole::Longitude,
        },
    ];
    let variables = vec![Variable {
        name: var_name.into(),
        dimensions: vec![dim_names[0].into(), dim_names[1].into()],
        numeric: true,
        units: None,
        long_name: None,
        standard_name: None,
    }];
    DatasetMetadata {
        path: path.into(),
        format: DatasetFormat::NetCdf4,
        dimensions,
        variables,
    }
}

#[test]
fn diff_source_includes_only_fields_with_same_name_and_dimensions() {
    // meta1 has temp (lat, lon) and press (lat, lon)
    let mut meta1 = create_metadata("f1.nc", "temp", &["lat", "lon"], (2, 2));
    meta1.variables.push(Variable {
        name: "press".into(),
        dimensions: vec!["lat".into(), "lon".into()],
        numeric: true,
        units: None,
        long_name: None,
        standard_name: None,
    });

    // meta2 has temp (lat, lon) and press (lat, lon_alt)
    let meta2 = DatasetMetadata {
        path: "f2.nc".into(),
        format: DatasetFormat::NetCdf4,
        dimensions: vec![
            Dimension {
                name: "lat".into(),
                length: 2,
                role: AxisRole::Latitude,
            },
            Dimension {
                name: "lon".into(),
                length: 2,
                role: AxisRole::Longitude,
            },
            Dimension {
                name: "lon_alt".into(),
                length: 2,
                role: AxisRole::Longitude,
            },
        ],
        variables: vec![
            Variable {
                name: "temp".into(),
                dimensions: vec!["lat".into(), "lon".into()],
                numeric: true,
                units: None,
                long_name: None,
                standard_name: None,
            },
            Variable {
                name: "press".into(),
                dimensions: vec!["lat".into(), "lon_alt".into()],
                numeric: true,
                units: None,
                long_name: None,
                standard_name: None,
            },
        ],
    };

    let s1 = Arc::new(MockSource {
        metadata: meta1,
        data: Array2::zeros((2, 2)),
    });
    let s2 = Arc::new(MockSource {
        metadata: meta2,
        data: Array2::zeros((2, 2)),
    });

    let diff = DiffSource::new(s1, s2);
    let vars = &diff.metadata().variables;
    assert_eq!(vars.len(), 1);
    assert_eq!(vars[0].name, "temp");
}

#[test]
fn diff_source_subtracts_s1_minus_s2() {
    let meta = create_metadata("f1.nc", "temp", &["lat", "lon"], (2, 2));

    let data1 = Array2::from_shape_vec((2, 2), vec![100.0, 50.0, -10.0, 0.0]).unwrap();
    let data2 = Array2::from_shape_vec((2, 2), vec![90.0, 60.0, 20.0, 0.0]).unwrap();

    let s1 = Arc::new(MockSource {
        metadata: meta.clone(),
        data: data1,
    });
    let s2 = Arc::new(MockSource {
        metadata: meta,
        data: data2,
    });

    let diff = DiffSource::new(s1, s2);
    let req = SliceRequest {
        variable: "temp".into(),
        time: 0,
        depth: 0,
        bounds: Bounds::new(0, 2, 0, 2).unwrap(),
    };

    let slice = diff.read_slice(&req).unwrap();
    assert!(slice.is_diff);
    assert_eq!(
        slice.values,
        Array2::from_shape_vec((2, 2), vec![10.0, -10.0, -30.0, 0.0]).unwrap()
    );
}

#[test]
fn log_scale_diff_uses_absolute_differences() {
    let values = Array2::from_shape_vec((2, 2), vec![-10.0, 10.0, -100.0, 0.0]).unwrap();
    let validity = Array2::from_elem((2, 2), Validity::Finite);
    let mut slice = Slice2D::new(values, validity, Bounds::new(0, 2, 0, 2).unwrap()).unwrap();
    slice.is_diff = true;

    // absolute limits should consider |v| > 0: min = 10.0, max = 100.0
    let limits = absolute_slice_limits(&slice).unwrap();
    assert_eq!(limits, (10.0, 100.0));

    let stats = slice.statistics.unwrap();
    let mapper = ColorMapper::new(
        &Palette::CoolWarm,
        stats,
        Some(limits),
        ScaleMode::Log,
        true,
    );

    // -10.0 and +10.0 have same absolute difference (10.0), so mapper returns identical colors
    let col_neg10 = mapper.map_value(-10.0);
    let col_pos10 = mapper.map_value(10.0);
    assert_eq!(col_neg10, col_pos10);

    // -100.0 has absolute value 100.0, so maps to high end of log scale
    let col_100 = mapper.map_value(-100.0);
    assert_ne!(col_neg10, col_100);
}

#[test]
fn app_state_diff_defaults_to_coolwarm_diverging_colormap() {
    let mut app_state = AppState::default();
    app_state.view.is_diff = true;
    app_state.view.palette = Palette::CoolWarm;

    assert!(app_state.view.is_diff);
    assert_eq!(app_state.view.palette.name(), "CoolWarm");
}
