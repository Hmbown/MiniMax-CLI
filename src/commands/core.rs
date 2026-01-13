//! Core commands: help, clear, exit, mode, model

use std::fmt::Write;

use crate::tools::plan::PlanState;
use crate::tui::app::{App, AppAction, AppMode};

use super::{CommandResult, rlm};

/// Show help information
pub fn help(app: &mut App, topic: Option<&str>) -> CommandResult {
    if let Some(topic) = topic {
        // Show help for specific command
        if let Some(cmd) = super::get_command_info(topic) {
            let mut help = format!(
                "{}\n\n  {}\n\n  Usage: {}",
                cmd.name, cmd.description, cmd.usage
            );
            if !cmd.aliases.is_empty() {
                let _ = write!(help, "\n  Aliases: {}", cmd.aliases.join(", "));
            }
            return CommandResult::message(help);
        }
        return CommandResult::error(format!("Unknown command: {topic}"));
    }

    // Show help overlay
    app.show_help = true;
    CommandResult::ok()
}

/// Clear conversation history
pub fn clear(app: &mut App) -> CommandResult {
    app.history.clear();
    app.mark_history_updated();
    app.api_messages.clear();
    app.transcript_selection.clear();
    app.clear_todos();
    if let Ok(mut plan) = app.plan_state.lock() {
        *plan = PlanState::default();
    }
    app.tool_log.clear();
    CommandResult::message("Conversation cleared")
}

/// Exit the application
pub fn exit() -> CommandResult {
    CommandResult::action(AppAction::Quit)
}

/// Switch or view current mode
pub fn mode(app: &mut App, mode_name: Option<&str>) -> CommandResult {
    if let Some(mode_str) = mode_name {
        let new_mode = match mode_str.to_lowercase().as_str() {
            "normal" | "n" => Some(AppMode::Normal),
            "edit" | "e" => Some(AppMode::Edit),
            "agent" | "a" => Some(AppMode::Agent),
            "plan" | "p" => Some(AppMode::Plan),
            "rlm" | "r" => Some(AppMode::Rlm),
            _ => None,
        };

        match new_mode {
            Some(m) => {
                app.set_mode(m);
                if m == AppMode::Rlm {
                    let mut msg = String::from("Switched to RLM mode.\n");
                    msg.push_str(&rlm::welcome_message());
                    CommandResult::message(msg)
                } else {
                    CommandResult::message(format!("Switched to {} mode", m.label()))
                }
            }
            None => CommandResult::error(format!(
                "Unknown mode: {mode_str}. Use: normal, edit, agent, plan, rlm"
            )),
        }
    } else {
        CommandResult::message(format!(
            "Current mode: {} - {}",
            app.mode.label(),
            app.mode.description()
        ))
    }
}

/// Switch or view current model
pub fn model(app: &mut App, model_name: Option<&str>) -> CommandResult {
    if let Some(name) = model_name {
        let old_model = app.model.clone();
        app.model = name.to_string();
        CommandResult::message(format!("Model changed: {old_model} → {name}"))
    } else {
        CommandResult::message(format!("Current model: {}", app.model))
    }
}

/// List sub-agent status from the engine
pub fn subagents(_app: &mut App) -> CommandResult {
    CommandResult::with_message_and_action("Fetching sub-agent status...", AppAction::ListSubAgents)
}

/// Show `MiniMax` dashboard and docs links
pub fn minimax_links() -> CommandResult {
    CommandResult::message(
        "MiniMax Links:\n\
─────────────────────────────\n\
Dashboard: https://platform.minimax.io\n\
Docs:      https://platform.minimax.io/docs\n\n\
Tip: API keys are available in the dashboard console.",
    )
}
