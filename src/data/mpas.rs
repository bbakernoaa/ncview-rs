use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use ndarray::Array2;

use crate::analysis::projection::ProjectionIndex;
use crate::error::{NcvError, Result};

use super::{
    AxisRole, DataSource, DatasetMetadata, Dimension, PointCoordinates, Variable,
    netcdf3::NetCdf3Source,
    netcdf4::NetCdf4Source,
    normalize_longitude,
    slice::{CoordinateGrid, Slice2D, SliceRequest, Validity},
};

/// Spacing of the regular latitude/longitude grid that mesh fields are sampled onto.
pub const DEFAULT_RESOLUTION_DEG: f64 = 0.25;

const LATITUDE_DIMENSION: &str = "latitude";
const LONGITUDE_DIMENSION: &str = "longitude";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeshLocation {
    Cell,
    Vertex,
}

impl MeshLocation {
    pub fn dimension_name(self) -> &'static str {
        match self {
            Self::Cell => "nCells",
            Self::Vertex => "nVertices",
        }
    }

    pub fn coordinate_names(self) -> (&'static str, &'static str) {
        match self {
            Self::Cell => ("latCell", "lonCell"),
            Self::Vertex => ("latVertex", "lonVertex"),
        }
    }
}

/// Indices for the non-mesh dimensions of a variable.
#[derive(Debug, Clone, Default)]
pub(crate) struct Selection {
    pub time: usize,
    pub depth: usize,
    pub fixed: Vec<(String, usize)>,
}

pub struct MpasSource {
    inner: Box<dyn MeshValueSource>,
    grid: Option<Box<dyn MeshValueSource>>,
    metadata: DatasetMetadata,
    path: PathBuf,
    resolution_deg: f64,
    nearest: Mutex<HashMap<MeshLocation, Arc<NearestMap>>>,
}

/// For each regular-grid cell, the flat index of the nearest mesh point (`usize::MAX` if none).
struct NearestMap {
    mesh_length: usize,
    nearest: Vec<usize>,
}

trait MeshValueSource: DataSource {
    fn read_variable_values(&self, variable: &str) -> Result<Vec<f64>>;

    /// Values along `mesh_dimension`, with every other dimension fixed by `selection`.
    fn read_mesh_values(
        &self,
        variable: &str,
        mesh_dimension: &str,
        selection: &Selection,
    ) -> Result<Vec<f64>>;
}

impl MeshValueSource for NetCdf4Source {
    fn read_variable_values(&self, variable: &str) -> Result<Vec<f64>> {
        NetCdf4Source::read_variable_values(self, variable)
    }

    fn read_mesh_values(
        &self,
        variable: &str,
        mesh_dimension: &str,
        selection: &Selection,
    ) -> Result<Vec<f64>> {
        let values = NetCdf4Source::read_variable_values(self, variable)?;
        select_mesh_values(
            &values,
            self.metadata(),
            variable,
            mesh_dimension,
            selection,
        )
    }
}

impl MeshValueSource for NetCdf3Source {
    fn read_variable_values(&self, variable: &str) -> Result<Vec<f64>> {
        NetCdf3Source::read_variable_values(self, variable)
    }

    fn read_mesh_values(
        &self,
        variable: &str,
        mesh_dimension: &str,
        selection: &Selection,
    ) -> Result<Vec<f64>> {
        NetCdf3Source::read_mesh_values(self, variable, mesh_dimension, selection)
    }
}

fn radians_to_degrees(value: f64) -> f64 {
    value * 180.0 / std::f64::consts::PI
}

/// Convert coordinate values to degrees, treating angles without degree units as radians.
fn to_degrees(values: Vec<f64>, units: Option<&str>) -> Vec<f64> {
    let already_degrees =
        units.is_some_and(|units| units.to_ascii_lowercase().starts_with("degree"));
    if already_degrees {
        values
    } else {
        values.into_iter().map(radians_to_degrees).collect()
    }
}

fn shape_for_resolution(resolution_deg: f64) -> (usize, usize) {
    (
        (180.0 / resolution_deg).round() as usize,
        (360.0 / resolution_deg).round() as usize,
    )
}

impl MpasSource {
    pub fn open(path: &Path, grid_path: Option<&Path>) -> Result<Box<dyn DataSource>> {
        let source = open_mesh_source(path)?;
        Self::from_source(path, source, grid_path)
    }

