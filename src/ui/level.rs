use ratatui::layout::Rect;

const ROW: u16 = 1;
/// Rows the section draws before the list: heading, readout, stepper.
const LEVEL_CHROME_ROWS: u16 = 3;
/// Rows from `area.y` to the first variable row (see the plan's row map):
/// border(1) + colormap heading + palette + scale + "View actions" heading
/// + 2 action rows + "Navigation" heading + 2 nav rows + "Variables" heading
/// + search row = 13.
const SIDEBAR_ROWS_ABOVE_VARIABLES: u16 = 13;
/// Blank separator between the variable list and the Level section.
const LEVEL_SEPARATOR_ROWS: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepthButton {
    Prev,
    Next,
}

#[derive(Debug, Clone, Copy)]
pub struct LevelPanel<'a> {
    pub labels: &'a [String],
    pub selected: usize,
    pub cursor: usize,
    pub focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelSection {
    pub heading: u16,
    pub header: u16,
    pub stepper: u16,
    pub list_top: u16,
    pub list_rows: usize,
    pub window_top: usize,
    pub stepper_prev: Rect,
    pub stepper_next: Rect,
    pub list_rect: Rect,
}

/// Absolute row of the first variable in the sidebar list. Shared by the widget
/// and the hit-test so both agree where the variable rows end.
pub fn first_variable_row(area: Rect) -> u16 {
    area.y.saturating_add(SIDEBAR_ROWS_ABOVE_VARIABLES)
}

/// Absolute geometry of the sidebar Level section, shared by the widget and the
/// mouse hit-test so click targets cannot drift from what is drawn.
///
/// * `area` - the sidebar panel area, including its border.
/// * `variable_rows` - how many variable rows the widget actually draws.
/// * `label_count` - number of vertical levels; `<= 1` hides the section.
/// * `stepper_span` - width of one stepper button (half the inner width).
pub fn level_section(
    area: Rect,
    variable_rows: usize,
    label_count: usize,
    stepper_span: u16,
) -> Option<LevelSection> {
    if label_count <= 1 {
        return None;
    }
    let inner_left = area.x.saturating_add(1);
    let inner_width = area.width.saturating_sub(2);
    let heading = first_variable_row(area)
        .saturating_add(u16::try_from(variable_rows).unwrap_or(u16::MAX))
        .saturating_add(LEVEL_SEPARATOR_ROWS);
    let list_top = heading.saturating_add(LEVEL_CHROME_ROWS);
    let list_rows = usize::from(area.bottom().saturating_sub(1).saturating_sub(list_top));
    if list_rows == 0 {
        return None;
    }
    let span = stepper_span.max(1);
    Some(LevelSection {
        heading,
        header: heading.saturating_add(ROW),
        stepper: heading.saturating_add(2),
        list_top,
        list_rows,
        window_top: 0,
        stepper_prev: Rect::new(inner_left, heading.saturating_add(2), span, ROW),
        stepper_next: Rect::new(
            inner_left.saturating_add(span),
            heading.saturating_add(2),
            span,
            ROW,
        ),
        list_rect: Rect::new(
            inner_left,
            list_top,
            inner_width,
            u16::try_from(list_rows).unwrap_or(u16::MAX),
        ),
    })
}

/// Index window of `label_count` levels that fits in `rows`, keeping `active`
/// visible with the window centered on it and clamped to the ends.
/// Returns `(top, len)`.
pub fn level_window(label_count: usize, rows: usize, active: usize) -> (usize, usize) {
    if label_count == 0 || rows == 0 {
        return (0, 0);
    }
    let len = rows.min(label_count);
    let last_top = label_count - len;
    let mut top = active.saturating_sub(len / 2).min(last_top);
    if active >= top + len {
        top = (active + 1).saturating_sub(len).min(last_top);
    }
    (top, len)
}

