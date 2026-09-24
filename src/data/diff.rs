use std::sync::{Arc, atomic::AtomicBool};

use ndarray::Array2;

use crate::error::{NcvError, Result};

use super::{
    DataSource, DatasetMetadata, PointCoordinates,
    slice::{Slice2D, SliceRequest, Validity},
};

pub struct DiffSource {
    source1: Arc<dyn DataSource>,
    source2: Arc<dyn DataSource>,
    metadata: DatasetMetadata,
}

impl DiffSource {
    pub fn new(source1: Arc<dyn DataSource>, source2: Arc<dyn DataSource>) -> Self {
        let meta1 = source1.metadata();
        let meta2 = source2.metadata();

        let path = format!("diff: {} vs {}", meta1.path, meta2.path);

        let mut diff_variables = Vec::new();
        for v1 in &meta1.variables {
            if !v1.numeric || v1.dimensions.len() < 2 {
                continue;
            }
            if let Some(v2) = meta2.variables.iter().find(|v| v.name == v1.name) {
                if v1.dimensions == v2.dimensions
                    && dimensions_lengths_match(meta1, meta2, &v1.dimensions)
                {
                    diff_variables.push(v1.clone());
                }
            }
        }

        let mut diff_dimensions = Vec::new();
        for dim1 in &meta1.dimensions {
            if diff_variables
                .iter()
                .any(|v| v.dimensions.contains(&dim1.name))
            {
                diff_dimensions.push(dim1.clone());
            }
        }

        let metadata = DatasetMetadata {
            path,
            format: meta1.format,
            dimensions: diff_dimensions,
            variables: diff_variables,
        };

        Self {
            source1,
            source2,
            metadata,
        }
    }
}

fn dimensions_lengths_match(
    meta1: &DatasetMetadata,
    meta2: &DatasetMetadata,
    dim_names: &[String],
) -> bool {
    for name in dim_names {
        let len1 = meta1
            .dimensions
            .iter()
            .find(|d| d.name == *name)
            .map(|d| d.length);
        let len2 = meta2
            .dimensions
            .iter()
            .find(|d| d.name == *name)
            .map(|d| d.length);
        if len1.zip(len2).is_none_or(|(l1, l2)| l1 != l2) {
            return false;
        }
    }
    true
}

impl DataSource for DiffSource {
    fn metadata(&self) -> &DatasetMetadata {
        &self.metadata
    }

