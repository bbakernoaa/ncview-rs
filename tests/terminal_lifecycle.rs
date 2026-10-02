use ncview_rs::events::terminal::TerminalSession;

#[test]
fn restore_is_idempotent_without_terminal_entry() {
    // CI usually has no controlling TTY, so entering may fail. When it is
    // available, exercise repeated explicit restoration and Drop restoration.
    if let Ok(mut session) = TerminalSession::enter() {
        session.restore().unwrap();
        session.restore().unwrap();
    }
}

#[test]
fn terminal_restore_while_palette_picker_is_active_preserves_overlay_state() {
    use ncview_rs::{
        app::{AppState, Command, Overlay},
        render::colors::Palette,
    };

    let mut state = AppState::default();
    state.view.palette = Palette::Viridis;
    state.reduce(Command::OpenPalettePicker);
    state.reduce(Command::MovePalettePicker(1));
    let focused = state
        .view
        .palette_picker
        .as_ref()
        .unwrap()
        .focused_palette
        .clone();

    // CI may have no controlling TTY; when one is available, restoration while
    // a modal is active must remain safe and idempotent.
    if let Ok(mut session) = TerminalSession::enter() {
        session.restore().unwrap();
        session.restore().unwrap();
    }

    assert_eq!(state.view.overlay, Some(Overlay::PalettePicker));
    assert_eq!(state.view.palette, Palette::Viridis);
    assert_eq!(
        state.view.palette_picker.as_ref().unwrap().focused_palette,
        focused
    );
    state.reduce(Command::CancelPalettePicker);
    assert!(state.view.overlay.is_none());
}
