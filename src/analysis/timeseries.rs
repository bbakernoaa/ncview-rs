#[derive(Debug, Clone, PartialEq)]
pub struct TimeSample {
    pub index: usize,
    pub value: Option<f64>,
}

pub fn point_across_time(
    values: &[Vec<Option<f64>>],
    row: usize,
    col: usize,
) -> Option<Vec<TimeSample>> {
    if values.is_empty() {
        return None;
    }
    Some(
        values
            .iter()
            .enumerate()
            .map(|(index, slice)| TimeSample {
                index,
                value: slice.get(row + col).copied().flatten(),
            })
            .collect(),
    )
}
