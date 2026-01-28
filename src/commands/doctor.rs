//! Doctor command for interactive diagnostics within the TUI
//!
//! Provides system diagnostics including:
//! - API key validity (format check)
//! - Config file existence and validity
//! - Settings file status
//! - Workspace accessibility
//! - MCP server status summary
//! - Session directory status
//! - Skills directory status
//! - Network connectivity (basic check)

use super::CommandResult;
use crate::config::{Config, has_api_key};
use crate::mcp::McpPool;
use crate::palette;
use crate::settings::Settings;
use crate::tui::app::App;
use colored::Colorize;
use std::net::ToSocketAddrs;
use std::path::PathBuf;

/// Diagnostic check result with status and optional hint
struct CheckResult {
    status: Status,
    message: String,
    hint: Option<String>,
}

#[derive(Clone, Copy)]
enum Status {
    Ok,
    Warning,
    Error,
}

impl CheckResult {
    fn ok(message: impl Into<String>) -> Self {
        Self {
            status: Status::Ok,
            message: message.into(),
            hint: None,
        }
    }

    fn warning(message: impl Into<String>) -> Self {
        Self {
            status: Status::Warning,
            message: message.into(),
            hint: None,
        }
    }

    fn warning_with_hint(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            status: Status::Warning,
            message: message.into(),
            hint: Some(hint.into()),
        }
    }

    fn error_with_hint(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            status: Status::Error,
            message: message.into(),
            hint: Some(hint.into()),
        }
    }
}

/// Run diagnostics and return formatted results
pub fn doctor(app: &mut App) -> CommandResult {
    let checks = run_all_checks(app);
    let output = format_diagnostic_output(&checks);
    CommandResult::message(output)
}

/// Run all diagnostic checks
fn run_all_checks(app: &App) -> Vec<(&'static str, Vec<CheckResult>)> {
    vec![
        ("API Key", check_api_key(app)),
        ("Configuration", check_config()),
        ("Settings", check_settings()),
        ("Workspace", check_workspace(app)),
        ("MCP Servers", check_mcp_servers(app)),
        ("Sessions", check_sessions()),
        ("Skills", check_skills()),
        ("Network", check_network()),
    ]
}

/// Check API key status
fn check_api_key(_app: &App) -> Vec<CheckResult> {
    let mut results = Vec::new();

    // Check environment variable first
    if std::env::var("MINIMAX_API_KEY").is_ok() {
        results.push(CheckResult::ok(
            "MINIMAX_API_KEY environment variable is set",
        ));
    } else {
        // Check config file
        match Config::load(None, None) {
            Ok(config) => {
                if has_api_key(&config) {
                    results.push(CheckResult::ok("API key found in config file"));
                } else {
                    results.push(CheckResult::error_with_hint(
                        "No API key configured",
                        "Run /setup to configure your API key interactively",
                    ));
                }
            }
            Err(_) => {
                results.push(CheckResult::error_with_hint(
                    "Could not load config to check API key",
                    "Run /setup to configure your API key",
                ));
            }
        }
    }

    // Check API key format if available
    if let Ok(key) = std::env::var("MINIMAX_API_KEY") {
        if is_valid_api_key_format(&key) {
            results.push(CheckResult::ok("API key format is valid"));
        } else {
            results.push(CheckResult::warning_with_hint(
                "API key format looks unusual (expected: sk-api-...)",
                "Verify your key at https://platform.minimax.io",
            ));
        }
    } else if let Ok(config) = Config::load(None, None)
        && let Ok(key) = config.minimax_api_key()
    {
        if is_valid_api_key_format(&key) {
            results.push(CheckResult::ok("API key format is valid"));
        } else {
            results.push(CheckResult::warning_with_hint(
                "API key format looks unusual (expected: sk-api-...)",
                "Verify your key at https://platform.minimax.io",
            ));
        }
    }

    results
}

/// Check if API key has valid format (starts with expected prefix)
fn is_valid_api_key_format(key: &str) -> bool {
    key.starts_with("sk-") || key.starts_with("sk-api-") || key.len() > 20
}