    fn from_source(
        path: &Path,
        source: Box<dyn MeshValueSource>,
        grid_path: Option<&Path>,
    ) -> Result<Box<dyn DataSource>> {
        let inner_metadata = source.metadata().clone();
        let mesh = detect(&inner_metadata).ok_or_else(|| NcvError::InvalidDataset {
            path: path.to_path_buf(),
            reason: format!(
                "MPAS mesh detection failed for {}; expected nCells or nVertices metadata",
                path.display()
            ),
        })?;

        let grid = if local_coordinates_present(&inner_metadata) {
            None
        } else {
            let grid_path = grid_path.ok_or_else(|| NcvError::InvalidDataset {
                path: path.to_path_buf(),
                reason: format!(
                    "MPAS mesh detected ({} present) but lat/lon coordinates are not in this file; pass the init or static file with --grid <PATH>",
                    mesh.dimension_name()
                ),
            })?;
            Some(open_mesh_source(grid_path)?)
        };

        let (latitude_length, longitude_length) = shape_for_resolution(DEFAULT_RESOLUTION_DEG);
        Ok(Box::new(Self {
            metadata: virtual_metadata(&inner_metadata, latitude_length, longitude_length),
            inner: source,
            grid,
            path: path.to_path_buf(),
            resolution_deg: DEFAULT_RESOLUTION_DEG,
            nearest: Mutex::new(HashMap::new()),
        }) as Box<dyn DataSource>)
    }

    fn grid_shape(&self) -> (usize, usize) {
        shape_for_resolution(self.resolution_deg)
    }

    fn grid_latitude(&self, row: usize) -> f64 {
        -90.0 + (row as f64 + 0.5) * self.resolution_deg
    }

    fn grid_longitude(&self, col: usize) -> f64 {
        normalize_longitude(-180.0 + (col as f64 + 0.5) * self.resolution_deg)
    }

    fn coordinate_source(&self) -> &dyn MeshValueSource {
        self.grid.as_deref().unwrap_or(self.inner.as_ref())
    }

    fn read_mesh_coordinates(&self, mesh: MeshLocation) -> Result<(Vec<f64>, Vec<f64>)> {
        let (lat_name, lon_name) = mesh.coordinate_names();
        let source = self.coordinate_source();
        let units = |name: &str| {
            source
                .metadata()
                .variables
                .iter()
                .find(|variable| variable.name == name)
                .and_then(|variable| variable.units.clone())
        };
        let lat_units = units(lat_name);
        let lon_units = units(lon_name);
        let lat = to_degrees(source.read_variable_values(lat_name)?, lat_units.as_deref());
        let lon = to_degrees(source.read_variable_values(lon_name)?, lon_units.as_deref());
        if lat.len() != lon.len() {
            return Err(NcvError::InvalidDataset {
                path: self.path.clone(),
                reason: format!(
                    "MPAS mesh coordinate lengths differ: {} lat values vs {} lon values",
                    lat.len(),
                    lon.len()
                ),
            });
        }
        // Non-finite entries are kept so mesh indices stay aligned with the data values.
        let lon = lon.into_iter().map(normalize_longitude).collect();
        Ok((lat, lon))
    }

    fn nearest_map(&self, mesh: MeshLocation) -> Result<Arc<NearestMap>> {
        let mut cache = self.nearest.lock().map_err(|_| NcvError::Adapter {
            path: self.path.clone(),
            reason: "MPAS nearest-point cache lock was poisoned".into(),
        })?;
        if let Some(map) = cache.get(&mesh) {
            return Ok(Arc::clone(map));
        }
        let (lat, lon) = self.read_mesh_coordinates(mesh)?;
        let cols = lat.len().max(1);
        let index = ProjectionIndex::build(&lat, &lon, cols);
        let (rows, grid_cols) = self.grid_shape();
        let mut nearest = Vec::with_capacity(rows * grid_cols);
        for row in 0..rows {
            let latitude = self.grid_latitude(row);
            for col in 0..grid_cols {
                let longitude = self.grid_longitude(col);
                nearest.push(
                    index
                        .nearest(latitude, longitude)
                        .map_or(usize::MAX, |source| source.row * cols + source.col),
                );
            }
        }
        let map = Arc::new(NearestMap {
            mesh_length: lat.len(),
            nearest,
        });
        cache.insert(mesh, Arc::clone(&map));
        Ok(map)
    }

    fn mesh_for_variable(&self, variable: &str) -> Option<MeshLocation> {
        let item = self
            .inner
            .metadata()
            .variables
            .iter()
            .find(|item| item.name == variable)?;
        [MeshLocation::Cell, MeshLocation::Vertex]
            .into_iter()
            .find(|mesh| {
                item.dimensions
                    .iter()
                    .any(|name| name == mesh.dimension_name())
            })
    }

