//! Slash command registry and dispatch system
//!
//! This module provides a modular command system inspired by Codex-rs.
//! Commands are organized by category and dispatched through a central registry.

mod config;
mod core;
mod debug;
mod init;
mod queue;
mod session;
mod skills;

use crate::tui::app::{App, AppAction};

/// Result of executing a command
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// Optional message to display to the user
    pub message: Option<String>,
    /// Optional action for the app to take
    pub action: Option<AppAction>,
}

impl CommandResult {
    /// Create an empty result (command succeeded with no output)
    pub fn ok() -> Self {
        Self {
            message: None,
            action: None,
        }
    }

    /// Create a result with just a message
    pub fn message(msg: impl Into<String>) -> Self {
        Self {
            message: Some(msg.into()),
            action: None,
        }
    }

    /// Create a result with an action
    pub fn action(action: AppAction) -> Self {
        Self {
            message: None,
            action: Some(action),
        }
    }

    /// Create a result with both message and action
    #[allow(dead_code)]
    pub fn with_message_and_action(msg: impl Into<String>, action: AppAction) -> Self {
        Self {
            message: Some(msg.into()),
            action: Some(action),
        }
    }

    /// Create an error message result
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            message: Some(format!("Error: {}", msg.into())),
            action: None,
        }
    }
}

/// Command metadata for help and autocomplete
#[derive(Debug, Clone, Copy)]
pub struct CommandInfo {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    pub usage: &'static str,
}

/// All registered commands
pub const COMMANDS: &[CommandInfo] = &[
    // Core commands
    CommandInfo {
        name: "help",
        aliases: &["?"],
        description: "Show help information",
        usage: "/help [command]",
    },
    CommandInfo {
        name: "clear",
        aliases: &[],
        description: "Clear conversation history",
        usage: "/clear",
    },
    CommandInfo {
        name: "exit",
        aliases: &["quit", "q"],
        description: "Exit the application",
        usage: "/exit",
    },
    CommandInfo {
        name: "mode",
        aliases: &[],
        description: "Switch or view current mode",
        usage: "/mode [normal|edit|agent|plan|rlm]",
    },
    CommandInfo {
        name: "model",
        aliases: &[],
        description: "Switch or view current model",
        usage: "/model [name]",
    },
    CommandInfo {
        name: "queue",
        aliases: &["queued"],
        description: "View or edit queued messages",
        usage: "/queue [list|edit <n>|drop <n>|clear]",
    },
    CommandInfo {
        name: "subagents",
        aliases: &["agents"],
        description: "List sub-agent status",
        usage: "/subagents",
    },
    CommandInfo {
        name: "minimax",
        aliases: &["dashboard", "api"],
        description: "Show MiniMax dashboard and docs links",
        usage: "/minimax",
    },
    // Session commands
    CommandInfo {
        name: "save",
        aliases: &[],
        description: "Save session to file",
        usage: "/save [path]",
    },
    CommandInfo {
        name: "load",
        aliases: &[],
        description: "Load session from file",
        usage: "/load [path]",
    },
    CommandInfo {
        name: "compact",
        aliases: &[],
        description: "Toggle auto-compaction",
        usage: "/compact",
    },
    CommandInfo {
        name: "export",
        aliases: &[],
        description: "Export conversation to markdown",
        usage: "/export [path]",
    },
    // Config commands
    CommandInfo {
        name: "config",
        aliases: &[],
        description: "Display current configuration",
        usage: "/config",
    },
    CommandInfo {
        name: "set",
        aliases: &[],
        description: "Modify a setting",
        usage: "/set <key> <value>",
    },
    CommandInfo {
        name: "yolo",
        aliases: &[],
        description: "Enable agent mode with shell execution",
        usage: "/yolo",
    },
    CommandInfo {
        name: "trust",
        aliases: &[],
        description: "Enable trust mode (access files outside workspace)",
        usage: "/trust",
    },
    CommandInfo {
        name: "logout",
        aliases: &[],
        description: "Clear API key and return to setup",
        usage: "/logout",
    },
    // Debug commands
    CommandInfo {
        name: "tokens",
        aliases: &[],
        description: "Show token usage for session",
        usage: "/tokens",
    },
    CommandInfo {
        name: "system",
        aliases: &[],
        description: "Show current system prompt",
        usage: "/system",
    },
    CommandInfo {
        name: "context",
        aliases: &[],
        description: "Show context window usage",
        usage: "/context",
    },
    CommandInfo {
        name: "undo",
        aliases: &[],
        description: "Remove last message pair",
        usage: "/undo",
    },
    CommandInfo {
        name: "retry",
        aliases: &[],
        description: "Retry the last request",
        usage: "/retry",
    },
    CommandInfo {
        name: "init",
        aliases: &[],
        description: "Generate MINIMAX.md for project",
        usage: "/init",
    },
    CommandInfo {
        name: "settings",
        aliases: &[],
        description: "Show persistent settings",
        usage: "/settings",
    },
    // Skills commands
    CommandInfo {
        name: "skills",
        aliases: &[],
        description: "List available skills",
        usage: "/skills",
    },
    CommandInfo {
        name: "skill",
        aliases: &[],
        description: "Activate a skill for next message",
        usage: "/skill <name>",
    },
    // Debug/cost command
    CommandInfo {
        name: "cost",
        aliases: &[],
        description: "Show session cost breakdown",
        usage: "/cost",
    },
];

