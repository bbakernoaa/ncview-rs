use std::io::{self, Write};

use crossterm::{
    cursor::{Hide, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

pub struct TerminalSession {
    active: bool,
}

impl TerminalSession {
    pub fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture, Hide)?;
        Ok(Self { active: true })
    }

    pub fn restore(&mut self) -> io::Result<()> {
        if self.active {
            disable_raw_mode()?;
            let mut stdout = io::stdout();
            execute!(stdout, Show, DisableMouseCapture, LeaveAlternateScreen)?;
            stdout.flush()?;
            self.active = false;
        }
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}