    fn read_window(
        &self,
        request: &SliceRequest,
        fixed_axes: &[(String, usize)],
    ) -> Result<Slice2D> {
        let Some(mesh) = self.mesh_for_variable(&request.variable) else {
            return self
                .inner
                .read_slice_on_axes(request, None, None, fixed_axes);
        };
        let selection = Selection {
            time: request.time,
            depth: request.depth,
            fixed: fixed_axes.to_vec(),
        };
        let mesh_values =
            self.inner
                .read_mesh_values(&request.variable, mesh.dimension_name(), &selection)?;
        let map = self.nearest_map(mesh)?;
        if mesh_values.len() != map.mesh_length {
            return Err(NcvError::InvalidDataset {
                path: self.path.clone(),
                reason: format!(
                    "MPAS mesh length mismatch for {}: {} coordinate values, {} data values",
                    request.variable,
                    map.mesh_length,
                    mesh_values.len()
                ),
            });
        }

        let (grid_rows, grid_cols) = self.grid_shape();
        let bounds = request.bounds;
        if bounds.row_end > grid_rows || bounds.col_end > grid_cols {
            return Err(NcvError::InvalidSlice(
                "requested window exceeds the MPAS regular grid".into(),
            ));
        }
        let rows = bounds.row_end - bounds.row_start;
        let cols = bounds.col_end - bounds.col_start;
        let values = Array2::from_shape_fn((rows, cols), |(row, col)| {
            let cell = (bounds.row_start + row) * grid_cols + bounds.col_start + col;
            let index = map.nearest[cell];
            if index == usize::MAX {
                f64::NAN
            } else {
                mesh_values[index]
            }
        });
        let validity = Array2::from_shape_fn((rows, cols), |(row, col)| {
            if values[(row, col)].is_finite() {
                Validity::Finite
            } else {
                Validity::NaN
            }
        });
        let latitude_axis = (bounds.row_start..bounds.row_end)
            .map(|row| self.grid_latitude(row))
            .collect();
        let longitude_axis = (bounds.col_start..bounds.col_end)
            .map(|col| self.grid_longitude(col))
            .collect();
        Slice2D::new(values, validity, bounds).map(|slice| {
            slice.with_coordinates(CoordinateGrid {
                latitude: None,
                longitude: None,
                latitude_axis: Some(latitude_axis),
                longitude_axis: Some(longitude_axis),
            })
        })
    }
}

/// Index along a non-mesh dimension: an explicit fixed index, else the time/depth role, else a singleton.
pub(crate) fn dimension_index(
    variable: &str,
    name: &str,
    length: usize,
    selection: &Selection,
) -> Result<usize> {
    let index = if let Some((_, index)) = selection
        .fixed
        .iter()
        .find(|(fixed, _)| fixed.as_str() == name)
    {
        *index
    } else if is_time_dimension(name) {
        selection.time
    } else if is_vertical_dimension(name) {
        selection.depth
    } else if length == 1 {
        0
    } else {
        return Err(NcvError::UnsupportedVariable {
            variable: variable.to_owned(),
            reason: format!("non-spatial dimension '{name}' needs a time, depth, or fixed index"),
        });
    };
    if index >= length {
        return Err(NcvError::InvalidSlice(format!(
            "index {index} exceeds dimension '{name}' length {length}"
        )));
    }
    Ok(index)
}

pub(crate) fn select_mesh_values(
    values: &[f64],
    metadata: &DatasetMetadata,
    variable: &str,
    mesh_dimension: &str,
    selection: &Selection,
) -> Result<Vec<f64>> {
    let variable_metadata = metadata
        .variables
        .iter()
        .find(|item| item.name == variable)
        .ok_or_else(|| NcvError::UnsupportedVariable {
            variable: variable.to_owned(),
            reason: "variable not found".into(),
        })?;
    let dimensions = variable_metadata
        .dimensions
        .iter()
        .map(|name| {
            let length = metadata
                .dimensions
                .iter()
                .find(|dimension| dimension.name == *name)
                .map(|dimension| dimension.length)
                .ok_or_else(|| NcvError::InvalidDataset {
                    path: metadata.path.clone().into(),
                    reason: format!("variable '{variable}' references unknown dimension '{name}'"),
                })?;
            Ok((name.as_str(), length))
        })
        .collect::<Result<Vec<_>>>()?;
    let expected = dimensions
        .iter()
        .try_fold(1_usize, |product, (_, length)| product.checked_mul(*length))
        .ok_or_else(|| NcvError::InvalidSlice("MPAS variable shape overflows usize".into()))?;
    if expected != values.len() {
        return Err(NcvError::Adapter {
            path: metadata.path.clone().into(),
            reason: format!(
                "variable '{variable}' returned {} values, expected {expected}",
                values.len()
            ),
        });
    }
    let mesh_axis = dimensions
        .iter()
        .position(|(name, _)| *name == mesh_dimension)
        .ok_or_else(|| NcvError::UnsupportedVariable {
            variable: variable.to_owned(),
            reason: format!("variable does not use mesh dimension '{mesh_dimension}'"),
        })?;
    let strides = (0..dimensions.len())
        .map(|axis| {
            dimensions[axis + 1..]
                .iter()
                .map(|(_, length)| length)
                .product::<usize>()
        })
        .collect::<Vec<_>>();
    let mut base = 0;
    for (axis, (name, length)) in dimensions.iter().enumerate() {
        if axis != mesh_axis {
            base += dimension_index(variable, name, *length, selection)? * strides[axis];
        }
    }
    Ok((0..dimensions[mesh_axis].1)
        .map(|mesh_index| values[base + mesh_index * strides[mesh_axis]])
        .collect())
}

