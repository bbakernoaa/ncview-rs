#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridClass {
    Regular,
    Rectilinear,
    Curvilinear,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoordinateGrid {
    pub lat: Vec<f64>,
    pub lon: Vec<f64>,
    pub rows: usize,
    pub cols: usize,
    pub class: GridClass,
}

pub fn classify(lat: &[f64], lon: &[f64], rows: usize, cols: usize) -> Option<CoordinateGrid> {
    if lat.len() != rows.checked_mul(cols)? || lon.len() != lat.len() {
        return None;
    }
    let class = if rows == 1 || cols == 1 {
        GridClass::Rectilinear
    } else {
        let lat_regular = lat.chunks(cols).all(|row| {
            row.iter()
                .all(|value| (*value - row[0]).abs() < f64::EPSILON)
        });
        let lon_regular = (0..cols).all(|col| {
            (1..rows).all(|row| (lon[row * cols + col] - lon[col]).abs() < f64::EPSILON)
        });
        if lat_regular && lon_regular {
            GridClass::Rectilinear
        } else {
            GridClass::Curvilinear
        }
    };
    Some(CoordinateGrid {
        lat: lat.to_vec(),
        lon: lon.to_vec(),
        rows,
        cols,
        class,
    })
}
