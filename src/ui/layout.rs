use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DashboardAreas {
    pub header: Rect,
    pub sidebar: Rect,
    pub canvas: Rect,
    pub legend: Rect,
    /// The level gauge band, or `Rect::ZERO` when the selected variable has no
    /// vertical axis. Kept beside `timeline` so hit-tests and rendering agree.
    pub level: Rect,
    pub timeline: Rect,
    pub status: Rect,
    pub constrained: bool,
}

pub fn dashboard(area: Rect, show_level: bool) -> DashboardAreas {
    let mut constraints = vec![Constraint::Length(2), Constraint::Min(4)];
    if show_level {
        constraints.push(Constraint::Length(3));
    }
    constraints.push(Constraint::Length(3));
    constraints.push(Constraint::Length(1));
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(32),
            Constraint::Min(1),
            Constraint::Length(30),
        ])
        .split(vertical[1]);
    let offset = usize::from(show_level);
    DashboardAreas {
        header: vertical[0],
        sidebar: body[0],
        canvas: body[1],
        legend: body[2],
        level: if show_level { vertical[2] } else { Rect::ZERO },
        timeline: vertical[2 + offset],
        status: vertical[3 + offset],
        constrained: area.width < 68 || area.height < 8,
    }
}

#[cfg(test)]
mod tests {
    use super::dashboard;
    use ratatui::layout::Rect;

    #[test]
    fn layout_collapses_for_small_terminals() {
        let areas = dashboard(Rect::new(0, 0, 40, 7), false);
        assert!(areas.constrained);
        assert!(areas.header.height > 0);
    }

    #[test]
    fn hidden_level_row_matches_legacy_geometry() {
        let areas = dashboard(Rect::new(0, 0, 100, 30), false);
        assert!(areas.level.is_empty());
        assert_eq!(areas.timeline, Rect::new(0, 26, 100, 3));
        assert_eq!(areas.status, Rect::new(0, 29, 100, 1));
    }

    #[test]
    fn level_row_inserts_above_the_time_row() {
        let areas = dashboard(Rect::new(0, 0, 100, 30), true);
        assert_eq!(areas.level, Rect::new(0, 23, 100, 3));
        assert_eq!(areas.timeline, Rect::new(0, 26, 100, 3));
        assert_eq!(areas.status, Rect::new(0, 29, 100, 1));
        // The body (sidebar/canvas/legend) shrinks by the level row.
        assert_eq!(areas.sidebar.height, 21);
    }
}