/// Level index drawn at absolute terminal `row`, if that row is a list row.
pub fn level_at(section: &LevelSection, row: u16) -> Option<usize> {
    if row < section.list_top || usize::from(row - section.list_top) >= section.list_rows {
        return None;
    }
    Some(section.window_top + usize::from(row - section.list_top))
}

/// Which stepper button, if any, occupies absolute column `x` on the stepper row.
pub fn level_button(section: &LevelSection, x: u16) -> Option<DepthButton> {
    if section.stepper_prev.contains((x, section.stepper).into()) {
        return Some(DepthButton::Prev);
    }
    if section.stepper_next.contains((x, section.stepper).into()) {
        return Some(DepthButton::Next);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        DepthButton, first_variable_row, level_at, level_button, level_section, level_window,
    };
    use ratatui::layout::Rect;

    // A 32x30 sidebar whose top-left sits at the origin; the Dataset panel
    // border means content starts one row below `area.y`.
    const AREA: Rect = Rect::new(0, 0, 32, 30);

    #[test]
    fn first_variable_row_matches_the_widget() {
        // "Variables" heading at area.y+11, search row at +12, first variable +13.
        assert_eq!(first_variable_row(AREA), AREA.y + 13);
    }

    #[test]
    fn section_is_absent_without_levels() {
        assert!(level_section(AREA, 4, 0, 10).is_none());
        assert!(level_section(AREA, 4, 1, 10).is_none());
    }

    #[test]
    fn section_sits_below_the_variable_list() {
        let section = level_section(AREA, 4, 6, 10).unwrap();
        // blank separator then heading
        assert_eq!(section.heading, first_variable_row(AREA) + 4 + 1);
        assert_eq!(section.header, section.heading + 1);
        assert_eq!(section.stepper, section.header + 1);
        assert_eq!(section.list_top, section.stepper + 1);
    }

    #[test]
    fn list_rows_stop_at_the_panel_bottom() {
        let section = level_section(AREA, 4, 40, 10).unwrap();
        let inner_bottom = usize::from(AREA.y + AREA.height - 1); // bottom border row
        assert!(usize::from(section.list_top) + section.list_rows <= inner_bottom);
        assert!(section.list_rows > 0);
    }

    #[test]
    fn level_at_maps_list_rows_to_indices_and_rejects_other_rows() {
        let section = level_section(AREA, 4, 40, 10).unwrap();
        assert_eq!(level_at(&section, section.heading), None);
        assert_eq!(level_at(&section, section.stepper), None);
        assert_eq!(
            level_at(&section, section.list_top),
            Some(section.window_top)
        );
        let last = section.list_top + u16::try_from(section.list_rows - 1).unwrap();
        assert_eq!(
            level_at(&section, last),
            Some(section.window_top + section.list_rows - 1)
        );
        assert_eq!(level_at(&section, last + 1), None);
    }

    #[test]
    fn window_keeps_the_active_level_visible() {
        assert_eq!(level_window(40, 8, 0), (0, 8));
        assert_eq!(level_window(40, 8, 39), (32, 8));
        assert_eq!(level_window(40, 8, 4), (0, 8));
        assert_eq!(level_window(3, 8, 1), (0, 3));
        assert_eq!(level_window(40, 8, 20), (16, 8));
    }

    #[test]
    fn window_never_overflows_the_range() {
        for active in 0..40 {
            let (top, len) = level_window(40, 8, active);
            assert!(top + len <= 40, "active {active} -> ({top},{len})");
            assert!(
                active >= top && active < top + len,
                "active {active} outside ({top},{len})"
            );
        }
    }

    #[test]
    fn stepper_halves_resolve_to_buttons() {
        let section = level_section(AREA, 4, 40, 10).unwrap();
        assert_eq!(
            level_button(&section, section.stepper_prev.x + 1),
            Some(DepthButton::Prev)
        );
        assert_eq!(
            level_button(&section, section.stepper_next.x + 1),
            Some(DepthButton::Next)
        );
        assert_eq!(level_button(&section, AREA.x), None);
    }
}
