//! Pin commands: pin, unpin, pins

use crate::tui::app::{App, PinSource};

use super::CommandResult;

/// Pin a message from history.
/// `/pin` - pin the last assistant message
/// `/pin <n>` - pin the nth message from history (0-indexed)
pub fn pin(app: &mut App, arg: Option<&str>) -> CommandResult {
    let cell_idx = match arg {
        None => {
            // Find the last assistant message
            let mut found_idx = None;
            for (idx, cell) in app.history.iter().enumerate().rev() {
                if matches!(cell, crate::tui::history::HistoryCell::Assistant { .. }) {
                    found_idx = Some(idx);
                    break;
                }
            }
            match found_idx {
                Some(idx) => idx,
                None => return CommandResult::error("No assistant message found to pin"),
            }
        }
        Some(n) => match n.parse::<usize>() {
            Ok(idx) => idx,
            Err(_) => return CommandResult::error(format!("Invalid index: {n}")),
        },
    };

    if app.pin_message(cell_idx) {
        let pin_count = app.pin_count();
        CommandResult::message(format!(
            "📌 Message pinned ({} {}). Type /pins to see all pinned messages.",
            pin_count,
            if pin_count == 1 { "pin" } else { "pins" }
        ))
    } else {
        CommandResult::error(format!(
            "Cannot pin message at index {}. Only user and assistant messages can be pinned.",
            cell_idx
        ))
    }
}

/// List all pinned messages.
/// `/pins` - show all pinned messages
pub fn list_pins(app: &App) -> CommandResult {
    let pins = app.list_pins();
    if pins.is_empty() {
        return CommandResult::message("No pinned messages. Use /pin to pin a message.");
    }

    let mut output = String::from("📌 Pinned Messages:\n");
    output.push_str("────────────────────\n");

    for (idx, pin) in pins.iter().enumerate() {
        let source_label = match pin.source {
            PinSource::User => "You",
            PinSource::Assistant => "MiniMax",
        };
        let preview = pin.preview();
        output.push_str(&format!("{}. [{}] {}\n", idx + 1, source_label, preview));
    }

    output.push_str("\nUse /unpin <n> to remove a pin, or /unpin all to clear all.");
    CommandResult::message(output)
}

/// Unpin a message.
/// `/unpin <n>` - unpin the nth pinned message (1-indexed)
/// `/unpin all` - clear all pinned messages
pub fn unpin(app: &mut App, arg: Option<&str>) -> CommandResult {
    let arg = match arg {
        Some(a) => a,
        None => return CommandResult::error("Usage: /unpin <n> or /unpin all"),
    };

    if arg == "all" {
        let count = app.pin_count();
        app.clear_pins();
        return CommandResult::message(format!("Cleared all {} pinned messages.", count));
    }

    match arg.parse::<usize>() {
        Ok(0) => CommandResult::error("Index must be 1 or greater."),
        Ok(idx) => {
            let zero_based = idx - 1; // Convert from 1-indexed to 0-indexed
            if app.unpin_message(zero_based) {
                CommandResult::message(format!("Unpinned message {}.", idx))
            } else {
                CommandResult::error(format!(
                    "No pinned message at index {}. Use /pins to see all pins.",
                    idx
                ))
            }
        }
        Err(_) => {
            CommandResult::error(format!("Invalid argument: {}. Use a number or 'all'.", arg))
        }
    }
}
