use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEventKind};

use crate::app::Command;

pub fn command_from_event(event: Event) -> Option<Command> {
    command_from_event_with_search(event, false)
}

pub fn command_from_event_with_search(
    event: Event,
    variable_search_active: bool,
) -> Option<Command> {
    match event {
        Event::Key(key) => command_from_key_with_search(key, variable_search_active),
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
    command_from_key_with_search(key, false)
}

pub fn command_from_key_with_search(
    key: KeyEvent,
    variable_search_active: bool,
) -> Option<Command> {
    if variable_search_active {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p') {
            return Some(Command::OpenCommandPalette);
        }
        return match key.code {
            KeyCode::Esc => Some(Command::Quit),
            KeyCode::Enter => Some(Command::SubmitVariableSearch),
            KeyCode::Up => Some(Command::SelectVariable(0)),
            KeyCode::Down => Some(Command::SelectVariable(1)),
            KeyCode::Backspace => Some(Command::DeleteInput),
            KeyCode::Char(character) => Some(Command::InputChar(character)),
            _ => None,
        };
    }
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
        KeyCode::Char('r') => Some(Command::ResetZoom),
        KeyCode::Char('x') => Some(Command::OpenAxisOverlay),
        KeyCode::Enter => Some(Command::ActivatePoint),
        KeyCode::Char('g') => Some(Command::ToggleGridMode),
        KeyCode::Char('b') => Some(Command::ToggleLandBorders),
        KeyCode::Char('z') => Some(Command::ToggleColorScaleScope),
        KeyCode::Char('s') => Some(Command::ToggleScale),
        KeyCode::Char(' ') => Some(Command::TogglePlayback),
        KeyCode::Char('{') => Some(Command::PreviousFile),
        KeyCode::Char('}') => Some(Command::NextFile),
        KeyCode::Tab => Some(Command::NextLimitField),
        KeyCode::Backspace => Some(Command::DeleteInput),
        KeyCode::Char(character) => Some(Command::InputChar(character)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{command_from_key, command_from_key_with_search};
    use crate::app::Command;
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
}
