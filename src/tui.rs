use std::io;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::event::DisableMouseCapture;
use crossterm::cursor::{EnableBlinking, SetCursorStyle, Show};
use crossterm::event::EnableMouseCapture;
use crossterm::{execute, ExecutableCommand};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

pub fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    // Request blinking cursor; some terminals may ignore this.
    let _ = stdout.execute(EnableBlinking);
    let _ = stdout.execute(SetCursorStyle::BlinkingBlock);
    let _ = stdout.execute(EnableMouseCapture);
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

pub fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), DisableMouseCapture, LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

pub fn install_panic_hook() {
    // Install a panic hook to restore the terminal state if we panic
    std::panic::set_hook(Box::new(|info| {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = stdout.execute(DisableMouseCapture);
        let _ = stdout.execute(LeaveAlternateScreen);
        let _ = stdout.execute(Show);
        // Print the panic info to stderr after attempting to restore
        eprintln!("panic: {}", info);
    }));
}
