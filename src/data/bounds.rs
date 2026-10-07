//! Coordinate-space bounds mapped to the source-index bounds used by slices.

use std::{error::Error, fmt};

use ndarray::Array2;

use crate::data::slice::Bounds;
use crate::data::{AxisRole, DataSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisKind {
    Linear,
    Longitude,
    Latitude,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumericBounds {
    pub min_x: f64,
    pub max_x: f64,
    pub min_y: f64,
    pub max_y: f64,
}

impl NumericBounds {
    pub fn validate(self) -> Result<Self, BoundsError> {
        validate_range(self.min_x, self.max_x)?;
        validate_range(self.min_y, self.max_y)?;
        Ok(self)
    }
}

impl IndexRange {
    /// Convert inclusive, zero-based cell indices to a half-open index range.
    pub fn from_index_bounds(min: f64, max: f64, length: usize) -> Result<Self, BoundsError> {
        validate_range(min, max)?;
        if length == 0
            || min < 0.0
            || max >= length as f64
            || min.fract() != 0.0
            || max.fract() != 0.0
        {
            return Err(BoundsError(format!(
                "index bounds {min}..={max} must be whole cells within 0..{}",
                length.saturating_sub(1)
            )));
        }
        let start = min as usize;
        let last = max as usize;
        Ok(Self {
            start,
            end: last + 1,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundsError(String);

impl fmt::Display for BoundsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for BoundsError {}

/// Map inclusive coordinate endpoints to the smallest half-open index range
/// containing coordinate samples in the requested interval.
pub fn axis_index_range(
    min: f64,
    max: f64,
    coordinates: &[f64],
    kind: AxisKind,
) -> Result<IndexRange, BoundsError> {
    validate_range(min, max)?;
    if coordinates.is_empty() {
        return Err(BoundsError("coordinate axis is empty".into()));
    }
    validate_physical_range(min, max, kind)?;

    let convention = if kind == AxisKind::Longitude {
        longitude_convention(coordinates)
    } else {
        LongitudeConvention::Signed
    };
    let (min, max) = convert_range(min, max, kind, convention)?;

    let mut first = usize::MAX;
    let mut last = 0;
    for (index, &coordinate) in coordinates.iter().enumerate() {
        if !coordinate.is_finite() {
            continue;
        }
        let coordinate = if kind == AxisKind::Longitude {
            convert_longitude(coordinate, convention)
        } else {
            coordinate
        };
        if coordinate >= min && coordinate <= max {
            first = first.min(index);
            last = last.max(index);
        }
    }
    if first == usize::MAX {
        return Err(BoundsError(format!(
            "bounds {min}..={max} do not overlap the coordinate axis"
        )));
    }
    Ok(IndexRange {
        start: first,
        end: last + 1,
    })
}

/// Resolve a geographic rectangle on curvilinear 2-D coordinates to its
/// smallest row/column envelope. The envelope may contain additional cells.
pub fn curvilinear_index_bounds(
    min_lon: f64,
    max_lon: f64,
    min_lat: f64,
    max_lat: f64,
    longitude: &Array2<f64>,
    latitude: &Array2<f64>,
) -> Result<Bounds, BoundsError> {
    validate_range(min_lon, max_lon)?;
    validate_range(min_lat, max_lat)?;
    validate_physical_range(min_lon, max_lon, AxisKind::Longitude)?;
    validate_physical_range(min_lat, max_lat, AxisKind::Latitude)?;
    if longitude.raw_dim() != latitude.raw_dim() || longitude.is_empty() {
        return Err(BoundsError(
            "curvilinear longitude and latitude grids must have matching, non-empty shapes".into(),
        ));
    }
    let convention = if longitude
        .iter()
        .any(|coordinate| coordinate.is_finite() && *coordinate > 180.0)
    {
        LongitudeConvention::ZeroTo360
    } else {
        LongitudeConvention::Signed
    };
    let (min_lon, max_lon) = convert_range(min_lon, max_lon, AxisKind::Longitude, convention)?;

    let mut row_min = usize::MAX;
    let mut row_max = 0;
    let mut col_min = usize::MAX;
    let mut col_max = 0;
    for row in 0..longitude.nrows() {
        for col in 0..longitude.ncols() {
            let Some(&lon) = longitude.get((row, col)) else {
                continue;
            };
            let Some(&lat) = latitude.get((row, col)) else {
                continue;
            };
            let lon = convert_longitude(lon, convention);
            if lon.is_finite()
                && lat.is_finite()
                && lon >= min_lon
                && lon <= max_lon
                && lat >= min_lat
                && lat <= max_lat
            {
                row_min = row_min.min(row);
                row_max = row_max.max(row);
                col_min = col_min.min(col);
                col_max = col_max.max(col);
            }
        }
    }
    if row_min == usize::MAX {
        return Err(BoundsError(
            "geographic bounds do not contain any curvilinear grid points".into(),
        ));
    }
    Bounds::new(row_min, row_max + 1, col_min, col_max + 1)
        .map_err(|error| BoundsError(error.to_string()))
}

/// Resolve bounds for a data-source variable using its selected x/y axes.
/// Coordinate access is performed by the caller's worker, not a TUI reducer.
pub fn resolve_source_bounds(
    source: &dyn DataSource,
    variable_name: &str,
    x_axis: &str,
    y_axis: &str,
    requested: NumericBounds,
) -> Result<Bounds, BoundsError> {
    resolve_source_bounds_with_feedback(source, variable_name, x_axis, y_axis, requested)
        .map(|(bounds, _)| bounds)
}

/// Resolve source bounds and indicate when a curvilinear geographic rectangle
/// was approximated by a row/column envelope.
pub fn resolve_source_bounds_with_feedback(
    source: &dyn DataSource,
    variable_name: &str,
    x_axis: &str,
    y_axis: &str,
    requested: NumericBounds,
) -> Result<(Bounds, bool), BoundsError> {
    let requested = requested.validate()?;
    let metadata = source.metadata();
    let variable = metadata
        .variables
        .iter()
        .find(|variable| variable.name.eq_ignore_ascii_case(variable_name))
        .ok_or_else(|| BoundsError(format!("variable {variable_name} is unavailable")))?;
    let x_dimension = variable
        .dimensions
        .iter()
        .find(|name| name.eq_ignore_ascii_case(x_axis))
        .ok_or_else(|| {
            BoundsError(format!(
                "x axis {x_axis} is not a dimension of {variable_name}"
            ))
        })?;
    let y_dimension = variable
        .dimensions
        .iter()
        .find(|name| name.eq_ignore_ascii_case(y_axis))
        .ok_or_else(|| {
            BoundsError(format!(
                "y axis {y_axis} is not a dimension of {variable_name}"
            ))
        })?;
    if x_dimension.eq_ignore_ascii_case(y_dimension) {
        return Err(BoundsError("x and y axes must be distinct".into()));
    }
    let dimension_length = |name: &str| {
        metadata
            .dimensions
            .iter()
            .find(|dimension| dimension.name.eq_ignore_ascii_case(name))
            .map(|dimension| dimension.length)
            .ok_or_else(|| BoundsError(format!("axis dimension {name} is unavailable")))
    };
    let x_length = dimension_length(x_dimension)?;
    let y_length = dimension_length(y_dimension)?;
    let x_values = source.dimension_values(variable_name, x_dimension);
    let y_values = source.dimension_values(variable_name, y_dimension);
    let row_axis = variable
        .dimensions
        .get(variable.dimensions.len().saturating_sub(2))
        .map(String::as_str);
    let col_axis = variable.dimensions.last().map(String::as_str);
    let curvilinear_axes = row_axis.is_some_and(|axis| axis.eq_ignore_ascii_case(y_axis))
        && col_axis.is_some_and(|axis| axis.eq_ignore_ascii_case(x_axis));
    if curvilinear_axes {
        let sample = source.point_coordinates(variable_name, 0, 0);
        let vectors_are_geographic = x_values.as_ref().is_some_and(|values| {
            axis_coordinate_kind(metadata, x_dimension, values, true) == AxisKind::Longitude
        }) && y_values.as_ref().is_some_and(|values| {
            axis_coordinate_kind(metadata, y_dimension, values, false) == AxisKind::Latitude
        });
        let vectors_are_indices = x_values
            .as_ref()
            .is_some_and(|values| is_zero_based_index_vector(values))
            && y_values
                .as_ref()
                .is_some_and(|values| is_zero_based_index_vector(values));
        if sample.latitude.is_some()
            && sample.longitude.is_some()
            && (!vectors_are_geographic || vectors_are_indices)
        {
            let mut longitude = Array2::from_elem((y_length, x_length), f64::NAN);
            let mut latitude = Array2::from_elem((y_length, x_length), f64::NAN);
            for row in 0..y_length {
                for col in 0..x_length {
                    let point = source.point_coordinates(variable_name, row, col);
                    longitude[(row, col)] = point.longitude.unwrap_or(f64::NAN);
                    latitude[(row, col)] = point.latitude.unwrap_or(f64::NAN);
                }
            }
            return curvilinear_index_bounds(
                requested.min_x,
                requested.max_x,
                requested.min_y,
                requested.max_y,
                &longitude,
                &latitude,
            )
            .map(|bounds| (bounds, true));
        }
    }

    let x_range = if let Some(values) = &x_values {
        if values.len() != x_length {
            return Err(BoundsError(
                "x-axis coordinate count does not match its dimension".into(),
            ));
        }
        axis_index_range(
            requested.min_x,
            requested.max_x,
            values,
            axis_coordinate_kind(metadata, x_dimension, values, true),
        )?
    } else {
        IndexRange::from_index_bounds(requested.min_x, requested.max_x, x_length)?
    };
    let y_range = if let Some(values) = &y_values {
        if values.len() != y_length {
            return Err(BoundsError(
                "y-axis coordinate count does not match its dimension".into(),
            ));
        }
        axis_index_range(
            requested.min_y,
            requested.max_y,
            values,
            axis_coordinate_kind(metadata, y_dimension, values, false),
        )?
    } else {
        IndexRange::from_index_bounds(requested.min_y, requested.max_y, y_length)?
    };
    Bounds::new(y_range.start, y_range.end, x_range.start, x_range.end)
        .map(|bounds| (bounds, false))
        .map_err(|error| BoundsError(error.to_string()))
}

fn is_zero_based_index_vector(values: &[f64]) -> bool {
    !values.is_empty()
        && values
            .iter()
            .enumerate()
            .all(|(index, value)| *value == index as f64)
}

fn axis_coordinate_kind(
    metadata: &crate::data::DatasetMetadata,
    dimension: &str,
    values: &[f64],
    x_axis: bool,
) -> AxisKind {
    let role = metadata
        .dimensions
        .iter()
        .find(|item| item.name.eq_ignore_ascii_case(dimension))
        .map(|item| item.role)
        .unwrap_or(AxisRole::Other);
    let coordinate_role = metadata.variables.iter().find_map(|variable| {
        if variable.dimensions.len() == 1 && variable.dimensions[0].eq_ignore_ascii_case(dimension)
        {
            let standard_name = variable.standard_name.as_deref().unwrap_or_default();
            let units = variable
                .units
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if standard_name.eq_ignore_ascii_case("longitude") || units.contains("degrees_east") {
                Some(AxisRole::Longitude)
            } else if standard_name.eq_ignore_ascii_case("latitude")
                || units.contains("degrees_north")
            {
                Some(AxisRole::Latitude)
            } else {
                None
            }
        } else {
            None
        }
    });
    match (role, coordinate_role, x_axis) {
        (AxisRole::Longitude, _, true) | (_, Some(AxisRole::Longitude), true) => {
            AxisKind::Longitude
        }
        (AxisRole::Latitude, _, false) | (_, Some(AxisRole::Latitude), false) => AxisKind::Latitude,
        _ if values
            .iter()
            .any(|value| value.is_finite() && *value > 180.0)
            && x_axis =>
        {
            AxisKind::Longitude
        }
        _ => AxisKind::Linear,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LongitudeConvention {
    Signed,
    ZeroTo360,
}

fn longitude_convention(coordinates: &[f64]) -> LongitudeConvention {
    if coordinates
        .iter()
        .any(|coordinate| coordinate.is_finite() && *coordinate > 180.0)
    {
        LongitudeConvention::ZeroTo360
    } else {
        LongitudeConvention::Signed
    }
}

fn convert_range(
    min: f64,
    max: f64,
    kind: AxisKind,
    convention: LongitudeConvention,
) -> Result<(f64, f64), BoundsError> {
    let source_min = min;
    if kind == AxisKind::Longitude && max - min >= 360.0 {
        if max - min > 360.0 {
            return Err(BoundsError(
                "longitude span cannot exceed 360 degrees".into(),
            ));
        }
        return Ok(match convention {
            LongitudeConvention::Signed => (-180.0, 180.0),
            LongitudeConvention::ZeroTo360 => (0.0, 360.0),
        });
    }
    let (min, mut max) = if kind == AxisKind::Longitude {
        (
            convert_longitude(min, convention),
            convert_longitude(max, convention),
        )
    } else {
        (min, max)
    };
    // In a 0–360 coordinate vector, the signed interval [-90, 0] maps to
    // [270, 360], not [270, 0]. Preserve the upper endpoint at the seam so
    // this ordinary interval does not look like a wrapped request.
    if kind == AxisKind::Longitude
        && convention == LongitudeConvention::ZeroTo360
        && source_min < 0.0
        && max == 0.0
    {
        max = 360.0;
    }
    if min > max {
        return Err(BoundsError(
            "longitude interval crosses the coordinate convention boundary; enter ordered bounds within one convention".into(),
        ));
    }
    Ok((min, max))
}

fn convert_longitude(value: f64, convention: LongitudeConvention) -> f64 {
    if !value.is_finite() {
        return value;
    }
    match convention {
        LongitudeConvention::Signed => {
            let mut result = value;
            while result < -180.0 {
                result += 360.0;
            }
            while result > 180.0 {
                result -= 360.0;
            }
            result
        }
        LongitudeConvention::ZeroTo360 => {
            let mut result = value;
            while result < 0.0 {
                result += 360.0;
            }
            while result > 360.0 {
                result -= 360.0;
            }
            if result == 0.0 && value > 0.0 {
                360.0
            } else {
                result
            }
        }
    }
}

fn validate_range(min: f64, max: f64) -> Result<(), BoundsError> {
    if !min.is_finite() || !max.is_finite() {
        return Err(BoundsError("bounds must be finite numbers".into()));
    }
    if min > max {
        return Err(BoundsError(
            "minimum bound must be less than or equal to maximum bound".into(),
        ));
    }
    Ok(())
}

fn validate_physical_range(min: f64, max: f64, kind: AxisKind) -> Result<(), BoundsError> {
    let limits = match kind {
        AxisKind::Linear => return Ok(()),
        AxisKind::Longitude => (-180.0, 360.0, "longitude"),
        AxisKind::Latitude => (-90.0, 90.0, "latitude"),
    };
    if min < limits.0 || max > limits.1 {
        return Err(BoundsError(format!(
            "{} bounds must be between {} and {} degrees",
            limits.2, limits.0, limits.1
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use ndarray::array;

    use super::{AxisKind, IndexRange, axis_index_range, curvilinear_index_bounds};

    #[test]
    fn maps_inclusive_ranges_on_ascending_and_descending_axes() {
        assert_eq!(
            axis_index_range(10.0, 20.0, &[0.0, 10.0, 20.0, 30.0], AxisKind::Linear).unwrap(),
            IndexRange { start: 1, end: 3 }
        );
        assert_eq!(
            axis_index_range(10.0, 20.0, &[30.0, 20.0, 10.0, 0.0], AxisKind::Linear).unwrap(),
            IndexRange { start: 1, end: 3 }
        );
    }

    #[test]
    fn rejects_non_finite_reversed_and_non_overlapping_ranges() {
        let axis = [0.0, 1.0, 2.0];
        assert!(axis_index_range(f64::NAN, 1.0, &axis, AxisKind::Linear).is_err());
        assert!(axis_index_range(1.0, f64::INFINITY, &axis, AxisKind::Linear).is_err());
        assert!(axis_index_range(2.0, 1.0, &axis, AxisKind::Linear).is_err());
        assert!(axis_index_range(4.0, 5.0, &axis, AxisKind::Linear).is_err());
    }

    #[test]
    fn index_fallback_maps_inclusive_integer_cell_indices() {
        assert_eq!(
            IndexRange::from_index_bounds(1.0, 3.0, 5).unwrap(),
            IndexRange { start: 1, end: 4 }
        );
        assert!(IndexRange::from_index_bounds(-1.0, 2.0, 5).is_err());
        assert!(IndexRange::from_index_bounds(1.5, 3.0, 5).is_err());
        assert!(IndexRange::from_index_bounds(4.0, 5.0, 5).is_err());
    }

    #[test]
    fn maps_signed_and_zero_to_360_longitudes_to_axis_samples() {
        let axis_360 = [0.0, 90.0, 180.0, 270.0, 360.0];
        assert_eq!(
            axis_index_range(260.0, 300.0, &axis_360, AxisKind::Longitude).unwrap(),
            IndexRange { start: 3, end: 4 }
        );
        assert_eq!(
            axis_index_range(-100.0, -60.0, &axis_360, AxisKind::Longitude).unwrap(),
            IndexRange { start: 3, end: 4 }
        );

        let axis_signed = [-180.0, -90.0, 0.0, 90.0, 180.0];
        assert_eq!(
            axis_index_range(260.0, 300.0, &axis_signed, AxisKind::Longitude).unwrap(),
            IndexRange { start: 1, end: 2 }
        );
    }

    #[test]
    fn full_globe_longitude_endpoints_preserve_the_zero_to_360_span() {
        let signed = [-180.0, -90.0, 0.0, 90.0, 180.0];
        let wrapped = [0.0, 90.0, 180.0, 270.0];
        assert_eq!(
            axis_index_range(0.0, 360.0, &signed, AxisKind::Longitude).unwrap(),
            IndexRange { start: 0, end: 5 }
        );
        assert_eq!(
            axis_index_range(0.0, 360.0, &wrapped, AxisKind::Longitude).unwrap(),
            IndexRange { start: 0, end: 4 }
        );
    }

    #[test]
    fn applies_geographic_physical_limits_without_restricting_linear_axes() {
        let latitude = [-90.0, 0.0, 90.0];
        assert!(axis_index_range(-91.0, 10.0, &latitude, AxisKind::Latitude).is_err());
        assert!(axis_index_range(0.0, 361.0, &[0.0, 180.0, 360.0], AxisKind::Longitude).is_err());
        assert!(
            axis_index_range(200.0, 250.0, &[0.0, 100.0, 200.0, 300.0], AxisKind::Linear).is_ok()
        );
    }

    #[test]
    fn maps_curvilinear_geographic_rectangle_to_minimal_index_envelope() {
        let longitude = array![[0.0, 10.0, 20.0], [0.0, 10.0, 20.0], [0.0, 10.0, 20.0]];
        let latitude = array![[-10.0, -10.0, -10.0], [0.0, 0.0, 0.0], [10.0, 10.0, 10.0]];
        assert_eq!(
            curvilinear_index_bounds(5.0, 15.0, -5.0, 5.0, &longitude, &latitude).unwrap(),
            crate::data::slice::Bounds {
                row_start: 1,
                row_end: 2,
                col_start: 1,
                col_end: 2,
            }
        );
        assert!(curvilinear_index_bounds(30.0, 40.0, -5.0, 5.0, &longitude, &latitude).is_err());
    }
}
