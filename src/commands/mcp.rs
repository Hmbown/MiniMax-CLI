//! MCP command for displaying MCP server status

use crate::mcp::McpPool;
use crate::tui::app::App;

use super::CommandResult;
use std::path::PathBuf;

/// Show MCP server status information
pub fn mcp_status(app: &mut App) -> CommandResult {
    let mut output = String::new();

    output.push_str("MCP Servers:\n");

    // Get the MCP config path from the app or use default
    let config_path = get_mcp_config_path(app);

    // Try to load the MCP pool/config
    match McpPool::from_config_path(&config_path) {
        Ok(pool) => {
            let config = pool.config();

            if config.servers.is_empty() {
                output.push_str("  (no servers configured)\n");
                return CommandResult::message(output);
            }

            let connected = pool.connected_servers();
            let connected_set: std::collections::HashSet<_> = connected.iter().copied().collect();

            for (name, server_config) in &config.servers {
                let is_connected = connected_set.contains(name.as_str());
                let is_disabled = server_config.disabled;

                // Status indicator and message
                if is_disabled {
                    output.push_str(&format!("  ○ {} (disabled)\n", name));
                } else if is_connected {
                    output.push_str(&format!("  ✓ {} (connected)\n", name));
                } else {
                    output.push_str(&format!("  ✗ {} (not connected)\n", name));
                }

                // Tools list (only show if connected and not disabled)
                if is_disabled {
                    output.push_str("    Tools: (disabled)\n");
                } else if is_connected {
                    // Find the connection to get tools
                    if let Some(conn) = pool
                        .connected_servers()
                        .iter()
                        .find(|&&s| s == name.as_str())
                    {
                        // We can't directly access tools from just the server name,
                        // but we can get all tools and filter by prefix
                        let all_tools = pool.all_tools();
                        let server_tools: Vec<_> = all_tools
                            .iter()
                            .filter(|(full_name, _)| {
                                full_name.starts_with(&format!("mcp_{}_", name))
                            })
                            .map(|(_, tool)| tool.name.as_str())
                            .collect();

                        if server_tools.is_empty() {
                            output.push_str("    Tools: (none discovered)\n");
                        } else {
                            output.push_str(&format!("    Tools: {}\n", server_tools.join(", ")));
                        }
                    } else {
                        output.push_str("    Tools: (unavailable)\n");
                    }
                } else {
                    output.push_str("    Tools: (unavailable)\n");
                }

                // Configuration details
                output.push_str(&format!(
                    "    Command: {} {}\n",
                    server_config.command,
                    server_config.args.join(" ")
                ));

                if !server_config.env.is_empty() {
                    let env_keys: Vec<_> = server_config.env.keys().cloned().collect();
                    output.push_str(&format!("    Env: {}\n", env_keys.join(", ")));
                }

                // Show timeout overrides if any
                if let Some(timeout) = server_config.connect_timeout {
                    output.push_str(&format!("    Connect timeout: {}s\n", timeout));
                }
                if let Some(timeout) = server_config.execute_timeout {
                    output.push_str(&format!("    Execute timeout: {}s\n", timeout));
                }
                if let Some(timeout) = server_config.read_timeout {
                    output.push_str(&format!("    Read timeout: {}s\n", timeout));
                }
            }

            // Show global timeouts
            output.push_str(&format!(
                "\nGlobal Timeouts: connect={}s, execute={}s, read={}s\n",
                config.timeouts.connect_timeout,
                config.timeouts.execute_timeout,
                config.timeouts.read_timeout
            ));
        }
        Err(e) => {
            output.push_str(&format!("  (failed to load MCP config: {})\n", e));
        }
    }

    CommandResult::message(output)
}

/// Get the MCP config path from app or use default locations
fn get_mcp_config_path(app: &App) -> PathBuf {
    // Try to get from app's mcp_config_path field
    // Since we can't directly access it, use default paths

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