/// Replace each mesh dimension with a latitude/longitude plane, keeping every other dimension.
fn virtual_metadata(
    inner: &DatasetMetadata,
    latitude_length: usize,
    longitude_length: usize,
) -> DatasetMetadata {
    let is_mesh_dimension = |name: &str| {
        name == MeshLocation::Cell.dimension_name() || name == MeshLocation::Vertex.dimension_name()
    };
    let mut dimensions = inner
        .dimensions
        .iter()
        .filter(|dimension| !is_mesh_dimension(&dimension.name))
        .map(|dimension| Dimension {
            name: dimension.name.clone(),
            length: dimension.length,
            role: if is_time_dimension(&dimension.name) {
                AxisRole::Time
            } else if is_vertical_dimension(&dimension.name) {
                AxisRole::Depth
            } else {
                dimension.role
            },
        })
        .collect::<Vec<_>>();
    dimensions.push(Dimension {
        name: LATITUDE_DIMENSION.into(),
        length: latitude_length,
        role: AxisRole::Latitude,
    });
    dimensions.push(Dimension {
        name: LONGITUDE_DIMENSION.into(),
        length: longitude_length,
        role: AxisRole::Longitude,
    });
    let variables = inner
        .variables
        .iter()
        .map(|variable| {
            let mut names = variable
                .dimensions
                .iter()
                .filter(|name| !is_mesh_dimension(name))
                .cloned()
                .collect::<Vec<_>>();
            if variable
                .dimensions
                .iter()
                .any(|name| is_mesh_dimension(name))
            {
                names.push(LATITUDE_DIMENSION.into());
                names.push(LONGITUDE_DIMENSION.into());
            }
            Variable {
                dimensions: names,
                ..variable.clone()
            }
        })
        .collect();
    DatasetMetadata {
        path: inner.path.clone(),
        format: inner.format,
        dimensions,
        variables,
    }
}

impl DataSource for MpasSource {
    fn metadata(&self) -> &DatasetMetadata {
        &self.metadata
    }

    fn read_slice(&self, request: &SliceRequest) -> Result<Slice2D> {
        self.read_window(request, &[])
    }

    fn read_slice_on_axes(
        &self,
        request: &SliceRequest,
        _row_axis: Option<&str>,
        _col_axis: Option<&str>,
        fixed_axes: &[(String, usize)],
    ) -> Result<Slice2D> {
        self.read_window(request, fixed_axes)
    }

    fn time_label(&self, index: usize) -> Option<String> {
        self.inner.time_label(index)
    }

    fn vertical_label(&self, variable: &str, index: usize) -> Option<String> {
        self.inner.vertical_label(variable, index)
    }

    fn dimension_values(&self, variable: &str, dimension: &str) -> Option<Vec<f64>> {
        let (rows, cols) = self.grid_shape();
        match dimension {
            LATITUDE_DIMENSION => Some((0..rows).map(|row| self.grid_latitude(row)).collect()),
            LONGITUDE_DIMENSION => Some((0..cols).map(|col| self.grid_longitude(col)).collect()),
            _ => self.inner.dimension_values(variable, dimension),
        }
    }

    fn point_coordinates(&self, _variable: &str, row: usize, col: usize) -> PointCoordinates {
        let (rows, cols) = self.grid_shape();
        if row >= rows || col >= cols {
            return PointCoordinates::default();
        }
        PointCoordinates {
            latitude: Some(self.grid_latitude(row)),
            longitude: Some(self.grid_longitude(col)),
        }
    }
}

pub(crate) fn is_time_dimension(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("time") || lower == "date" || lower == "dates"
}

