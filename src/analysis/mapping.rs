use crate::data::slice::Bounds;
use ratatui::layout::Rect;

pub fn screen_to_source(x: u16, y: u16, canvas: Rect, bounds: Bounds) -> Option<(usize, usize)> {
    if canvas.width == 0
        || canvas.height == 0
        || x < canvas.x
        || y < canvas.y
        || x >= canvas.right()
        || y >= canvas.bottom()
    {
        return None;
    }
    let col = bounds.col_start
        + usize::from(x - canvas.x) * (bounds.col_end - bounds.col_start)
            / usize::from(canvas.width);
    let row = bounds.row_start
        + usize::from(y - canvas.y) * (bounds.row_end - bounds.row_start)
            / usize::from(canvas.height);
    Some((row.min(bounds.row_end - 1), col.min(bounds.col_end - 1)))
}

#[cfg(test)]
mod tests {
    use super::screen_to_source;
    use crate::data::slice::Bounds;
    use ratatui::layout::Rect;
    #[test]
    fn mapping_handles_edges_and_padding() {
        let bounds = Bounds::new(10, 20, 30, 40).unwrap();
        let canvas = Rect::new(5, 2, 10, 10);
        assert_eq!(screen_to_source(5, 2, canvas, bounds), Some((10, 30)));
        assert_eq!(screen_to_source(14, 11, canvas, bounds), Some((19, 39)));
        assert_eq!(screen_to_source(4, 2, canvas, bounds), None);
    }
}
