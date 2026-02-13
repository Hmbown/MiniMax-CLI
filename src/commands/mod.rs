//! Slash command registry and dispatch system
//!
//! This module provides a modular command system inspired by Codex-rs.
//! Commands are organized by category and dispatched through a central registry.

mod config;
mod core;
mod debug;
mod doctor;
mod init;
mod jobs;
mod mcp;
mod pins;
mod queue;
mod reload;
pub mod rlm;
mod session;
mod setup;
mod skills;
mod snippets;
mod task;
mod usage;

use crate::tui::app::{App, AppAction, AppMode};

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
        name: "reset",
        aliases: &[],
        description: "Hard reset of session - clears history, todos, plan, and all state",
        usage: "/reset",
    },
    CommandInfo {
        name: "exit",
        aliases: &["quit", "q"],
        description: "Exit the application",
        usage: "/exit",
    },
    CommandInfo {
        name: "model",
        aliases: &[],
        description: "Switch model (interactive picker or direct name)",
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
        description: "List/manage background sub-agent status",
        usage: "/subagents [list|show <agent_id>|wait <agent_id> [timeout_ms]|cancel <agent_id>|clean [max_age_ms]]",
    },
    CommandInfo {
        name: "jobs",
        aliases: &[],
        description: "List/manage background shell jobs",
        usage: "/jobs [list|wait <task_id> [timeout_ms]|kill <task_id>|clean [max_age_ms]]",
    },
    CommandInfo {
        name: "task",
        aliases: &["tasks"],
        description: "Manage background tasks",
        usage: "/task [add <prompt>|list|show <id>|cancel <id>]",
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
        description: "Load session from file (or RLM context in RLM mode)",
        usage: "/load [path]",
    },
    CommandInfo {
        name: "save-session",
        aliases: &["save_session"],
        description: "Save RLM session to file",
        usage: "/save-session [path]",
    },
    CommandInfo {
        name: "sessions",
        aliases: &["resume"],
        description: "List and switch between saved sessions",
        usage: "/sessions",
    },
    CommandInfo {
        name: "history",
        aliases: &[],
        description: "Browse and select from command history",
        usage: "/history",
    },
    CommandInfo {
        name: "status",
        aliases: &[],
        description: "Show RLM context status",
        usage: "/status",
    },
    CommandInfo {
        name: "repl",
        aliases: &[],
        description: "Toggle RLM REPL mode",
        usage: "/repl",
    },
    CommandInfo {
        name: "compact",
        aliases: &[],
        description: "Toggle auto-compaction or trigger manual compaction",
        usage: "/compact [now]",
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
        description: "Toggle YOLO mode (shell + trust + auto-approve)",
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
    CommandInfo {
        name: "setup",
        aliases: &[],
        description: "Run interactive setup wizard",
        usage: "/setup",
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
        description: "Generate AGENTS.md for project",
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
    CommandInfo {
        name: "debug",
        aliases: &[],
        description: "Show debug information",
        usage: "/debug",
    },
    CommandInfo {
        name: "reload",
        aliases: &[],
        description: "Reload configuration from disk",
        usage: "/reload",
    },
    CommandInfo {
        name: "usage",
        aliases: &[],
        description: "Show API usage and quota information",
        usage: "/usage",
    },
    CommandInfo {
        name: "mcp",
        aliases: &[],
        description: "Show MCP server status",
        usage: "/mcp",
    },
    CommandInfo {
        name: "doctor",
        aliases: &["diagnose", "health"],
        description: "Run interactive diagnostics and health checks",
        usage: "/doctor",
    },
    CommandInfo {
        name: "tutorial",
        aliases: &[],
        description: "Restart the getting started tutorial",
        usage: "/tutorial",
    },
    // Pin commands
    CommandInfo {
        name: "pin",
        aliases: &[],
        description: "Pin a message from history",
        usage: "/pin [n]",
    },
    CommandInfo {
        name: "pins",
        aliases: &[],
        description: "List all pinned messages",
        usage: "/pins",
    },
    CommandInfo {
        name: "unpin",
        aliases: &[],
        description: "Unpin a message or clear all",
        usage: "/unpin <n>|all",
    },
    // Snippet commands
    CommandInfo {
        name: "snippet",
        aliases: &[],
        description: "Insert a snippet template into input",
        usage: "/snippet <name>",
    },
    CommandInfo {
        name: "snippets",
        aliases: &[],
        description: "List available snippets",
        usage: "/snippets",
    },
    CommandInfo {
        name: "search",
        aliases: &["find"],
        description: "Search through conversation history",
        usage: "/search [query]",
    },
    CommandInfo {
        name: "copy",
        aliases: &[],
        description: "Copy last assistant message (or Nth message) to clipboard",
        usage: "/copy [n]",
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
        "reset" => session::reset(app),
        "exit" | "quit" | "q" => core::exit(),
        "model" => core::model(app, arg),
        "queue" | "queued" => queue::queue(app, arg),
        "subagents" | "agents" => core::subagents(app, arg),
        "jobs" => jobs::jobs(app, arg),
        "task" | "tasks" => task::task(app, arg),
        "minimax" | "dashboard" | "api" => core::minimax_links(),

        // Session commands
        "save" => session::save(app, arg),
        "load" => {
            if app.mode == AppMode::Rlm {
                rlm::load(app, arg)
            } else {
                session::load(app, arg)
            }
        }
        "save-session" | "save_session" => rlm::save_session(app, arg),
        "sessions" | "resume" => CommandResult::action(AppAction::OpenSessionPicker),
        "history" => CommandResult::action(AppAction::OpenHistoryPicker),
        "status" => rlm::status(app),
        "repl" => rlm::repl(app),
        "compact" => session::compact(app, arg),
        "export" => session::export(app, arg),

        // Config commands
        "config" => config::show_config(app),
        "settings" => config::show_settings(app),
        "set" => config::set_config(app, arg),
        "yolo" => config::yolo(app),
        "trust" => config::trust(app),
        "logout" => config::logout(app),
        "reload" => reload::reload(app),
        "setup" => setup::setup(app),

        // Debug commands
        "tokens" => debug::tokens(app),
        "cost" => debug::cost(app),
        "debug" => debug::debug_info(app),
        "system" => debug::system_prompt(app),
        "context" => debug::context(app),
        "undo" => debug::undo(app),
        "retry" => debug::retry(app),
        "usage" => usage::usage(app),

        // Project commands
        "init" => init::init(app),

        // Skills commands
        "skills" => skills::list_skills(app),
        "skill" => skills::run_skill(app, arg),

        // MCP command
        "mcp" => mcp::mcp_status(app),

        // Doctor command
        "doctor" | "diagnose" | "health" => doctor::doctor(app),

        // Tutorial command
        "tutorial" => {
            app.tutorial.start();
            CommandResult::message("Starting tutorial...")
        }

        // Pin commands
        "pin" => pins::pin(app, arg),
        "pins" => pins::list_pins(app),
        "unpin" => pins::unpin(app, arg),

        // Snippet commands
        "snippet" => snippets::insert_snippet(app, arg),
        "snippets" => snippets::list_snippets(app),

        // Search command
        "search" | "find" => {
            let query = arg.unwrap_or("");
            app.search_query = query.to_string();
            CommandResult::action(AppAction::OpenSearch(query.to_string()))
        }

        // Copy command
        "copy" => core::copy(app, arg),

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
