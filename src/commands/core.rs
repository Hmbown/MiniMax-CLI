//! Core commands: help, clear, exit, model

use std::fmt::Write;

use crate::tools::plan::PlanState;
use crate::tui::app::{App, AppAction};
use crate::tui::views::{HelpView, ModalKind};

use super::CommandResult;

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
    if app.view_stack.top_kind() != Some(ModalKind::Help) {
        app.view_stack.push(HelpView::new());
    }
    CommandResult::ok()
}

/// Clear conversation history
pub fn clear(app: &mut App) -> CommandResult {
    app.history.clear();
    app.mark_history_updated();
    app.api_messages.clear();
    app.transcript_selection.clear();
    app.total_conversation_tokens = 0;
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

use crate::settings::Settings;
use crate::tui::model_picker::validate_model;

/// Switch or view current model
/// 
/// When called without arguments, opens an interactive model picker.
/// When called with an argument, validates and sets the model directly.
pub fn model(app: &mut App, model_name: Option<&str>) -> CommandResult {
    if let Some(name) = model_name {
        // Validate the model name
        if let Some(model_info) = validate_model(name) {
            let old_model = app.model.clone();
            let new_model = model_info.id.to_string();
            app.model = new_model.clone();
            
            // Persist to settings
            let mut settings = Settings::load().unwrap_or_default();
            settings.default_model = Some(new_model.clone());
            if let Err(e) = settings.save() {
                return CommandResult::message(format!(
                    "Model changed: {old_model} → {new_model} (failed to save: {e})"
                ));
            }
            
            CommandResult::message(format!(
                "Model changed: {old_model} → {new_model} (saved)\n\n{}",
                model_info.description
            ))
        } else {
            // Invalid model - show available models
            let available = crate::tui::model_picker::AVAILABLE_MODELS
                .iter()
                .map(|m| format!("  • {} - {}", m.id, m.name))
                .collect::<Vec<_>>()
                .join("\n");
            CommandResult::error(format!(
                "Unknown model: '{name}'\n\nAvailable models:\n{available}"
            ))
        }
    } else {
        // No argument - open the interactive picker
        CommandResult::action(crate::tui::app::AppAction::OpenModelPicker)
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
