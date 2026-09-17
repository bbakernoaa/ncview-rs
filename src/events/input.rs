use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};

use crate::app::{Command, InputMode, Overlay};

pub fn command_from_event(event: Event) -> Option<Command> {
    command_from_event_with_mode(event, InputMode::Normal)
}

pub fn command_from_event_with_search(
    event: Event,
    variable_search_active: bool,
) -> Option<Command> {
    let mode = if variable_search_active {
        InputMode::VariableSearch
    } else {
        InputMode::Normal
    };
    command_from_event_with_mode(event, mode)
}

pub fn command_from_event_with_mode(event: Event, mode: InputMode) -> Option<Command> {
    match event {
        Event::Key(key) => command_from_key_with_mode(key, mode),
        Event::Resize(width, height) => Some(Command::Resize { width, height }),
        Event::Mouse(mouse) if matches!(mouse.kind, MouseEventKind::Moved) => {
            Some(Command::Pointer {
                x: mouse.column,
                y: mouse.row,
            })
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::ScrollUp => {
            Some(Command::PointerScroll {
                x: mouse.column,
                y: mouse.row,
                delta: -1,
            })
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::ScrollDown => {
            Some(Command::PointerScroll {
                x: mouse.column,
                y: mouse.row,
                delta: 1,
            })
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
            Some(Command::BeginDrag {
                x: mouse.column,
                y: mouse.row,
                zoom: mouse.modifiers.contains(KeyModifiers::SHIFT),
            })
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Drag(MouseButton::Left) => {
            Some(Command::UpdateDrag {
                x: mouse.column,
                y: mouse.row,
            })
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Up(MouseButton::Left) => {
            Some(Command::MouseRelease {
                x: mouse.column,
                y: mouse.row,
            })
        }
        Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Right) => {
            Some(Command::MouseClick {
                x: mouse.column,
                y: mouse.row,
                right: true,
            })
        }
        _ => None,
    }
}

pub fn command_from_key(key: KeyEvent) -> Option<Command> {
    command_from_key_with_mode(key, InputMode::Normal)
}

pub fn command_from_key_with_search(
    key: KeyEvent,
    variable_search_active: bool,
) -> Option<Command> {
    let mode = if variable_search_active {
        InputMode::VariableSearch
    } else {
        InputMode::Normal
    };
    command_from_key_with_mode(key, mode)
}

pub fn command_from_key_with_mode(key: KeyEvent, mode: InputMode) -> Option<Command> {
    match mode {
        InputMode::VariableSearch => variable_search_command(key),
        InputMode::Sidebar => sidebar_command(key),
        InputMode::TextOverlay(overlay) => text_overlay_command(key, overlay),
        InputMode::PlotOverlay => plot_overlay_command(key),
        InputMode::Help => help_command(key),
        InputMode::Normal => normal_command(key),
    }
}

fn command_palette_shortcut(key: KeyEvent) -> Option<Command> {
    (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p'))
        .then_some(Command::OpenCommandPalette)
}

fn variable_search_command(key: KeyEvent) -> Option<Command> {
    command_palette_shortcut(key).or(match key.code {
        KeyCode::Esc => Some(Command::Quit),
        KeyCode::Enter => Some(Command::SubmitVariableSearch),
        KeyCode::Up => Some(Command::SelectVariable(0)),
        KeyCode::Down => Some(Command::SelectVariable(1)),
        KeyCode::Backspace => Some(Command::DeleteInput),
        KeyCode::Char(character) => Some(Command::InputChar(character)),
        _ => None,
    })
}

fn text_overlay_command(key: KeyEvent, overlay: Overlay) -> Option<Command> {
    command_palette_shortcut(key).or(match key.code {
        KeyCode::Esc => Some(Command::Quit),
        KeyCode::Enter => Some(match overlay {
            Overlay::CommandPalette => Command::ExecuteCommandPalette,
            Overlay::Limits | Overlay::Filter => Command::ApplyLimitDraft,
            _ => Command::ActivatePoint,
        }),
        KeyCode::Tab | KeyCode::Backspace => Some(match key.code {
            KeyCode::Tab => Command::NextLimitField,
            _ => Command::DeleteInput,
        }),
        KeyCode::Up => Some(text_overlay_direction(overlay, -1)),
        KeyCode::Down => Some(text_overlay_direction(overlay, 1)),
        KeyCode::Char(character) => Some(Command::InputChar(character)),
        _ => None,
    })
}

fn text_overlay_direction(overlay: Overlay, direction: isize) -> Command {
    match overlay {
        Overlay::CommandPalette => Command::PaletteMove(direction),
        Overlay::Axis => Command::CycleAxis(direction),
        _ => Command::NextLimitField,
    }
}

fn plot_overlay_command(key: KeyEvent) -> Option<Command> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Some(Command::Quit),
        KeyCode::Tab => Some(Command::NextLimitField),
        KeyCode::Up | KeyCode::Left => Some(Command::CyclePlotAxis(-1)),
        KeyCode::Down | KeyCode::Right => Some(Command::CyclePlotAxis(1)),
        KeyCode::Char('t') => Some(Command::SetPlotKind(crate::app::PlotKind::TimeSeries)),
        KeyCode::Char('d') => Some(Command::SetPlotKind(crate::app::PlotKind::Scatter)),
        KeyCode::Char('h') => Some(Command::SetPlotKind(crate::app::PlotKind::Histogram)),
        KeyCode::Char('k') => Some(Command::SetPlotKind(crate::app::PlotKind::Cdf)),
        KeyCode::Char('u') => Some(Command::SetPlotKind(crate::app::PlotKind::VerticalProfile)),
        KeyCode::Char('m') => Some(Command::TogglePointSelection),
        KeyCode::Enter => Some(Command::ActivatePoint),
        _ => None,
    }
}

fn help_command(key: KeyEvent) -> Option<Command> {
    matches!(
        key.code,
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?')
    )
    .then_some(Command::Quit)
}

fn sidebar_command(key: KeyEvent) -> Option<Command> {
    match key.code {
        KeyCode::Tab | KeyCode::Esc => Some(Command::ToggleSidebarFocus),
        KeyCode::Enter => Some(Command::ApplyDepthCursor),
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('[') => Some(Command::MoveDepthCursor(-1)),
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char(']') => {
            Some(Command::MoveDepthCursor(1))
        }
        KeyCode::PageUp => Some(Command::MoveDepthCursor(-7)),
        KeyCode::PageDown => Some(Command::MoveDepthCursor(7)),
        _ => None,
    }
}

fn normal_command(key: KeyEvent) -> Option<Command> {
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        let pan = match key.code {
            KeyCode::Up => Some((-1, 0)),
            KeyCode::Down => Some((1, 0)),
            KeyCode::Left => Some((0, -1)),
            KeyCode::Right => Some((0, 1)),
            _ => None,
        };
        if let Some((rows, cols)) = pan {
            return Some(Command::Pan { rows, cols });
        }
    }
    match key.code {
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Command::OpenCommandPalette)
        }
        KeyCode::Char(':') => Some(Command::OpenCommandPalette),
        KeyCode::Char('/') => Some(Command::OpenVariableSearch),
        KeyCode::Char('q') | KeyCode::Esc => Some(Command::Quit),
        KeyCode::Char('?') => Some(Command::ToggleHelp),
        KeyCode::Up => Some(Command::SelectVariable(0)),
        KeyCode::Down => Some(Command::SelectVariable(1)),
        KeyCode::Left => Some(Command::MoveTime(-1)),
        KeyCode::Right | KeyCode::Char('>') => Some(Command::MoveTime(1)),
        KeyCode::Char('<') => Some(Command::MoveTime(-1)),
        KeyCode::Char('-') => Some(Command::DecreasePlaybackSpeed),
        KeyCode::Char('+') => Some(Command::IncreasePlaybackSpeed),
        KeyCode::Char('[') => Some(Command::MoveDepth(-1)),
        KeyCode::Char(']') => Some(Command::MoveDepth(1)),
        KeyCode::Char('c') => Some(Command::CyclePalette),
        KeyCode::Char('v') => Some(Command::TogglePaletteReverse),
        KeyCode::Char('i') => Some(Command::CycleImageFilter),
        KeyCode::Char('e') => Some(Command::ExportCurrent),
        KeyCode::Char('a') => Some(Command::AutomaticLimits),
        KeyCode::Char('l') => Some(Command::OpenLimits),
        KeyCode::Char('f') => Some(Command::OpenFilter),
        KeyCode::Char('p') => Some(Command::OpenPlot),
        KeyCode::Char('t') => Some(Command::SetPlotKind(crate::app::PlotKind::TimeSeries)),
        KeyCode::Char('d') => Some(Command::SetPlotKind(crate::app::PlotKind::Scatter)),
        KeyCode::Char('h') => Some(Command::SetPlotKind(crate::app::PlotKind::Histogram)),
        KeyCode::Char('k') => Some(Command::SetPlotKind(crate::app::PlotKind::Cdf)),
        KeyCode::Char('u') => Some(Command::SetPlotKind(crate::app::PlotKind::VerticalProfile)),
        KeyCode::Char('r') => Some(Command::ResetZoom),
        KeyCode::Char('x') => Some(Command::OpenAxisOverlay),
        KeyCode::Enter => Some(Command::ActivatePoint),
        KeyCode::Char('g') => Some(Command::ToggleGridMode),
        KeyCode::Char('b') => Some(Command::ToggleLandBorders),
        KeyCode::Char('z') => Some(Command::ToggleColorScaleScope),
        KeyCode::Char('s') => Some(Command::ToggleScale),
        KeyCode::Char('m') => Some(Command::TogglePointSelection),
        KeyCode::Char(' ') => Some(Command::TogglePlayback),
        KeyCode::Char('{') => Some(Command::PreviousFile),
        KeyCode::Char('}') => Some(Command::NextFile),
        KeyCode::Tab => Some(Command::ToggleSidebarFocus),
        KeyCode::Backspace => Some(Command::DeleteInput),
        KeyCode::Char(character) => Some(Command::InputChar(character)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        command_from_event_with_mode, command_from_key, command_from_key_with_mode,
        command_from_key_with_search,
    };
    use crate::app::{Command, InputMode, Overlay};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};

    #[test]
    fn navigation_keys_map_to_domain_commands() {
        assert_eq!(
            command_from_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
            Some(Command::MoveTime(1))
        );
        assert_eq!(
            command_from_key(KeyEvent::new(KeyCode::Char(']'), KeyModifiers::NONE)),
            Some(Command::MoveDepth(1))
        );
        assert_eq!(
            command_from_key(KeyEvent::new(KeyCode::Char('<'), KeyModifiers::SHIFT)),
            Some(Command::MoveTime(-1))
        );
        assert_eq!(
            command_from_key(KeyEvent::new(KeyCode::Char('>'), KeyModifiers::SHIFT)),
            Some(Command::MoveTime(1))
        );
        assert_eq!(
            command_from_key(KeyEvent::new(KeyCode::Char('-'), KeyModifiers::NONE)),
            Some(Command::DecreasePlaybackSpeed)
        );
        assert_eq!(
            command_from_key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::SHIFT)),
            Some(Command::IncreasePlaybackSpeed)
        );
        assert_eq!(
            command_from_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)),
            Some(Command::OpenCommandPalette)
        );
        assert_eq!(
            command_from_key(KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE)),
            Some(Command::OpenCommandPalette)
        );
        assert_eq!(
            command_from_key_with_search(
                KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
                true,
            ),
            Some(Command::InputChar('q'))
        );
        assert_eq!(
            command_from_key(KeyEvent::new(KeyCode::Char('{'), KeyModifiers::NONE)),
            Some(Command::PreviousFile)
        );
        assert_eq!(
            command_from_key(KeyEvent::new(KeyCode::Char('}'), KeyModifiers::NONE)),
            Some(Command::NextFile)
        );
    }

    #[test]
    fn enter_submits_numeric_overlays_without_becoming_plot_activation() {
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            command_from_key_with_mode(enter, InputMode::TextOverlay(Overlay::Limits)),
            Some(Command::ApplyLimitDraft)
        );
        assert_eq!(
            command_from_key_with_mode(enter, InputMode::TextOverlay(Overlay::Filter)),
            Some(Command::ApplyLimitDraft)
        );
    }

    #[test]
    fn tab_toggles_sidebar_focus_in_normal_mode() {
        assert_eq!(
            command_from_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            Some(Command::ToggleSidebarFocus)
        );
    }

    #[test]
    fn sidebar_mode_keys_navigate_the_level_list() {
        let map = |code| {
            command_from_key_with_mode(KeyEvent::new(code, KeyModifiers::NONE), InputMode::Sidebar)
        };
        assert_eq!(map(KeyCode::Up), Some(Command::MoveDepthCursor(-1)));
        assert_eq!(map(KeyCode::Char('k')), Some(Command::MoveDepthCursor(-1)));
        assert_eq!(map(KeyCode::Down), Some(Command::MoveDepthCursor(1)));
        assert_eq!(map(KeyCode::Char('j')), Some(Command::MoveDepthCursor(1)));
        assert_eq!(map(KeyCode::PageUp), Some(Command::MoveDepthCursor(-7)));
        assert_eq!(map(KeyCode::PageDown), Some(Command::MoveDepthCursor(7)));
        assert_eq!(map(KeyCode::Char('[')), Some(Command::MoveDepthCursor(-1)));
        assert_eq!(map(KeyCode::Char(']')), Some(Command::MoveDepthCursor(1)));
        assert_eq!(map(KeyCode::Enter), Some(Command::ApplyDepthCursor));
        assert_eq!(map(KeyCode::Tab), Some(Command::ToggleSidebarFocus));
        assert_eq!(map(KeyCode::Esc), Some(Command::ToggleSidebarFocus));
    }

    #[test]
    fn scroll_wheel_becomes_pointer_scroll() {
        use crossterm::event::{Event, MouseEvent};
        let scroll = |kind| {
            command_from_event_with_mode(
                Event::Mouse(MouseEvent {
                    kind,
                    column: 5,
                    row: 12,
                    modifiers: KeyModifiers::NONE,
                }),
                InputMode::Normal,
            )
        };
        assert_eq!(
            scroll(MouseEventKind::ScrollUp),
            Some(Command::PointerScroll {
                x: 5,
                y: 12,
                delta: -1
            })
        );
        assert_eq!(
            scroll(MouseEventKind::ScrollDown),
            Some(Command::PointerScroll {
                x: 5,
                y: 12,
                delta: 1
            })
        );
    }
}
