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
        InputMode::VariableSearch => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p') {
                return Some(Command::OpenCommandPalette);
            }
            match key.code {
                KeyCode::Esc => Some(Command::Quit),
                KeyCode::Enter => Some(Command::SubmitVariableSearch),
                KeyCode::Up => Some(Command::SelectVariable(0)),
                KeyCode::Down => Some(Command::SelectVariable(1)),
                KeyCode::Backspace => Some(Command::DeleteInput),
                KeyCode::Char(character) => Some(Command::InputChar(character)),
                _ => None,
            }
        }
        InputMode::TextOverlay(overlay) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p') {
                return Some(Command::OpenCommandPalette);
            }
            match key.code {
                KeyCode::Esc => Some(Command::Quit),
                KeyCode::Enter => match overlay {
                    Overlay::CommandPalette => Some(Command::ExecuteCommandPalette),
                    Overlay::Limits | Overlay::Filter => Some(Command::ApplyLimitDraft),
                    _ => Some(Command::ActivatePoint),
                },
                KeyCode::Tab => Some(Command::NextLimitField),
                KeyCode::Backspace => Some(Command::DeleteInput),
                KeyCode::Up => match overlay {
                    Overlay::CommandPalette => Some(Command::PaletteMove(-1)),
                    Overlay::Axis => Some(Command::CycleAxis(-1)),
                    _ => Some(Command::NextLimitField),
                },
                KeyCode::Down => match overlay {
                    Overlay::CommandPalette => Some(Command::PaletteMove(1)),
                    Overlay::Axis => Some(Command::CycleAxis(1)),
                    _ => Some(Command::NextLimitField),
                },
                KeyCode::Char(character) => Some(Command::InputChar(character)),
                _ => None,
            }
        }
        InputMode::PlotOverlay => match key.code {
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
        },
        InputMode::Help => match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => Some(Command::Quit),
            _ => None,
        },
        InputMode::Normal => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                match key.code {
                    KeyCode::Up => return Some(Command::Pan { rows: -1, cols: 0 }),
                    KeyCode::Down => return Some(Command::Pan { rows: 1, cols: 0 }),
                    KeyCode::Left => return Some(Command::Pan { rows: 0, cols: -1 }),
                    KeyCode::Right => return Some(Command::Pan { rows: 0, cols: 1 }),
                    _ => {}
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
                KeyCode::Right => Some(Command::MoveTime(1)),
                // Keep the compact sidebar controls usable from the keyboard too.
                // Terminals report shifted angle brackets and plus as character keys.
                KeyCode::Char('<') => Some(Command::MoveTime(-1)),
                KeyCode::Char('>') => Some(Command::MoveTime(1)),
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
                KeyCode::Char('u') => {
                    Some(Command::SetPlotKind(crate::app::PlotKind::VerticalProfile))
                }
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
                KeyCode::Tab => Some(Command::NextLimitField),
                KeyCode::Backspace => Some(Command::DeleteInput),
                KeyCode::Char(character) => Some(Command::InputChar(character)),
                _ => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{command_from_key, command_from_key_with_mode, command_from_key_with_search};
    use crate::app::{Command, InputMode, Overlay};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
}
