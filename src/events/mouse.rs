use ratatui::layout::Rect;

use crate::data::slice::Bounds;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragState {
    pub start: (u16, u16),
    pub current: (u16, u16),
    pub zoom: bool,
}

impl DragState {
    pub fn pan_delta(
        self,
        canvas: Rect,
        row_span: usize,
        col_span: usize,
    ) -> Option<(isize, isize)> {
        if canvas.width == 0 || canvas.height == 0 || (row_span == 0 && col_span == 0) {
            return None;
        }
        let dx = i32::from(self.current.0) - i32::from(self.start.0);
        let dy = i32::from(self.current.1) - i32::from(self.start.1);
        if dx == 0 && dy == 0 {
            return None;
        }
        let rows = -scaled_delta(dy, row_span, usize::from(canvas.height));
        let cols = -scaled_delta(dx, col_span, usize::from(canvas.width));
        (rows != 0 || cols != 0).then_some((rows, cols))
    }

    pub fn bounds(self, canvas: Rect, rows: usize, cols: usize) -> Option<Bounds> {
        self.bounds_with_row_flip(canvas, rows, cols, false)
    }

    pub fn bounds_with_row_flip(
        self,
        canvas: Rect,
        rows: usize,
        cols: usize,
        flip_rows: bool,
    ) -> Option<Bounds> {
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
        let display_row_start = usize::from(top) * rows / usize::from(canvas.height);
        let display_row_end = ((usize::from(bottom) + 1) * rows / usize::from(canvas.height))
            .max(display_row_start + 1)
            .min(rows);
        let (row_start, row_end) = if flip_rows {
            (rows - display_row_end, rows - display_row_start)
        } else {
            (display_row_start, display_row_end)
        };
        let col_start = usize::from(left) * cols / usize::from(canvas.width);
        let col_end = ((usize::from(right) + 1) * cols / usize::from(canvas.width))
            .max(col_start + 1)
            .min(cols);
        Bounds::new(row_start, row_end, col_start, col_end).ok()
    }
}

fn scaled_delta(delta: i32, span: usize, pixels: usize) -> isize {
    if delta == 0 || span == 0 || pixels == 0 {
        return 0;
    }
    let magnitude = (delta.unsigned_abs() as usize * span + pixels / 2) / pixels;
    let magnitude = magnitude.max(1) as isize;
    if delta.is_negative() {
        -magnitude
    } else {
        magnitude
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::DragState;

    #[test]
    fn drag_delta_translates_screen_motion_into_view_pan() {
        let drag = DragState {
            start: (50, 50),
            current: (70, 30),
            zoom: false,
        };
        assert_eq!(
            drag.pan_delta(Rect::new(0, 0, 100, 100), 50, 80),
            Some((10, -16))
        );
    }
}
