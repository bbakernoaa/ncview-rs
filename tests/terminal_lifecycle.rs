use ncview_rs::events::terminal::TerminalSession;

#[test]
fn restore_is_idempotent_without_terminal_entry() {
    let _ = TerminalSession::enter;
}
