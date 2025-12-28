//! Companion mode for monitoring GUI activity from the terminal.

pub mod input;
pub mod state;
pub mod view;

use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use tokio::sync::mpsc;

use crate::ipc::{CompanionEvent, CompanionServer};
use state::CompanionState;

/// Start the IPC server and return the address to connect to.
///
/// This binds to an ephemeral port on localhost.
pub async fn start_server() -> anyhow::Result<(CompanionServer, String)> {
    let server = CompanionServer::bind(0).await?;
    let addr = server.local_addr().to_string();
    Ok((server, addr))
}

/// Run the companion mode.
///
/// This displays a terminal UI showing GUI activity and exits when the GUI disconnects.
pub async fn run(server: CompanionServer, gui_pid: u32) -> anyhow::Result<()> {
    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create state
    let mut state = CompanionState::new(gui_pid);

    // Create channel for IPC events
    let (tx, mut rx) = mpsc::channel::<CompanionEvent>(100);

    // Spawn connection handler
    let accept_handle = tokio::spawn(async move {
        loop {
            if let Err(e) = server.accept_and_forward(tx.clone()).await {
                // Log error but continue accepting
                eprintln!("Connection error: {}", e);
            }
        }
    });

    // Main event loop
    let result = run_loop(&mut terminal, &mut state, &mut rx).await;

    // Clean up
    accept_handle.abort();
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &mut CompanionState,
    rx: &mut mpsc::Receiver<CompanionEvent>,
) -> anyhow::Result<()> {
    loop {
        // Draw UI
        terminal.draw(|frame| {
            view::render(frame, state);
        })?;

        // Check for IPC events (non-blocking)
        while let Ok(event) = rx.try_recv() {
            let should_exit = matches!(event, CompanionEvent::Shutdown);
            state.apply_event(event);
            if should_exit {
                return Ok(());
            }
        }

        // Check for keyboard input (with timeout)
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match input::handle_key(key.code, state) {
                        input::KeyAction::Quit => return Ok(()),
                        input::KeyAction::Continue => {}
                    }
                }
            }
        }

        // Check if GUI process is still running
        if !is_process_running(state.gui_pid()) {
            state.set_disconnected();
            // Give user time to see the message
            tokio::time::sleep(Duration::from_secs(2)).await;
            return Ok(());
        }
    }
}

/// Check if a process with the given PID is still running.
fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // On Unix, we can use kill with signal 0 to check if process exists
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };

        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return false;
            }

            let mut exit_code: u32 = 0;
            let result =
                GetExitCodeProcess(handle, &mut exit_code) != 0 && exit_code == STILL_ACTIVE as u32;
            CloseHandle(handle);
            result
        }
    }
}
