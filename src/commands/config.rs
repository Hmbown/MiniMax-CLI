//! Config commands: config, set, settings, yolo, trust, logout

use super::CommandResult;
use crate::config::clear_api_key;
use crate::palette;
use crate::settings::Settings;
use crate::tui::app::{App, AppMode, OnboardingState};
use crate::tui::approval::ApprovalMode;

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
         Approval mode:  {}\n\
         Max sub-agents: {}\n\
         Trust mode:     {}\n\
         Auto-compact:   {}\n\
         Total tokens:   {}\n\
         Project doc:    {}",
        app.mode.label(),
        app.model,
        app.workspace.display(),
        if app.allow_shell { "yes" } else { "no" },
        app.approval_mode.label(),
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
             approval_mode: auto | suggest | never\n\n\
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
        "approval_mode" | "approval" => {
            let mode = match value.to_lowercase().as_str() {
                "auto" => Some(ApprovalMode::Auto),
                "suggest" | "suggested" => Some(ApprovalMode::Suggest),
                "never" => Some(ApprovalMode::Never),
                _ => None,
            };
            return match mode {
                Some(m) => {
                    app.approval_mode = m;
                    CommandResult::message(format!("approval_mode = {}", m.label()))
                }
                None => CommandResult::error("Invalid approval_mode. Use: auto, suggest, never"),
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
        "show_thinking" | "thinking" => {
            app.show_thinking = settings.show_thinking;
            app.mark_history_updated();
        }
        "show_tool_details" | "tool_details" => {
            app.show_tool_details = settings.show_tool_details;
            app.mark_history_updated();
        }
        "default_mode" | "mode" => {
            let mode = match settings.default_mode.as_str() {
                "agent" => AppMode::Agent,
                "plan" => AppMode::Plan,
                "yolo" => AppMode::Yolo,
                "rlm" => AppMode::Rlm,
                "duo" => AppMode::Duo,
                _ => AppMode::Normal,
            };
            app.set_mode(mode);
        }
        "max_history" | "history" => {
            app.max_input_history = settings.max_input_history;
        }
        "default_model" => {
            if let Some(ref model) = settings.default_model {
                app.model.clone_from(model);
            }
        }
        "theme" => {
            app.ui_theme = palette::ui_theme(&settings.theme);
            app.mark_history_updated();
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

/// Toggle YOLO mode (shell + trust + auto-approve)
pub fn yolo(app: &mut App) -> CommandResult {
    if app.mode == AppMode::Yolo {
        // Toggle OFF: Return to Normal mode
        app.set_mode(AppMode::Normal);
        CommandResult::message(
            "YOLO mode disabled - returned to Normal mode with suggested approvals".to_string(),
        )
    } else {
        // Toggle ON: Enable YOLO mode
        app.set_mode(AppMode::Yolo);
        CommandResult::message(format!(
            "⚠️  YOLO mode enabled - shell + trust + auto-approve!\n\n\
             WARNING: YOLO mode automatically executes tools without confirmation.\n\
             This includes file operations, shell commands, and code execution.\n\
             Use with caution!"
        ))
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::tui::app::{App, TuiOptions};
    use crate::tui::approval::ApprovalMode;
    use std::path::PathBuf;

    fn create_test_app() -> App {
        let options = TuiOptions {
            model: "test-model".to_string(),
            workspace: PathBuf::from("."),
            allow_shell: false,
            max_subagents: 1,
            skills_dir: PathBuf::from("."),
            memory_path: PathBuf::from("memory.md"),
            notes_path: PathBuf::from("notes.txt"),
            mcp_config_path: PathBuf::from("mcp.json"),
            use_memory: false,
            start_in_agent_mode: false,
            yolo: false,
            resume_session_id: None,
        };
        App::new(options, &Config::default())
    }

    #[test]
    fn test_yolo_command_toggles_on() {
        let mut app = create_test_app();
        // Initially in Normal mode
        assert_eq!(app.mode, AppMode::Normal);
        assert!(!app.yolo);
        // Toggle ON
        let _ = yolo(&mut app);
        assert!(app.allow_shell);
        assert!(app.trust_mode);
        assert!(app.yolo);
        assert_eq!(app.approval_mode, ApprovalMode::Auto);
        assert_eq!(app.mode, AppMode::Yolo);
    }

    #[test]
    fn test_yolo_command_toggles_off() {
        let mut app = create_test_app();
        // Start in YOLO mode
        app.set_mode(AppMode::Yolo);
        assert!(app.yolo);
        assert_eq!(app.mode, AppMode::Yolo);
        // Toggle OFF
        let _ = yolo(&mut app);
        assert_eq!(app.mode, AppMode::Normal);
        assert!(!app.yolo);
        assert!(!app.trust_mode);
        assert_eq!(app.approval_mode, ApprovalMode::Suggest);
    }
}
