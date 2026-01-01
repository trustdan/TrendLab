//! Interactive prompt for mode selection.

use std::io::{self, Write};
use std::time::Duration;

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    style::{Color, Print, SetForegroundColor},
    terminal::{self, ClearType},
};

/// The mode selected by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchMode {
    /// Launch the Terminal UI.
    Tui,
    /// Launch the Desktop GUI.
    Gui,
}

/// ASCII art banner for the launcher.
const BANNER: &str = r#"
  _____ ____  _____ _   _ ____  _        _    ____
 |_   _|  _ \| ____| \ | |  _ \| |      / \  | __ )
   | | | |_) |  _| |  \| | | | | |     / _ \ |  _ \
   | | |  _ <| |___| |\  | |_| | |___ / ___ \| |_) |
   |_| |_| \_\_____|_| \_|____/|_____/_/   \_\____/
"#;

/// Display the interactive mode selection prompt.
///
/// Returns `None` if the user cancels (Ctrl+C or Esc).
pub fn show() -> io::Result<Option<LaunchMode>> {
    terminal::enable_raw_mode()?;
    let result = show_inner();
    terminal::disable_raw_mode()?;
    result
}

fn show_inner() -> io::Result<Option<LaunchMode>> {
    let mut stdout = io::stdout();

    // Clear screen
    execute!(
        stdout,
        terminal::Clear(ClearType::All),
        cursor::MoveTo(0, 0)
    )?;

    // Print banner
    execute!(stdout, SetForegroundColor(Color::Cyan))?;
    for line in BANNER.lines() {
        execute!(stdout, Print(line), Print("\r\n"))?;
    }

    // Print subtitle
    execute!(
        stdout,
        SetForegroundColor(Color::DarkGrey),
        Print("            Trend-Following Backtest Lab\r\n"),
        Print("\r\n"),
    )?;

    // Print box
    execute!(
        stdout,
        SetForegroundColor(Color::Blue),
        Print("  ╭─────────────────────────────────────╮\r\n"),
        Print("  │                                     │\r\n"),
    )?;

    execute!(
        stdout,
        Print("  │   Press "),
        SetForegroundColor(Color::Yellow),
        Print("[T]"),
        SetForegroundColor(Color::Blue),
        Print(" for Terminal UI         │\r\n"),
    )?;

    execute!(
        stdout,
        Print("  │   Press "),
        SetForegroundColor(Color::DarkGrey),
        Print("[G]"),
        SetForegroundColor(Color::DarkGrey),
        Print(" for Desktop GUI "),
        SetForegroundColor(Color::Red),
        Print("(deprecated)"),
        SetForegroundColor(Color::Blue),
        Print("│\r\n"),
    )?;

    execute!(
        stdout,
        Print("  │                                     │\r\n"),
        SetForegroundColor(Color::DarkGrey),
        Print("  │   (or --tui / --gui to skip)        │\r\n"),
        SetForegroundColor(Color::Blue),
        Print("  │                                     │\r\n"),
        Print("  ╰─────────────────────────────────────╯\r\n"),
        SetForegroundColor(Color::Reset),
    )?;

    stdout.flush()?;

    // Wait for key press
    loop {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('t') | KeyCode::Char('T') => {
                            clear_and_reset(&mut stdout)?;
                            return Ok(Some(LaunchMode::Tui));
                        }
                        KeyCode::Char('g') | KeyCode::Char('G') => {
                            clear_and_reset(&mut stdout)?;
                            return Ok(Some(LaunchMode::Gui));
                        }
                        KeyCode::Char('c')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            clear_and_reset(&mut stdout)?;
                            return Ok(None);
                        }
                        KeyCode::Esc => {
                            clear_and_reset(&mut stdout)?;
                            return Ok(None);
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

fn clear_and_reset(stdout: &mut io::Stdout) -> io::Result<()> {
    execute!(
        stdout,
        terminal::Clear(ClearType::All),
        cursor::MoveTo(0, 0),
        SetForegroundColor(Color::Reset)
    )
}

/// Display the logging prompt.
///
/// This is shown before the mode selection prompt to ask if the user
/// wants to enable logging. Returns `true` if logging should be enabled.
///
/// Pressing [L] enables logging, any other key continues without logging.
pub fn show_logging_prompt() -> io::Result<bool> {
    terminal::enable_raw_mode()?;
    let result = show_logging_prompt_inner();
    terminal::disable_raw_mode()?;
    result
}

fn show_logging_prompt_inner() -> io::Result<bool> {
    let mut stdout = io::stdout();

    // Clear screen
    execute!(
        stdout,
        terminal::Clear(ClearType::All),
        cursor::MoveTo(0, 0)
    )?;

    // Print banner
    execute!(stdout, SetForegroundColor(Color::Cyan))?;
    for line in BANNER.lines() {
        execute!(stdout, Print(line), Print("\r\n"))?;
    }

    // Print subtitle
    execute!(
        stdout,
        SetForegroundColor(Color::DarkGrey),
        Print("            Trend-Following Backtest Lab\r\n"),
        Print("\r\n"),
    )?;

    // Print logging prompt box
    execute!(
        stdout,
        SetForegroundColor(Color::Blue),
        Print("  ╭─────────────────────────────────────╮\r\n"),
        Print("  │                                     │\r\n"),
    )?;

    execute!(
        stdout,
        Print("  │   Press "),
        SetForegroundColor(Color::Green),
        Print("[L]"),
        SetForegroundColor(Color::Blue),
        Print(" to enable logging       │\r\n"),
    )?;

    execute!(
        stdout,
        SetForegroundColor(Color::DarkGrey),
        Print("  │   Press any other key to continue   │\r\n"),
        SetForegroundColor(Color::Blue),
        Print("  │                                     │\r\n"),
        SetForegroundColor(Color::DarkGrey),
        Print("  │   (or --log / --no-log to skip)     │\r\n"),
        SetForegroundColor(Color::Blue),
        Print("  │                                     │\r\n"),
        Print("  ╰─────────────────────────────────────╯\r\n"),
        SetForegroundColor(Color::Reset),
    )?;

    stdout.flush()?;

    // Wait for any key press
    loop {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('l') | KeyCode::Char('L') => {
                            clear_and_reset(&mut stdout)?;
                            return Ok(true);
                        }
                        KeyCode::Char('c')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            // Ctrl+C - treat as no logging
                            clear_and_reset(&mut stdout)?;
                            return Ok(false);
                        }
                        _ => {
                            // Any other key - continue without logging
                            clear_and_reset(&mut stdout)?;
                            return Ok(false);
                        }
                    }
                }
            }
        }
    }
}
