//! Keyboard input handling for companion mode.

use crossterm::event::KeyCode;

use super::state::CompanionState;

/// Action to take after handling a key.
pub enum KeyAction {
    /// Continue running.
    Continue,
    /// Quit the companion.
    Quit,
}

/// Handle a key press.
pub fn handle_key(key: KeyCode, state: &mut CompanionState) -> KeyAction {
    match key {
        // Quit
        KeyCode::Char('q') | KeyCode::Char('Q') => KeyAction::Quit,

        // Toggle minimized mode
        KeyCode::Esc => {
            state.toggle_minimized();
            KeyAction::Continue
        }

        // Ignore other keys
        _ => KeyAction::Continue,
    }
}