/// Check configuration file status
fn check_config() -> Vec<CheckResult> {
    let mut results = Vec::new();

    let config_path = get_config_path();

    if !config_path.exists() {
        results.push(CheckResult::warning_with_hint(
            "No config file found",
            "Run /setup to create a config file with your API key",
        ));
        return results;
    }

    results.push(CheckResult::ok(format!(
        "Config file exists at {}",
        config_path.display()
    )));

    // Try to read and parse config
    match std::fs::read_to_string(&config_path) {
        Ok(content) => {
            if content.trim().is_empty() {
                results.push(CheckResult::warning("Config file is empty"));
            } else {
                match Config::load(Some(config_path.clone()), None) {
                    Ok(_) => {
                        results.push(CheckResult::ok("Config file is valid TOML"));
                    }
                    Err(e) => {
                        results.push(CheckResult::error_with_hint(
                            format!("Config file has errors: {e}"),
                            "Run /reload after fixing the config file",
                        ));
                    }
                }
            }
        }
        Err(e) => {
            results.push(CheckResult::error_with_hint(
                format!("Cannot read config file: {e}"),
                "Check file permissions",
            ));
        }
    }

    results
}

/// Check settings file status
fn check_settings() -> Vec<CheckResult> {
    let mut results = Vec::new();

    match Settings::path() {
        Ok(path) => {
            if path.exists() {
                match Settings::load() {
                    Ok(_) => {
                        results.push(CheckResult::ok("Settings loaded successfully"));
                    }
                    Err(e) => {
                        results.push(CheckResult::warning(format!("Settings file issue: {e}")));
                    }
                }
            } else {
                results.push(CheckResult::ok(
                    "Settings file does not exist (using defaults)",
                ));
            }
        }
        Err(e) => {
            results.push(CheckResult::warning(format!(
                "Could not determine settings path: {e}"
            )));
        }
    }

    results
}

/// Check workspace accessibility
fn check_workspace(app: &App) -> Vec<CheckResult> {
    let mut results = Vec::new();

    let workspace = &app.workspace;

    if workspace.exists() {
        results.push(CheckResult::ok(format!(
            "Workspace exists: {}",
            workspace.display()
        )));

        // Check if readable
        match std::fs::read_dir(workspace) {
            Ok(entries) => {
                let count = entries.count();
                results.push(CheckResult::ok(format!(
                    "Workspace is readable ({} items)",
                    count
                )));
            }
            Err(e) => {
                results.push(CheckResult::error_with_hint(
                    format!("Cannot read workspace: {e}"),
                    "Check directory permissions",
                ));
            }
        }

        // Check if writable
        let test_file = workspace.join(".minimax_write_test");
        match std::fs::write(&test_file, "test") {
            Ok(_) => {
                let _ = std::fs::remove_file(&test_file);
                results.push(CheckResult::ok("Workspace is writable"));
            }
            Err(e) => {
                results.push(CheckResult::error_with_hint(
                    format!("Cannot write to workspace: {e}"),
                    "Check directory permissions",
                ));
            }
        }
    } else {
        results.push(CheckResult::error_with_hint(
            format!("Workspace does not exist: {}", workspace.display()),
            "Check the --workspace option or current directory",
        ));
    }

    results
}

/// Check MCP server status
fn check_mcp_servers(app: &App) -> Vec<CheckResult> {
    let mut results = Vec::new();

    let config_path = get_mcp_config_path(app);

    if !config_path.exists() {
        results.push(CheckResult::ok(
            "No MCP config (MCP servers not configured)",
        ));
        return results;
    }

    results.push(CheckResult::ok(format!(
        "MCP config found: {}",
        config_path.display()
    )));

    match McpPool::from_config_path(&config_path) {
        Ok(pool) => {
            let config = pool.config();

            if config.servers.is_empty() {
                results.push(CheckResult::warning("MCP config exists but has no servers"));
            } else {
                let total = config.servers.len();
                let connected = pool.connected_servers().len();
                let disabled = config.servers.values().filter(|s| s.disabled).count();

                if connected == total {
                    results.push(CheckResult::ok(format!(
                        "All {} MCP server(s) connected",
                        total
                    )));
                } else if connected > 0 {
                    results.push(CheckResult::warning(format!(
                        "{} of {} MCP server(s) connected ({} disabled)",
                        connected, total, disabled
                    )));
                } else if disabled == total {
                    results.push(CheckResult::warning(format!(
                        "All {} MCP server(s) are disabled",
                        total
                    )));
                } else {
                    results.push(CheckResult::warning_with_hint(
                        format!("No MCP servers connected ({} configured)", total),
                        "Check MCP server configuration in mcp.json",
                    ));
                }

                // List server names
                for (name, server_config) in &config.servers {
                    let status = if server_config.disabled {
                        "disabled"
                    } else if pool.connected_servers().contains(&name.as_str()) {
                        "connected"
                    } else {
                        "not connected"
                    };
                    results.push(CheckResult::ok(format!("  - {} ({})", name, status)));
                }
            }
        }
        Err(e) => {
            results.push(CheckResult::error_with_hint(
                format!("Failed to load MCP config: {e}"),
                "Check mcp.json syntax",
            ));
        }
    }

    results
}

