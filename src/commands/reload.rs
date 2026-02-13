//! Config reload command

use crate::tui::app::{App, AppAction};

use super::CommandResult;

/// Reload configuration from disk
pub fn reload(_app: &mut App) -> CommandResult {
    CommandResult::with_message_and_action(
        "Reloading configuration and runtime state...",
        AppAction::ReloadConfig,
    )
}