/// Execute a slash command
pub fn execute(cmd: &str, app: &mut App) -> CommandResult {
    let parts: Vec<&str> = cmd.trim().splitn(2, ' ').collect();
    let command = parts[0].to_lowercase();
    let command = command.strip_prefix('/').unwrap_or(&command);
    let arg = parts.get(1).map(|s| s.trim());

    // Match command or alias
    match command {
        // Core commands
        "help" | "?" => core::help(app, arg),
        "clear" => core::clear(app),
        "exit" | "quit" | "q" => core::exit(),
        "mode" => core::mode(app, arg),
        "model" => core::model(app, arg),
        "queue" | "queued" => queue::queue(app, arg),
        "subagents" | "agents" => core::subagents(app),
        "minimax" | "dashboard" | "api" => core::minimax_links(),

        // Session commands
        "save" => session::save(app, arg),
        "load" => session::load(app, arg),
        "compact" => session::compact(app),
        "export" => session::export(app, arg),

        // Config commands
        "config" => config::show_config(app),
        "settings" => config::show_settings(app),
        "set" => config::set_config(app, arg),
        "yolo" => config::yolo(app),
        "trust" => config::trust(app),
        "logout" => config::logout(app),

        // Debug commands
        "tokens" => debug::tokens(app),
        "cost" => debug::cost(app),
        "system" => debug::system_prompt(app),
        "context" => debug::context(app),
        "undo" => debug::undo(app),
        "retry" => debug::retry(app),

        // Project commands
        "init" => init::init(app),

        // Skills commands
        "skills" => skills::list_skills(app),
        "skill" => skills::run_skill(app, arg),

        _ => CommandResult::error(format!(
            "Unknown command: /{command}. Type /help for available commands."
        )),
    }
}

/// Get command info by name or alias
pub fn get_command_info(name: &str) -> Option<&'static CommandInfo> {
    let name = name.strip_prefix('/').unwrap_or(name);
    COMMANDS
        .iter()
        .find(|cmd| cmd.name == name || cmd.aliases.contains(&name))
}

/// Get all commands matching a prefix (for autocomplete)
#[allow(dead_code)]
pub fn commands_matching(prefix: &str) -> Vec<&'static CommandInfo> {
    let prefix = prefix.strip_prefix('/').unwrap_or(prefix).to_lowercase();
    COMMANDS
        .iter()
        .filter(|cmd| {
            cmd.name.starts_with(&prefix) || cmd.aliases.iter().any(|a| a.starts_with(&prefix))
        })
        .collect()
}
