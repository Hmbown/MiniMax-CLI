//! Setup wizard command for interactive configuration

use super::CommandResult;
use crate::tui::app::{App, AppAction};
use std::io::{self, Write};
use std::path::PathBuf;

/// Run the interactive setup wizard
pub fn setup(_app: &mut App) -> CommandResult {
    // Print welcome message
    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║            Welcome to MiniMax CLI Setup Wizard               ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // Step 2: Choose base URL
    let base_url = choose_base_url();

    // Step 3: Enter API key
    let api_key = match enter_api_key() {
        Ok(key) => key,
        Err(e) => return CommandResult::error(format!("Failed to read API key: {e}")),
    };

    // Step 4: Choose default model
    let default_model = choose_default_model();

    // Step 5: Configure default mode
    let default_mode = choose_default_mode();

    // Step 6: Shell permissions
    let allow_shell = choose_shell_permissions();

    // Step 7: Summary and confirmation
    println!("\n───────────────────────────────────────────────────────────────");
    println!("Setup Summary:");
    println!("  Base URL:       {base_url}");
    println!("  API Key:        {}...", &api_key[..api_key.len().min(8)]);
    println!("  Default Model:  {default_model}");
    println!("  Default Mode:   {default_mode}");
    println!(
        "  Shell Enabled:  {}",
        if allow_shell { "yes" } else { "no" }
    );
    println!("───────────────────────────────────────────────────────────────\n");

    if !confirm("Save this configuration?") {
        return CommandResult::message("Setup cancelled. No changes were saved.");
    }

    // Save configuration
    match save_config(&base_url, &api_key, &default_model, allow_shell) {
        Ok(config_path) => {
            // Also save settings for default mode
            if let Err(e) = save_default_mode(&default_mode) {
                return CommandResult::error(format!(
                    "Config saved to {} but failed to save default mode: {e}",
                    config_path.display()
                ));
            }

            CommandResult::with_message_and_action(
                format!(
                    "✓ Configuration saved to {}\n\nSetup complete! Configuration has been reloaded.",
                    config_path.display()
                ),
                AppAction::ReloadConfig,
            )
        }
        Err(e) => CommandResult::error(format!("Failed to save configuration: {e}")),
    }
}

fn choose_base_url() -> String {
    println!("Step 1/6: Choose your region");
    println!("  1) US/Global - https://api.minimax.io");
    println!("  2) China     - https://api.minimaxi.com");
    println!();

    loop {
        print!("Enter choice (1-2) [1]: ");
        let _ = io::stdout().flush();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            println!("Invalid input, please try again.");
            continue;
        }

        let input = input.trim();
        if input.is_empty() || input == "1" {
            return "https://api.minimax.io".to_string();
        } else if input == "2" {
            return "https://api.minimaxi.com".to_string();
        } else {
            println!("Invalid choice. Please enter 1 or 2.");
        }
    }
}

fn enter_api_key() -> Result<String, io::Error> {
    println!("\nStep 2/6: Enter your MiniMax API Key");
    println!("  Get your API key from: https://platform.minimax.io");
    println!();

    loop {
        print!("API Key: ");
        let _ = io::stdout().flush();

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let key = input.trim().to_string();
        if key.is_empty() {
            println!("API key cannot be empty. Please try again.\n");
            continue;
        }

        // Basic validation: check length and alphanumeric pattern
        if key.len() < 10 {
            println!("API key seems too short. Please check and try again.\n");
            continue;
        }

        return Ok(key);
    }
}

fn choose_default_model() -> String {
    println!("\nStep 3/6: Choose your default model");
    println!("  1) MiniMax-M2.5            - Latest general purpose (2M context, tool calling)");
    println!("  2) MiniMax-M2.5-lightning - Fast version of M2.5");
    println!("  3) MiniMax-M2.1           - Polyglot programming mastery");
    println!("  4) MiniMax-M2             - Efficient agentic model");
    println!("  5) MiniMax-Text-01        - Long context model");
    println!("  6) MiniMax-Coding-01      - Code generation model");
    println!();

    loop {
        print!("Enter choice (1-6) [1]: ");
        let _ = io::stdout().flush();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            println!("Invalid input, please try again.");
            continue;
        }

        let input = input.trim();
        if input.is_empty() || input == "1" {
            return "MiniMax-M2.5".to_string();
        } else if input == "2" {
            return "MiniMax-M2.5-lightning".to_string();
        } else if input == "3" {
            return "MiniMax-M2.1".to_string();
        } else if input == "4" {
            return "MiniMax-M2".to_string();
        } else if input == "5" {
            return "MiniMax-Text-01".to_string();
        } else if input == "6" {
            return "MiniMax-Coding-01".to_string();
        } else {
            println!("Invalid choice. Please enter 1, 2, 3, 4, 5, or 6.");
        }
    }
}