/// Check session directory status
fn check_sessions() -> Vec<CheckResult> {
    let mut results = Vec::new();

    let session_dir = get_session_dir();

    if session_dir.exists() {
        match std::fs::read_dir(&session_dir) {
            Ok(entries) => {
                let count = entries
                    .filter(|e| {
                        e.as_ref()
                            .map(|e| {
                                e.path()
                                    .extension()
                                    .map(|ext| ext == "json")
                                    .unwrap_or(false)
                            })
                            .unwrap_or(false)
                    })
                    .count();

                if count > 0 {
                    results.push(CheckResult::ok(format!(
                        "Session directory exists ({} saved sessions)",
                        count
                    )));
                } else {
                    results.push(CheckResult::ok(
                        "Session directory exists (no saved sessions)",
                    ));
                }
            }
            Err(e) => {
                results.push(CheckResult::warning(format!(
                    "Cannot read session directory: {e}"
                )));
            }
        }
    } else {
        results.push(CheckResult::warning_with_hint(
            "Session directory does not exist",
            "Sessions will be created when you save",
        ));
    }

    results
}

/// Check skills directory status
fn check_skills() -> Vec<CheckResult> {
    let mut results = Vec::new();

    let skills_dir = get_skills_dir();

    if skills_dir.exists() {
        match std::fs::read_dir(&skills_dir) {
            Ok(entries) => {
                let count = entries.count();
                results.push(CheckResult::ok(format!(
                    "Skills directory exists ({} items)",
                    count
                )));
            }
            Err(e) => {
                results.push(CheckResult::warning(format!(
                    "Cannot read skills directory: {e}"
                )));
            }
        }
    } else {
        results.push(CheckResult::ok(
            "Skills directory not configured (optional)",
        ));
    }

    results
}

/// Check network connectivity
fn check_network() -> Vec<CheckResult> {
    let mut results = Vec::new();

    // Basic DNS resolution check
    let api_hosts = ["api.minimax.io", "api.minimaxi.com"];
    let mut any_resolved = false;

    for host in &api_hosts {
        match ToSocketAddrs::to_socket_addrs(&format!("{}:443", host)) {
            Ok(mut addrs) => {
                if addrs.next().is_some() {
                    any_resolved = true;
                    results.push(CheckResult::ok(format!("{} resolves successfully", host)));
                }
            }
            Err(_) => {
                // Don't report error for individual hosts, just try the next
            }
        }
    }

    if !any_resolved {
        results.push(CheckResult::warning_with_hint(
            "Could not resolve MiniMax API hostnames",
            "Check your network connection and DNS settings",
        ));
    }

    results
}

