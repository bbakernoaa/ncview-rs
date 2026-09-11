//! Read-only dataset and slice abstractions.

pub mod coordinates;
pub mod fixtures;
pub mod grib2;
pub mod grib2_catalog;
pub mod grib2_identity;
pub mod grib2_index;
pub mod grib2_manifest;
pub mod grib2_types;
pub mod netcdf4;
pub mod slice;

use std::{fs::File, io::Read, path::Path};

use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetFormat {
    NetCdf4,
    Grib2,
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

/// Normalize geographic longitudes to the convention used in the viewer.
/// Non-finite values are preserved so missing coordinate diagnostics are not
/// turned into arbitrary geographic positions.
pub fn normalize_longitude(value: f64) -> f64 {
    if value.is_finite() {
        (value + 180.0).rem_euclid(360.0) - 180.0
    } else {
        value
    }
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
    let extension_matches = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "grib" | "grib2" | "grb" | "grb2"
            )
        });
    let magic_matches = File::open(path)
        .and_then(|mut file| {
            let mut magic = [0_u8; 4];
            file.read_exact(&mut magic).map(|_| magic)
        })
        .is_ok_and(|magic| magic == *b"GRIB");
    if extension_matches || magic_matches {
        grib2::Grib2Source::open(path).map(|source| Box::new(source) as Box<dyn DataSource>)
    } else {
        netcdf4::NetCdf4Source::open(path).map(|source| Box::new(source) as Box<dyn DataSource>)
    }
}
