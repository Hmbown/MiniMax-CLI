//! Config reload command

use crate::config::Config;
use crate::tui::app::App;

use super::CommandResult;

/// Reload configuration from disk
pub fn reload(app: &mut App) -> CommandResult {
    let profile = std::env::var("MINIMAX_PROFILE").ok();
    let config_path = std::env::var("MINIMAX_CONFIG_PATH")
        .ok()
        .map(std::path::PathBuf::from);

    match Config::load(config_path, profile.as_deref()) {
        Ok(config) => {
            // Apply relevant config changes to the app
            if let Some(model) = &config.default_text_model {
                app.model.clone_from(model);
            }
            app.allow_shell = config.allow_shell();
            app.max_subagents = config.max_subagents();
            app.skills_dir = config.skills_dir();

            // Reload settings
            match crate::settings::Settings::load() {
                Ok(settings) => {
                    app.auto_compact = settings.auto_compact;
                    app.show_thinking = settings.show_thinking;
                    app.show_tool_details = settings.show_tool_details;
                    app.max_input_history = settings.max_input_history;
                    app.ui_theme = crate::palette::ui_theme(&settings.theme);
                }
                Err(e) => {
                    return CommandResult::error(format!(
                        "Config reloaded but failed to load settings: {e}"
                    ));
                }
            }

            CommandResult::message(
                "Configuration reloaded (settings + model + skills dir). Some changes require restart.",
            )
        }
        Err(e) => CommandResult::error(format!("Failed to reload config: {e}")),
    }
}
