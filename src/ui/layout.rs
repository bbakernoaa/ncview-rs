use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DashboardAreas {
    pub header: Rect,
    pub sidebar: Rect,
    pub canvas: Rect,
    pub legend: Rect,
    pub timeline: Rect,
    pub status: Rect,
    pub constrained: bool,
}

pub fn dashboard(area: Rect) -> DashboardAreas {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(4),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(area);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(28),
            Constraint::Min(1),
            Constraint::Length(30),
        ])
        .split(vertical[1]);
    DashboardAreas {
        header: vertical[0],
        sidebar: body[0],
        canvas: body[1],
        legend: body[2],
        timeline: vertical[2],
        status: vertical[3],
        constrained: area.width < 68 || area.height < 8,
    }
}

#[cfg(test)]
mod tests {
    use super::dashboard;
    use ratatui::layout::Rect;

    #[test]
    fn layout_collapses_for_small_terminals() {
        let areas = dashboard(Rect::new(0, 0, 40, 7));
        assert!(areas.constrained);
        assert!(areas.header.height > 0);
    }
}
