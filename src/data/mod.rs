//! Read-only dataset and slice abstractions.

pub mod coordinates;
pub mod fixtures;
pub mod netcdf4;
pub mod slice;

use std::path::Path;

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetFormat {
    NetCdf4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisRole {
    Time,
    Depth,
    Latitude,
    Longitude,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dimension {
    pub name: String,
    pub length: usize,
    pub role: AxisRole,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    pub name: String,
    pub dimensions: Vec<String>,
    pub numeric: bool,
    pub units: Option<String>,
    pub long_name: Option<String>,
    pub standard_name: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PointCoordinates {
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetMetadata {
    pub path: String,
    pub format: DatasetFormat,
    pub dimensions: Vec<Dimension>,
    pub variables: Vec<Variable>,
}

pub trait DataSource: Send + Sync {
    fn metadata(&self) -> &DatasetMetadata;
    fn read_slice(&self, request: &slice::SliceRequest) -> Result<slice::Slice2D>;

    fn read_slice_on_axes(
        &self,
        request: &slice::SliceRequest,
        _row_axis: Option<&str>,
        _col_axis: Option<&str>,
        _fixed_axes: &[(String, usize)],
    ) -> Result<slice::Slice2D> {
        self.read_slice(request)
    }

    fn time_label(&self, _index: usize) -> Option<String> {
        None
    }

    fn point_coordinates(&self, _variable: &str, _row: usize, _col: usize) -> PointCoordinates {
        PointCoordinates {
            latitude: None,
            longitude: None,
        }
    }
}

pub fn open(path: impl AsRef<Path>) -> Result<Box<dyn DataSource>> {
    let path = path.as_ref();
    netcdf4::NetCdf4Source::open(path).map(|source| Box::new(source) as Box<dyn DataSource>)
}