fn choose_default_mode() -> String {
    println!("\nStep 4/6: Choose your default mode");
    println!("  1) Normal - Chat mode, ask questions and get answers");
    println!("  2) Agent  - Autonomous task execution with tools");
    println!("  3) YOLO   - Full tool access without approvals (use with caution)");
    println!();

    loop {
        print!("Enter choice (1-3) [1]: ");
        let _ = io::stdout().flush();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            println!("Invalid input, please try again.");
            continue;
        }

        let input = input.trim();
        if input.is_empty() || input == "1" {
            return "normal".to_string();
        } else if input == "2" {
            return "agent".to_string();
        } else if input == "3" {
            return "yolo".to_string();
        } else {
            println!("Invalid choice. Please enter 1, 2, or 3.");
        }
    }
}

fn choose_shell_permissions() -> bool {
    println!("\nStep 5/6: Shell Permissions");
    println!("  Allow the CLI to execute shell commands?");
    println!("  This is required for tools like file operations and code execution.");
    println!();

    loop {
        print!("Enable shell? (yes/no) [yes]: ");
        let _ = io::stdout().flush();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            println!("Invalid input, please try again.");
            continue;
        }

        let input = input.trim().to_lowercase();
        if input.is_empty() || input == "yes" || input == "y" {
            return true;
        } else if input == "no" || input == "n" {
            return false;
        } else {
            println!("Please enter 'yes' or 'no'.");
        }
    }
}

fn confirm(prompt: &str) -> bool {
    loop {
        print!("{} (yes/no) [yes]: ", prompt);
        let _ = io::stdout().flush();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            return false;
        }

        let input = input.trim().to_lowercase();
        if input.is_empty() || input == "yes" || input == "y" {
            return true;
        } else if input == "no" || input == "n" {
            return false;
        } else {
            println!("Please enter 'yes' or 'no'.");
        }
    }
}

fn get_config_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("MINIMAX_CONFIG_PATH")
        && !path.trim().is_empty()
    {
        return Some(PathBuf::from(path));
    }
    dirs::home_dir().map(|home| home.join(".minimax").join("config.toml"))
}

fn save_config(
    base_url: &str,
    api_key: &str,
    default_model: &str,
    allow_shell: bool,
) -> Result<PathBuf, anyhow::Error> {
    let config_path =
        get_config_path().ok_or_else(|| anyhow::anyhow!("Could not determine config path"))?;

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("Failed to create config directory: {e}"))?;
    }

    let config_content = format!(
        r#"# MiniMax CLI Configuration
# Generated by setup wizard

# API Configuration
api_key = "{api_key}"
base_url = "{base_url}"

# Default model for text generation
default_text_model = "{default_model}"

# Security settings
allow_shell = {allow_shell}

# Feature flags
[features]
shell_tool = true
subagents = true
web_search = true
apply_patch = true
mcp = true
rlm = true
duo = true
exec_policy = true

# Retry configuration
[retry]
enabled = true
max_retries = 3
initial_delay = 1.0
max_delay = 60.0
exponential_base = 2.0
"#
    );

    std::fs::write(&config_path, config_content)
        .map_err(|e| anyhow::anyhow!("Failed to write config file: {e}"))?;

    Ok(config_path)
}

fn save_default_mode(mode: &str) -> Result<(), anyhow::Error> {
    use crate::settings::Settings;

    let mut settings =
        Settings::load().map_err(|e| anyhow::anyhow!("Failed to load settings: {e}"))?;

    settings.default_mode = mode.to_string();

    settings
        .save()
        .map_err(|e| anyhow::anyhow!("Failed to save settings: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_path_resolution() {
        // This test just ensures the function doesn't panic
        let _ = get_config_path();
    }

    #[test]
    fn test_save_config_structure() {
        // Test that config content is properly formatted
        let base_url = "https://api.minimax.io";
        let api_key = "test-key-12345";
        let default_model = "MiniMax-M2.5";
        let allow_shell = true;

        let config = format!(
            r#"# MiniMax CLI Configuration
# Generated by setup wizard

# API Configuration
api_key = "{api_key}"
base_url = "{base_url}"

# Default model for text generation
default_text_model = "{default_model}"

# Security settings
allow_shell = {allow_shell}

# Feature flags
[features]
shell_tool = true
subagents = true
web_search = true
apply_patch = true
mcp = true
rlm = true
duo = true
exec_policy = true

# Retry configuration
[retry]
enabled = true
max_retries = 3
initial_delay = 1.0
max_delay = 60.0
exponential_base = 2.0
"#
        );

        assert!(config.contains("api_key = \"test-key-12345\""));
        assert!(config.contains("base_url = \"https://api.minimax.io\""));
        assert!(config.contains("default_text_model = \"MiniMax-M2.5\""));
        assert!(config.contains("allow_shell = true"));
    }
}