/// Format all diagnostic results into a display string
fn format_diagnostic_output(groups: &[(&'static str, Vec<CheckResult>)]) -> String {
    use std::fmt::Write;

    let (green_r, green_g, green_b) = palette::MINIMAX_GREEN_RGB;
    let (orange_r, orange_g, orange_b) = palette::MINIMAX_ORANGE_RGB;
    let (red_r, red_g, red_b) = palette::MINIMAX_RED_RGB;
    let (blue_r, blue_g, blue_b) = palette::MINIMAX_BLUE_RGB;
    let (muted_r, muted_g, muted_b) = palette::MINIMAX_SILVER_RGB;

    let mut output = String::new();

    // Header
    let _ = writeln!(
        output,
        "{}",
        "╔═══════════════════════════════════════════════════════════════╗"
            .truecolor(blue_r, blue_g, blue_b)
    );
    let _ = writeln!(
        output,
        "{}",
        "║                    MiniMax CLI Doctor                         ║"
            .truecolor(blue_r, blue_g, blue_b)
    );
    let _ = writeln!(
        output,
        "{}",
        "╚═══════════════════════════════════════════════════════════════╝"
            .truecolor(blue_r, blue_g, blue_b)
    );
    let _ = writeln!(output);

    // Summary stats
    let mut total_ok = 0;
    let mut total_warning = 0;
    let mut total_error = 0;

    for (_, checks) in groups {
        for check in checks {
            match check.status {
                Status::Ok => total_ok += 1,
                Status::Warning => total_warning += 1,
                Status::Error => total_error += 1,
            }
        }
    }

    let _ = writeln!(
        output,
        "{} {} OK  {} {} Warning  {} {} Error",
        "●".truecolor(green_r, green_g, green_b),
        total_ok,
        "●".truecolor(orange_r, orange_g, orange_b),
        total_warning,
        "●".truecolor(red_r, red_g, red_b),
        total_error
    );
    let _ = writeln!(output);

    // Each group
    for (group_name, checks) in groups {
        if checks.is_empty() {
            continue;
        }

        let _ = writeln!(output, "{}", group_name.bold());
        let _ = writeln!(
            output,
            "{}",
            "─"
                .repeat(group_name.len())
                .truecolor(muted_r, muted_g, muted_b)
        );

        for check in checks {
            let icon = match check.status {
                Status::Ok => "✓".truecolor(green_r, green_g, green_b),
                Status::Warning => "⚠".truecolor(orange_r, orange_g, orange_b),
                Status::Error => "✗".truecolor(red_r, red_g, red_b),
            };

            let _ = writeln!(output, "  {} {}", icon, check.message);

            if let Some(hint) = &check.hint {
                let _ = writeln!(
                    output,
                    "    {} {}",
                    "→".truecolor(blue_r, blue_g, blue_b),
                    hint.truecolor(muted_r, muted_g, muted_b)
                );
            }
        }

        let _ = writeln!(output);
    }

    // Footer with helpful hints
    if total_error > 0 {
        let _ = writeln!(
            output,
            "{}",
            "Fix suggestions:".truecolor(blue_r, blue_g, blue_b).bold()
        );
        let _ = writeln!(
            output,
            "  {} Run {} to configure API key",
            "→".truecolor(blue_r, blue_g, blue_b),
            "/setup".truecolor(blue_r, blue_g, blue_b)
        );
        let _ = writeln!(
            output,
            "  {} Run {} after fixing config issues",
            "→".truecolor(blue_r, blue_g, blue_b),
            "/reload".truecolor(blue_r, blue_g, blue_b)
        );
        let _ = writeln!(output);
    }

    let _ = writeln!(
        output,
        "{}",
        "Run /doctor again to refresh diagnostics".truecolor(muted_r, muted_g, muted_b)
    );

    output
}

/// Get config file path
fn get_config_path() -> PathBuf {
    if let Ok(path) = std::env::var("MINIMAX_CONFIG_PATH") {
        return PathBuf::from(path);
    }
    dirs::home_dir()
        .map(|home| home.join(".minimax").join("config.toml"))
        .unwrap_or_else(|| PathBuf::from(".minimax/config.toml"))
}

/// Get session directory path
fn get_session_dir() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(".minimax").join("sessions"))
        .unwrap_or_else(|| PathBuf::from(".minimax/sessions"))
}

/// Get skills directory path
fn get_skills_dir() -> PathBuf {
    dirs::home_dir()
        .map(|home| home.join(".minimax").join("skills"))
        .unwrap_or_else(|| PathBuf::from(".minimax/skills"))
}

/// Get MCP config path from app or default locations
fn get_mcp_config_path(app: &App) -> PathBuf {
    // Try workspace/.minimax/mcp.json first
    let workspace_path = app.workspace.join(".minimax").join("mcp.json");
    if workspace_path.exists() {
        return workspace_path;
    }

    // Try ~/.minimax/mcp.json
    if let Some(home) = dirs::home_dir() {
        let home_path = home.join(".minimax").join("mcp.json");
        if home_path.exists() {
            return home_path;
        }
    }

    // Fall back to current directory
    PathBuf::from("mcp.json")
}
