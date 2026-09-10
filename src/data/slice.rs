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
    /// Latitude at each displayed source cell.  A 2-D array is used for both
    /// regular and curvilinear grids so the renderer never has to guess how
    /// a zoomed slice maps back to geographic space.
    pub latitude: Option<Array2<f64>>,
    /// Longitude at each displayed source cell.
    pub longitude: Option<Array2<f64>>,
}

impl CoordinateGrid {
    pub fn is_empty(&self) -> bool {
        self.latitude.is_none() && self.longitude.is_none()
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
    values
        .iter()
        .map(|&packed| {
            let validity = if attributes
                .fill
                .is_some_and(|fill| packed.to_bits() == fill.to_bits())
            {
                Validity::Fill
            } else if attributes
                .missing
                .is_some_and(|missing| packed.to_bits() == missing.to_bits())
            {
                Validity::Missing
            } else if attributes.valid_min.is_some_and(|min| packed < min)
                || attributes.valid_max.is_some_and(|max| packed > max)
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
            (packed * scale + offset, validity)
        })
        .unzip()
}

impl Slice2D {
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
        let mut finite_count = 0;
        for ((row, col), value) in values.indexed_iter() {
            if validity[(row, col)] == Validity::Finite && value.is_finite() {
                min = min.min(*value);
                max = max.max(*value);
                finite_count += 1;
            }
        }
        let statistics = (finite_count > 0).then_some(Statistics {
            min,
            max,
            finite_count,
        });
        Ok(Self {
            values,
            validity,
            source_bounds,
            statistics,
            coordinates: None,
        })
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
        Self::new(values, validity, bounds).map(|mut slice| {
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