pub(crate) fn is_vertical_dimension(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("depth")
        || lower.contains("level")
        || lower.contains("lev")
        || lower.contains("pressure")
        || lower.contains("height")
        || lower.contains("altitude")
}

fn open_mesh_source(path: &Path) -> Result<Box<dyn MeshValueSource>> {
    if super::netcdf3::is_netcdf3(path) {
        Ok(Box::new(NetCdf3Source::open(path)?) as Box<dyn MeshValueSource>)
    } else {
        Ok(Box::new(NetCdf4Source::open(path)?) as Box<dyn MeshValueSource>)
    }
}

pub fn detect(metadata: &DatasetMetadata) -> Option<MeshLocation> {
    [MeshLocation::Cell, MeshLocation::Vertex]
        .into_iter()
        .find(|mesh| {
            metadata
                .dimensions
                .iter()
                .any(|dimension| dimension.name == mesh.dimension_name())
        })
}

fn local_coordinates_present(metadata: &DatasetMetadata) -> bool {
    [MeshLocation::Cell, MeshLocation::Vertex]
        .into_iter()
        .any(|mesh| {
            let (lat_name, lon_name) = mesh.coordinate_names();
            metadata
                .variables
                .iter()
                .any(|variable| variable.name == lat_name)
                && metadata
                    .variables
                    .iter()
                    .any(|variable| variable.name == lon_name)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dimension(name: &str, length: usize, role: AxisRole) -> Dimension {
        Dimension {
            name: name.into(),
            length,
            role,
        }
    }

    fn field_metadata() -> DatasetMetadata {
        DatasetMetadata {
            path: "fixture.nc".into(),
            format: crate::data::DatasetFormat::NetCdf3,
            dimensions: vec![
                dimension("Time", 2, AxisRole::Time),
                dimension("nVertices", 3, AxisRole::Other),
                dimension("nVertLevels", 2, AxisRole::Other),
            ],
            variables: vec![Variable {
                name: "field".into(),
                dimensions: vec!["Time".into(), "nVertices".into(), "nVertLevels".into()],
                numeric: true,
                units: None,
                long_name: None,
                standard_name: None,
            }],
        }
    }

    #[test]
    fn radians_convert_to_degrees_and_longitude_wraps() {
        assert!((radians_to_degrees(0.5) - 28.64788975654116).abs() < 1e-9);
        assert!((normalize_longitude(540.0) - (-180.0)).abs() < 1e-9);
        assert_eq!(to_degrees(vec![1.0], Some("degrees_east")), vec![1.0]);
    }

    #[test]
    fn selects_time_and_vertical_plane_for_mesh_dimension() {
        let values = (0..12).map(f64::from).collect::<Vec<_>>();
        let selection = Selection {
            time: 1,
            depth: 0,
            fixed: Vec::new(),
        };

        let selected =
            select_mesh_values(&values, &field_metadata(), "field", "nVertices", &selection)
                .unwrap();

        assert_eq!(selected, [6.0, 8.0, 10.0]);
    }

    #[test]
    fn explicit_fixed_index_overrides_the_default_singleton() {
        let selection = Selection {
            time: 0,
            depth: 0,
            fixed: vec![("kernel".into(), 2)],
        };
        assert_eq!(dimension_index("v", "kernel", 3, &selection).unwrap(), 2);
        assert!(dimension_index("v", "kernel", 3, &Selection::default()).is_err());
    }

    #[test]
    fn virtual_metadata_replaces_mesh_dimension_with_lat_lon_plane() {
        let metadata = virtual_metadata(&field_metadata(), 720, 1440);

        assert!(
            metadata
                .dimensions
                .iter()
                .all(|dimension| dimension.name != "nVertices")
        );
        let depth = metadata
            .dimensions
            .iter()
            .find(|dimension| dimension.name == "nVertLevels")
            .unwrap();
        assert_eq!(depth.role, AxisRole::Depth);
        assert_eq!(
            metadata.variables[0].dimensions,
            ["Time", "nVertLevels", "latitude", "longitude"]
        );
        let longitude = metadata
            .dimensions
            .iter()
            .find(|dimension| dimension.name == "longitude")
            .unwrap();
        assert_eq!(
            (longitude.length, longitude.role),
            (1440, AxisRole::Longitude)
        );
    }

    #[test]
    fn quarter_degree_grid_has_era5_shape_and_cell_centres() {
        assert_eq!(shape_for_resolution(DEFAULT_RESOLUTION_DEG), (720, 1440));
        let latitude = -90.0 + 0.5 * DEFAULT_RESOLUTION_DEG;
        assert!((latitude + 89.875).abs() < 1e-12);
    }
}