    fn read_slice(&self, request: &SliceRequest) -> Result<Slice2D> {
        self.read_slice_on_axes_cancellable(
            request,
            None,
            None,
            &[],
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn read_slice_on_axes(
        &self,
        request: &SliceRequest,
        row_axis: Option<&str>,
        col_axis: Option<&str>,
        fixed_axes: &[(String, usize)],
    ) -> Result<Slice2D> {
        self.read_slice_on_axes_cancellable(
            request,
            row_axis,
            col_axis,
            fixed_axes,
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn read_slice_on_axes_cancellable(
        &self,
        request: &SliceRequest,
        row_axis: Option<&str>,
        col_axis: Option<&str>,
        fixed_axes: &[(String, usize)],
        cancelled: Arc<AtomicBool>,
    ) -> Result<Slice2D> {
        let slice1 = self.source1.read_slice_on_axes_cancellable(
            request,
            row_axis,
            col_axis,
            fixed_axes,
            Arc::clone(&cancelled),
        )?;
        let slice2 = self.source2.read_slice_on_axes_cancellable(
            request, row_axis, col_axis, fixed_axes, cancelled,
        )?;

        if slice1.values.dim() != slice2.values.dim() {
            return Err(NcvError::InvalidSlice(
                "slice dimensions differ between sources in diff mode".into(),
            ));
        }

        let (rows, cols) = slice1.values.dim();
        let mut diff_values = Array2::zeros((rows, cols));
        let mut diff_validity = Array2::from_elem((rows, cols), Validity::Missing);

        for row in 0..rows {
            for col in 0..cols {
                let v1 = slice1.values[(row, col)];
                let v2 = slice2.values[(row, col)];
                let m1 = slice1.validity[(row, col)];
                let m2 = slice2.validity[(row, col)];

                if m1 == Validity::Finite && m2 == Validity::Finite && v1.is_finite() && v2.is_finite()
                {
                    diff_values[(row, col)] = v1 - v2;
                    diff_validity[(row, col)] = Validity::Finite;
                } else {
                    diff_validity[(row, col)] = Validity::Missing;
                }
            }
        }

        let mut diff_slice = Slice2D::new(diff_values, diff_validity, request.bounds)?;
        diff_slice.is_diff = true;
        let coords = slice1
            .coordinates
            .or(slice2.coordinates);
        Ok(if let Some(c) = coords {
            diff_slice.with_coordinates(c)
        } else {
            diff_slice
        })
    }

    fn time_label(&self, index: usize) -> Option<String> {
        self.source1.time_label(index)
    }

    fn time_label_for_variable(&self, variable: &str, index: usize) -> Option<String> {
        self.source1.time_label_for_variable(variable, index)
    }

    fn vertical_label(&self, variable: &str, index: usize) -> Option<String> {
        self.source1.vertical_label(variable, index)
    }

    fn vertical_labels(&self, variable: &str) -> Vec<String> {
        self.source1.vertical_labels(variable)
    }

    fn dimension_values(&self, variable: &str, dimension: &str) -> Option<Vec<f64>> {
        self.source1.dimension_values(variable, dimension)
    }

    fn point_coordinates(&self, variable: &str, row: usize, col: usize) -> PointCoordinates {
        self.source1.point_coordinates(variable, row, col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{AxisRole, DatasetFormat, Dimension, Variable};

    struct TestSource {
        metadata: DatasetMetadata,
        data: Array2<f64>,
    }

    impl DataSource for TestSource {
        fn metadata(&self) -> &DatasetMetadata {
            &self.metadata
        }

        fn read_slice(&self, request: &SliceRequest) -> Result<Slice2D> {
            let validity = Array2::from_elem(self.data.dim(), Validity::Finite);
            Slice2D::new(self.data.clone(), validity, request.bounds)
        }
    }

    #[test]
    fn diff_source_filters_unmatched_or_different_dim_variables() {
        let meta1 = DatasetMetadata {
            path: "file1.nc".into(),
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
                    dimensions: vec!["lat".into(), "lon".into()],
                    numeric: true,
                    units: None,
                    long_name: None,
                    standard_name: None,
                },
            ],
        };

        let meta2 = DatasetMetadata {
            path: "file2.nc".into(),
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
                    name: "lon_other".into(),
                    length: 3,
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
                    dimensions: vec!["lat".into(), "lon_other".into()],
                    numeric: true,
                    units: None,
                    long_name: None,
                    standard_name: None,
                },
            ],
        };

        let s1 = Arc::new(TestSource {
            metadata: meta1,
            data: Array2::zeros((2, 2)),
        });
        let s2 = Arc::new(TestSource {
            metadata: meta2,
            data: Array2::zeros((2, 2)),
        });

        let diff = DiffSource::new(s1, s2);
        let vars = &diff.metadata().variables;
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].name, "temp");
    }

    #[test]
    fn diff_source_computes_difference() {
        let meta = DatasetMetadata {
            path: "file.nc".into(),
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
            ],
            variables: vec![Variable {
                name: "temp".into(),
                dimensions: vec!["lat".into(), "lon".into()],
                numeric: true,
                units: None,
                long_name: None,
                standard_name: None,
            }],
        };

        let data1 = Array2::from_shape_vec((2, 2), vec![10.0, 20.0, 30.0, 40.0]).unwrap();
        let data2 = Array2::from_shape_vec((2, 2), vec![1.0, 5.0, 10.0, 50.0]).unwrap();

        let s1 = Arc::new(TestSource {
            metadata: meta.clone(),
            data: data1,
        });
        let s2 = Arc::new(TestSource {
            metadata: meta,
            data: data2,
        });

        let diff = DiffSource::new(s1, s2);
        let req = SliceRequest {
            variable: "temp".into(),
            time: 0,
            depth: 0,
            bounds: crate::data::slice::Bounds::new(0, 2, 0, 2).unwrap(),
        };

        let slice = diff.read_slice(&req).unwrap();
        assert_eq!(
            slice.values,
            Array2::from_shape_vec((2, 2), vec![9.0, 15.0, 20.0, -10.0]).unwrap()
        );
    }
}
