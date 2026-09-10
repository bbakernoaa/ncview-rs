use ratatui::layout::Rect;

use crate::data::slice::Bounds;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragState {
    pub start: (u16, u16),
    pub current: (u16, u16),
}

impl DragState {
    pub fn bounds(self, canvas: Rect, rows: usize, cols: usize) -> Option<Bounds> {
        if canvas.width == 0 || canvas.height == 0 {
            return None;
        }
        let x0 = self
            .start
            .0
            .max(canvas.x)
            .min(canvas.right().saturating_sub(1));
        let x1 = self
            .current
            .0
            .max(canvas.x)
            .min(canvas.right().saturating_sub(1));
        let y0 = self
            .start
            .1
            .max(canvas.y)
            .min(canvas.bottom().saturating_sub(1));
        let y1 = self
            .current
            .1
            .max(canvas.y)
            .min(canvas.bottom().saturating_sub(1));
        let left = x0.min(x1) - canvas.x;
        let right = x0.max(x1) - canvas.x;
        let top = y0.min(y1) - canvas.y;
        let bottom = y0.max(y1) - canvas.y;
        if left == right || top == bottom {
            return None;
        }
        let row_start = usize::from(top) * rows / usize::from(canvas.height);
        let row_end = ((usize::from(bottom) + 1) * rows / usize::from(canvas.height))
            .max(row_start + 1)
            .min(rows);
        let col_start = usize::from(left) * cols / usize::from(canvas.width);
        let col_end = ((usize::from(right) + 1) * cols / usize::from(canvas.width))
            .max(col_start + 1)
            .min(cols);
        Bounds::new(row_start, row_end, col_start, col_end).ok()
    }
}
