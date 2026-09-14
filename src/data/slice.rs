use ndarray::Array2;

use crate::error::{NcvError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub row_start: usize,
    pub row_end: usize,
    pub col_start: usize,
    pub col_end: usize,
}

impl Bounds {
    pub fn new(row_start: usize, row_end: usize, col_start: usize, col_end: usize) -> Result<Self> {
        if row_start >= row_end || col_start >= col_end {
            return Err(NcvError::InvalidSlice(
                "bounds must be non-empty and half-open".into(),
            ));
        }
        Ok(Self {
            row_start,
            row_end,
            col_start,
            col_end,
        })
    }

    pub fn shape(self) -> (usize, usize) {
        (self.row_end - self.row_start, self.col_end - self.col_start)
    }

    pub fn element_count(self) -> Result<usize> {
        self.shape()
            .0
            .checked_mul(self.shape().1)
            .ok_or_else(|| NcvError::InvalidSlice("element count overflow".into()))
    }

    pub fn byte_count(self, bytes_per_element: usize) -> Result<usize> {
        self.element_count()?
            .checked_mul(bytes_per_element)
            .ok_or_else(|| NcvError::InvalidSlice("byte count overflow".into()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validity {
    Finite,
    Fill,
    Missing,
    InvalidRange,
    NaN,
    PosInf,
    NegInf,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Statistics {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub finite_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceRequest {
    pub variable: String,
    pub time: usize,
    pub depth: usize,
    pub bounds: Bounds,
}

#[derive(Debug, Clone)]
pub struct CoordinateGrid {
    /// Latitude at each displayed source cell for a curvilinear grid.
    pub latitude: Option<Array2<f64>>,
    /// Longitude at each displayed source cell for a curvilinear grid.
    pub longitude: Option<Array2<f64>>,
    /// Latitude values for a regular grid, indexed by source row.
    ///
    /// Keeping regular coordinates as axes avoids expanding two small 1-D
    /// vectors into full-size f64 planes for large rasters.
    pub latitude_axis: Option<Vec<f64>>,
    /// Longitude values for a regular grid, indexed by source column.
    pub longitude_axis: Option<Vec<f64>>,
}

impl CoordinateGrid {
    pub fn is_empty(&self) -> bool {
        self.latitude.is_none()
            && self.longitude.is_none()
            && self.latitude_axis.is_none()
            && self.longitude_axis.is_none()
    }

    pub fn shape(&self) -> Option<(usize, usize)> {
        self.latitude
            .as_ref()
            .or(self.longitude.as_ref())
            .map(|grid| grid.dim())
            .or_else(|| {
                Some((
                    self.latitude_axis.as_ref()?.len(),
                    self.longitude_axis.as_ref()?.len(),
                ))
            })
    }

    pub fn latitude_at(&self, row: usize, col: usize) -> Option<f64> {
        self.latitude
            .as_ref()
            .and_then(|grid| grid.get((row, col)).copied())
            .or_else(|| self.latitude_axis.as_ref()?.get(row).copied())
    }

    pub fn longitude_at(&self, row: usize, col: usize) -> Option<f64> {
        self.longitude
            .as_ref()
            .and_then(|grid| grid.get((row, col)).copied())
            .or_else(|| self.longitude_axis.as_ref()?.get(col).copied())
    }
}

#[derive(Debug, Clone)]
pub struct Slice2D {
    pub values: Array2<f64>,
    pub validity: Array2<Validity>,
    pub source_bounds: Bounds,
    pub statistics: Option<Statistics>,
    pub coordinates: Option<CoordinateGrid>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PackedAttributes {
    pub fill: Option<f64>,
    pub missing: Option<f64>,
    pub valid_min: Option<f64>,
    pub valid_max: Option<f64>,
    pub scale_factor: Option<f64>,
    pub add_offset: Option<f64>,
}

pub fn classify_packed(values: &[f64], attributes: PackedAttributes) -> (Vec<f64>, Vec<Validity>) {
    let scale = attributes.scale_factor.unwrap_or(1.0);
    let offset = attributes.add_offset.unwrap_or(0.0);
    let fill_bits = attributes.fill.map(f64::to_bits);
    let missing_bits = attributes.missing.map(f64::to_bits);
    let valid_min = attributes.valid_min;
    let valid_max = attributes.valid_max;

    let len = values.len();
    let mut out_values = Vec::with_capacity(len);
    let mut out_validity = Vec::with_capacity(len);
    let is_identity = scale == 1.0 && offset == 0.0;

    for &packed in values {
        let bits = packed.to_bits();
        let validity = if fill_bits.is_some_and(|f| bits == f) {
            Validity::Fill
        } else if missing_bits.is_some_and(|m| bits == m) {
            Validity::Missing
        } else if valid_min.is_some_and(|min| packed < min)
            || valid_max.is_some_and(|max| packed > max)
        {
            Validity::InvalidRange
        } else if packed.is_nan() {
            Validity::NaN
        } else if packed == f64::INFINITY {
            Validity::PosInf
        } else if packed == f64::NEG_INFINITY {
            Validity::NegInf
        } else {
            Validity::Finite
        };

        let val = if is_identity {
            packed
        } else {
            packed * scale + offset
        };
        out_values.push(val);
        out_validity.push(validity);
    }

    (out_values, out_validity)
}

impl Slice2D {
    /// Approximate resident bytes owned by the decoded scientific result.
    /// This counts the value, validity, and optional coordinate arrays that
    /// remain live while an asynchronous replacement is prepared.
    pub fn memory_bytes(&self) -> usize {
        let values = self.values.len().saturating_mul(std::mem::size_of::<f64>());
        let validity = self
            .validity
            .len()
            .saturating_mul(std::mem::size_of::<Validity>());
        let coordinates = self.coordinates.as_ref().map_or(0, |grid| {
            let latitude = grid.latitude.as_ref().map_or(0, |values| {
                values.len().saturating_mul(std::mem::size_of::<f64>())
            });
            let longitude = grid.longitude.as_ref().map_or(0, |values| {
                values.len().saturating_mul(std::mem::size_of::<f64>())
            });
            let latitude_axis = grid.latitude_axis.as_ref().map_or(0, |values| {
                values.len().saturating_mul(std::mem::size_of::<f64>())
            });
            let longitude_axis = grid.longitude_axis.as_ref().map_or(0, |values| {
                values.len().saturating_mul(std::mem::size_of::<f64>())
            });
            latitude
                .saturating_add(longitude)
                .saturating_add(latitude_axis)
                .saturating_add(longitude_axis)
        });
        values.saturating_add(validity).saturating_add(coordinates)
    }

    pub fn with_statistics(
        values: Array2<f64>,
        validity: Array2<Validity>,
        source_bounds: Bounds,
        statistics: Option<Statistics>,
    ) -> Result<Self> {
        if values.raw_dim() != validity.raw_dim()
            || values.raw_dim() != ndarray::Ix2(source_bounds.shape().0, source_bounds.shape().1)
        {
            return Err(NcvError::InvalidSlice(
                "values, mask, and bounds shapes differ".into(),
            ));
        }
        Ok(Self {
            values,
            validity,
            source_bounds,
            statistics,
            coordinates: None,
        })
    }

    pub fn new(
        values: Array2<f64>,
        validity: Array2<Validity>,
        source_bounds: Bounds,
    ) -> Result<Self> {
        if values.raw_dim() != validity.raw_dim()
            || values.raw_dim() != ndarray::Ix2(source_bounds.shape().0, source_bounds.shape().1)
        {
            return Err(NcvError::InvalidSlice(
                "values, mask, and bounds shapes differ".into(),
            ));
        }
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        let mut sum = 0.0;
        let mut finite_count = 0;
        if let (Some(v_slice), Some(m_slice)) = (values.as_slice(), validity.as_slice()) {
            for (&value, &mask) in v_slice.iter().zip(m_slice.iter()) {
                if mask == Validity::Finite && value.is_finite() {
                    min = f64::min(min, value);
                    max = f64::max(max, value);
                    sum += value;
                    finite_count += 1;
                }
            }
        } else {
            for (&value, &mask) in values.iter().zip(validity.iter()) {
                if mask == Validity::Finite && value.is_finite() {
                    min = f64::min(min, value);
                    max = f64::max(max, value);
                    sum += value;
                    finite_count += 1;
                }
            }
        }
        let statistics = (finite_count > 0).then_some(Statistics {
            min,
            max,
            mean: sum / finite_count as f64,
            finite_count,
        });
        Self::with_statistics(values, validity, source_bounds, statistics)
    }

    pub fn with_coordinates(mut self, coordinates: CoordinateGrid) -> Self {
        if !coordinates.is_empty()
            && coordinates
                .latitude
                .as_ref()
                .is_none_or(|grid| grid.raw_dim() == self.values.raw_dim())
            && coordinates
                .longitude
                .as_ref()
                .is_none_or(|grid| grid.raw_dim() == self.values.raw_dim())
            && coordinates
                .latitude_axis
                .as_ref()
                .is_none_or(|axis| axis.len() == self.values.nrows())
            && coordinates
                .longitude_axis
                .as_ref()
                .is_none_or(|axis| axis.len() == self.values.ncols())
        {
            self.coordinates = Some(coordinates);
        }
        self
    }

    pub fn permuted_axes(&self) -> Result<Self> {
        let values = self.values.view().permuted_axes([1, 0]).to_owned();
        let validity = self.validity.view().permuted_axes([1, 0]).to_owned();
        let bounds = Bounds::new(
            self.source_bounds.col_start,
            self.source_bounds.col_end,
            self.source_bounds.row_start,
            self.source_bounds.row_end,
        )?;
        Self::with_statistics(values, validity, bounds, self.statistics).map(|mut slice| {
            if let Some(coordinates) = &self.coordinates {
                slice.coordinates = Some(CoordinateGrid {
                    latitude: coordinates
                        .latitude
                        .as_ref()
                        .map(|grid| grid.view().permuted_axes([1, 0]).to_owned()),
                    longitude: coordinates
                        .longitude
                        .as_ref()
                        .map(|grid| grid.view().permuted_axes([1, 0]).to_owned()),
                    // A compact axis is tied to its row/column role. An
                    // arbitrary transpose turns it into a 2-D mapping, so
                    // drop it and let the renderer use its geographic
                    // fallback rather than allocating a huge temporary grid.
                    latitude_axis: None,
                    longitude_axis: None,
                });
            }
            slice
        })
    }

    pub fn value_at_source(&self, row: usize, col: usize) -> Option<f64> {
        if row < self.source_bounds.row_start
            || row >= self.source_bounds.row_end
            || col < self.source_bounds.col_start
            || col >= self.source_bounds.col_end
        {
            return None;
        }
        let local_row = row - self.source_bounds.row_start;
        let local_col = col - self.source_bounds.col_start;
        (self.validity[(local_row, local_col)] == Validity::Finite)
            .then_some(self.values[(local_row, local_col)])
            .filter(|value| value.is_finite())
    }
}
