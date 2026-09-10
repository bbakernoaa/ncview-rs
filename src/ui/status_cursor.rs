pub fn cursor_label(cursor: Option<(usize, usize, f64)>) -> String {
    cursor.map_or_else(|| "cursor: —".into(), |(row, col, value)| format!("lat index {row}  lon index {col}  value {value:.6}"))
}
