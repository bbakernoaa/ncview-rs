use ndarray::Array2;

use super::slice::{Bounds, Slice2D, Validity};
use crate::error::Result;

pub fn regular_values(rows: usize, cols: usize) -> Result<Slice2D> {
    let values = Array2::from_shape_fn((rows, cols), |(row, col)| (row * cols + col) as f64);
    let validity = Array2::from_elem((rows, cols), Validity::Finite);
    Slice2D::new(values, validity, Bounds::new(0, rows, 0, cols)?)
}
