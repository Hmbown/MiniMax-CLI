//! Config commands: config, set, settings, yolo, trust, logout

use super::CommandResult;
use crate::config::clear_api_key;
use crate::settings::Settings;
use crate::tui::app::{App, AppMode, OnboardingState};

/// Display current configuration
pub fn show_config(app: &mut App) -> CommandResult {
    let has_project_doc = app.project_doc.is_some();
    let config_info = format!(
        "Session Configuration:\n\
         ─────────────────────────────\n\
         Mode:           {}\n\
         Model:          {}\n\
         Workspace:      {}\n\
         Shell enabled:  {}\n\
         Max sub-agents: {}\n\
         Trust mode:     {}\n\
         Auto-compact:   {}\n\
         Total tokens:   {}\n\
         Project doc:    {}",
        app.mode.label(),
        app.model,
        app.workspace.display(),
        if app.allow_shell { "yes" } else { "no" },
        app.max_subagents,
        if app.trust_mode { "yes" } else { "no" },
        if app.auto_compact { "yes" } else { "no" },
        app.total_tokens,
        if has_project_doc {
            "loaded"
        } else {
            "not found"
        },
    );
    CommandResult::message(config_info)
}

/// Show persistent settings
pub fn show_settings(_app: &mut App) -> CommandResult {
    match Settings::load() {
        Ok(settings) => CommandResult::message(settings.display()),
        Err(e) => CommandResult::error(format!("Failed to load settings: {e}")),
    }
}

/// Modify a setting at runtime
pub fn set_config(app: &mut App, args: Option<&str>) -> CommandResult {
    let Some(args) = args else {
        let available = Settings::available_settings()
            .iter()
            .map(|(k, d)| format!("  {k}: {d}"))
            .collect::<Vec<_>>()
            .join("\n");
        return CommandResult::message(format!(
            "Usage: /set <key> <value>\n\n\
             Available settings:\n{available}\n\n\
             Session-only settings:\n  \
             model: Current model\n  \
             mode: Current mode\n\n\
             Add --save to persist to settings file."
        ));
    };

    let parts: Vec<&str> = args.splitn(2, ' ').collect();
    if parts.len() < 2 {
        return CommandResult::error("Usage: /set <key> <value>");
    }

    let key = parts[0].to_lowercase();
    let (value, should_save) = if parts[1].ends_with(" --save") {
        (parts[1].trim_end_matches(" --save").trim(), true)
    } else {
        (parts[1].trim(), false)
    };

    // Handle session-only settings first
    match key.as_str() {
        "model" => {
            app.model = value.to_string();
            return CommandResult::message(format!("model = {value}"));
        }
        "mode" => {
            let mode = match value.to_lowercase().as_str() {
                "normal" | "n" => Some(AppMode::Normal),
                "edit" | "e" => Some(AppMode::Edit),
                "agent" | "a" => Some(AppMode::Agent),
                "plan" | "p" => Some(AppMode::Plan),
                "rlm" | "r" => Some(AppMode::Rlm),
                _ => None,
            };
            return match mode {
                Some(m) => {
                    app.set_mode(m);
                    CommandResult::message(format!("mode = {}", m.label()))
                }
                None => CommandResult::error("Invalid mode. Use: normal, edit, agent, plan, rlm"),
            };
        }
        _ => {}
    }

    // Load and update persistent settings
    let mut settings = match Settings::load() {
        Ok(s) => s,
        Err(e) => return CommandResult::error(format!("Failed to load settings: {e}")),
    };

    if let Err(e) = settings.set(&key, value) {
        return CommandResult::error(format!("{e}"));
    }

    // Apply to current session
    match key.as_str() {
        "auto_compact" | "compact" => {
            app.auto_compact = settings.auto_compact;
        }
        "default_model" => {
            if let Some(ref model) = settings.default_model {
                app.model.clone_from(model);
            }
        }
        _ => {}
    }

    // Save if requested
    if should_save {
        if let Err(e) = settings.save() {
            return CommandResult::error(format!("Failed to save: {e}"));
        }
        CommandResult::message(format!("{key} = {value} (saved)"))
    } else {
        CommandResult::message(format!(
            "{key} = {value} (session only, add --save to persist)"
        ))
    }
}

/// Enable YOLO mode (agent + shell)
pub fn yolo(app: &mut App) -> CommandResult {
    app.allow_shell = true;
    app.set_mode(AppMode::Agent);
    CommandResult::message("YOLO mode enabled - agent mode with shell execution!")
}

/// Enable trust mode (file access outside workspace)
pub fn trust(app: &mut App) -> CommandResult {
    app.trust_mode = true;
    CommandResult::message("Trust mode enabled - can access files outside workspace")
}

/// Logout - clear API key and return to onboarding
pub fn logout(app: &mut App) -> CommandResult {
    match clear_api_key() {
        Ok(()) => {
            app.onboarding = OnboardingState::Welcome;
            app.api_key_input.clear();
            app.api_key_cursor = 0;
            CommandResult::message("Logged out. Enter a new API key to continue.")
        }
        Err(e) => CommandResult::error(format!("Failed to clear API key: {e}")),
    }
}
