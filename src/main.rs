//! CLI entry point for the `MiniMax` client.

use std::io;
use std::path::PathBuf;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use dotenvy::dotenv;

mod client;
mod command_safety;
mod commands;
mod compaction;
mod config;
mod core;
mod duo;
mod hooks;
mod llm_client;
mod logging;
mod mcp;
mod models;
mod modules;
mod pricing;
mod project_context;
mod project_doc;
mod prompts;
mod rlm;
mod sandbox;
mod session_manager;
mod settings;
mod skills;
mod smoke;
mod tools;
mod tui;
mod ui;
mod utils;

use crate::config::Config;

#[derive(Parser, Debug)]
#[command(
    name = "minimax",
    author,
    version,
    about = "MiniMax CLI - Chat with MiniMax M2.1",
    long_about = "Unofficial CLI for MiniMax M2.1 API.\n\nJust run 'minimax' to start chatting.\n\nNot affiliated with MiniMax Inc."
)]
struct Cli {
    /// Subcommand to run
    #[command(subcommand)]
    command: Option<Commands>,

    /// Send a one-shot prompt (non-interactive)
    #[arg(short, long)]
    prompt: Option<String>,

    /// YOLO mode: enable agent tools + shell execution
    #[arg(long)]
    yolo: bool,

    /// Maximum number of concurrent sub-agents (1-5)
    #[arg(long)]
    max_subagents: Option<usize>,

    /// Path to config file
    #[arg(long)]
    config: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,

    /// Config profile name
    #[arg(long)]
    profile: Option<String>,

    /// Workspace directory for file operations
    #[arg(short, long)]
    workspace: Option<PathBuf>,

    /// Resume a previous session by ID or prefix
    #[arg(short, long)]
    resume: Option<String>,

    /// Continue the most recent session
    #[arg(short = 'c', long = "continue")]
    continue_session: bool,
}

#[derive(Subcommand, Debug, Clone)]
#[allow(clippy::large_enum_variant)]
enum Commands {
    /// Run system diagnostics and check configuration
    Doctor,
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
    /// List saved sessions
    Sessions {
        /// Maximum number of sessions to display
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Search sessions by title
        #[arg(short, long)]
        search: Option<String>,
    },
    /// Create default AGENTS.md in current directory
    Init,
    /// Smoke test MiniMax media generation (writes real files)
    SmokeMedia {
        /// Confirm you want to spend credits and write files
        #[arg(long)]
        confirm: bool,
        /// Output directory for generated files (default: --workspace / current directory)
        #[arg(long)]
        output_dir: Option<PathBuf>,
        /// Prompt for image generation
        #[arg(
            long,
            default_value = "A friendly robot playing a golden trumpet, colorful illustration"
        )]
        image_prompt: String,
        /// Image model name
        #[arg(long, default_value = "image-01")]
        image_model: String,
        /// Prompt for music generation
        #[arg(
            long,
            default_value = "Cheerful upbeat trumpet solo, jazzy, high quality"
        )]
        music_prompt: String,
        /// Music model name
        #[arg(long, default_value = "music-1.5")]
        music_model: String,
        /// Text for text-to-speech generation
        #[arg(long, default_value = "Hello from MiniMax CLI smoke test.")]
        tts_text: String,
        /// TTS model name
        #[arg(long, default_value = "speech-02-hd")]
        tts_model: String,
        /// Prompt for video generation
        #[arg(
            long,
            default_value = "A cinematic slow pan across a cozy coffee shop interior, warm lighting, rain outside the window"
        )]
        video_prompt: String,
        /// Video model name
        #[arg(long, default_value = "MiniMax-Hailuo-02")]
        video_model: String,
        /// Video duration in seconds
        #[arg(long, default_value_t = 6)]
        video_duration: u32,
        /// Video resolution (MiniMax supports 512P, 768P, 1080P; 720p maps to 768P)
        #[arg(long, default_value = "768P")]
        video_resolution: String,
        /// Submit video generation without waiting/downloading
        #[arg(long)]
        video_async: bool,
        /// Skip image generation
        #[arg(long)]
        skip_image: bool,
        /// Skip music generation
        #[arg(long)]
        skip_music: bool,
        /// Skip TTS generation
        #[arg(long)]
        skip_tts: bool,
        /// Skip video generation
        #[arg(long)]
        skip_video: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    let cli = Cli::parse();
    logging::set_verbose(cli.verbose);

    // Handle subcommands first
    if let Some(command) = cli.command.clone() {
        return match command {
            Commands::Doctor => {
                run_doctor();
                Ok(())
            }
            Commands::Completions { shell } => {
                generate_completions(shell);
                Ok(())
            }
            Commands::Sessions { limit, search } => list_sessions(limit, search),
            Commands::Init => init_project(),
            Commands::SmokeMedia {
                confirm,
                output_dir,
                image_prompt,
                image_model,
                music_prompt,
                music_model,
                tts_text,
                tts_model,
                video_prompt,
                video_model,
                video_duration,
                video_resolution,
                video_async,
                skip_image,
                skip_music,
                skip_tts,
                skip_video,
            } => {
                if !confirm {
                    anyhow::bail!(
                        "Refusing to run: this command makes paid network calls and writes files. Re-run with --confirm."
                    );
                }

                let profile = cli
                    .profile
                    .clone()
                    .or_else(|| std::env::var("MINIMAX_PROFILE").ok());
                let config = Config::load(cli.config.clone(), profile.as_deref())?;
                let workspace = cli.workspace.clone().unwrap_or_else(|| {
                    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                });

                let output_dir = output_dir.unwrap_or_else(|| workspace.clone());

                smoke::run_smoke_media(
                    &config,
                    smoke::SmokeMediaOptions {
                        output_dir,
                        image_prompt,
                        image_model,
                        music_prompt,
                        music_model,
                        tts_text,
                        tts_model,
                        video_prompt,
                        video_model,
                        video_duration,
                        video_resolution,
                        video_async,
                        skip_image,
                        skip_music,
                        skip_tts,
                        skip_video,
                    },
                )
                .await?;

                Ok(())
            }
        };
    }

    let profile = cli
        .profile
        .or_else(|| std::env::var("MINIMAX_PROFILE").ok());
    let config = Config::load(cli.config, profile.as_deref())?;

    let workspace = cli
        .workspace
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let model = config
        .default_text_model
        .clone()
        .unwrap_or_else(|| "MiniMax-M2.1".to_string());
    let max_subagents = cli
        .max_subagents
        .map_or_else(|| config.max_subagents(), |value| value.clamp(1, 5));

    // One-shot prompt mode
    if let Some(prompt) = cli.prompt {
        return run_one_shot(&config, &model, &prompt).await;
    }

    // Handle session resume
    let resume_session_id = if cli.continue_session {
        // Get most recent session
        match session_manager::SessionManager::default_location() {
            Ok(manager) => manager.get_latest_session().ok().flatten().map(|m| m.id),
            Err(_) => None,
        }
    } else {
        cli.resume.clone()
    };

    // Default: Interactive TUI
    // --yolo starts in YOLO mode (shell + trust + auto-approve)
    tui::run_tui(
        &config,
        tui::TuiOptions {
            model,
            workspace,
            allow_shell: cli.yolo || config.allow_shell(),
            skills_dir: config.skills_dir(),
            memory_path: config.memory_path(),
            notes_path: config.notes_path(),
            mcp_config_path: config.mcp_config_path(),
            use_memory: false,
            start_in_agent_mode: cli.yolo,
            yolo: cli.yolo, // YOLO mode auto-approves all tool executions
            resume_session_id,
            max_subagents,
        },
    )
    .await
}

/// Generate shell completions for the given shell
fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut io::stdout());
}

/// Run system diagnostics
fn run_doctor() {
    use colored::Colorize;

    println!("{}", "MiniMax CLI Doctor".bold().cyan());
    println!("{}", "==================".cyan());
    println!();

    // Version info
    println!("{}", "Version Information:".bold());
    println!("  minimax-cli: {}", env!("CARGO_PKG_VERSION"));
    println!("  rust: {}", rustc_version());
    println!();

    // Check configuration
    println!("{}", "Configuration:".bold());
    let config_dir =
        dirs::home_dir().map_or_else(|| PathBuf::from(".minimax"), |h| h.join(".minimax"));

    let config_file = config_dir.join("config.toml");
    if config_file.exists() {
        println!(
            "  {} config.toml found at {}",
            "✓".green(),
            config_file.display()
        );
    } else {
        println!(
            "  {} config.toml not found (will use defaults)",
            "!".yellow()
        );
    }

    // Check API keys
    println!();
    println!("{}", "API Keys:".bold());
    if std::env::var("MINIMAX_API_KEY").is_ok() {
        println!("  {} MINIMAX_API_KEY is set", "✓".green());
    } else {
        let key_in_config = Config::load(None, None)
            .ok()
            .and_then(|c| c.minimax_api_key().ok())
            .is_some();
        if key_in_config {
            println!("  {} MiniMax API key found in config", "✓".green());
        } else {
            println!("  {} MiniMax API key not configured", "✗".red());
            println!("    Run 'minimax' to configure interactively, or set MINIMAX_API_KEY");
        }
    }

    // Check MCP configuration
    println!();
    println!("{}", "MCP Servers:".bold());
    let mcp_config = config_dir.join("mcp.json");
    if mcp_config.exists() {
        println!("  {} mcp.json found", "✓".green());
        if let Ok(content) = std::fs::read_to_string(&mcp_config)
            && let Ok(config) = serde_json::from_str::<crate::mcp::McpConfig>(&content)
        {
            if config.servers.is_empty() {
                println!("  {} 0 server(s) configured", "·".dimmed());
            } else {
                println!(
                    "  {} {} server(s) configured",
                    "·".dimmed(),
                    config.servers.len()
                );
                for name in config.servers.keys() {
                    println!("    - {name}");
                }
            }
        }
    } else {
        println!("  {} mcp.json not found (no MCP servers)", "·".dimmed());
    }

    // Check skills directory
    println!();
    println!("{}", "Skills:".bold());
    let skills_dir = config_dir.join("skills");
    if skills_dir.exists() {
        let skill_count = std::fs::read_dir(skills_dir)
            .map(|entries| entries.filter_map(std::result::Result::ok).count())
            .unwrap_or(0);
        println!(
            "  {} skills directory found ({} items)",
            "✓".green(),
            skill_count
        );
    } else {
        println!("  {} skills directory not found", "·".dimmed());
    }

    // Platform-specific checks
    println!();
    println!("{}", "Platform:".bold());
    println!("  OS: {}", std::env::consts::OS);
    println!("  Arch: {}", std::env::consts::ARCH);

    #[cfg(target_os = "macos")]
    {
        if std::path::Path::new("/usr/bin/sandbox-exec").exists() {
            println!("  {} macOS sandbox available", "✓".green());
        } else {
            println!("  {} macOS sandbox not available", "!".yellow());
        }
    }

    println!();
    println!("{}", "All checks complete!".green().bold());
}

fn rustc_version() -> String {
    // Try to get rustc version, fall back to "unknown"
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_string(), |s| s.trim().to_string())
}

/// List saved sessions
fn list_sessions(limit: usize, search: Option<String>) -> Result<()> {
    use colored::Colorize;
    use session_manager::{SessionManager, format_session_line};

    let manager = SessionManager::default_location()?;

    let sessions = if let Some(query) = search {
        manager.search_sessions(&query)?
    } else {
        manager.list_sessions()?
    };

    if sessions.is_empty() {
        println!("{}", "No sessions found.".yellow());
        println!("Start a new session with: {}", "minimax".cyan());
        return Ok(());
    }

    println!("{}", "Saved Sessions".bold().cyan());
    println!("{}", "==============".cyan());
    println!();

    for (i, session) in sessions.iter().take(limit).enumerate() {
        let line = format_session_line(session);
        if i == 0 {
            println!("  {} {}", "*".green(), line);
        } else {
            println!("    {line}");
        }
    }

    let total = sessions.len();
    if total > limit {
        println!();
        println!(
            "  {} more session(s). Use --limit to show more.",
            total - limit
        );
    }

    println!();
    println!(
        "Resume with: {} {}",
        "minimax --resume".cyan(),
        "<session-id>".dimmed()
    );
    println!("Continue latest: {}", "minimax --continue".cyan());

    Ok(())
}

/// Initialize a new project with AGENTS.md
fn init_project() -> Result<()> {
    use colored::Colorize;
    use project_context::create_default_agents_md;

    let workspace = std::env::current_dir()?;
    let agents_path = workspace.join("AGENTS.md");

    if agents_path.exists() {
        println!(
            "{} AGENTS.md already exists at {}",
            "!".yellow(),
            agents_path.display()
        );
        return Ok(());
    }

    match create_default_agents_md(&workspace) {
        Ok(path) => {
            println!("{} Created {}", "✓".green(), path.display());
            println!();
            println!("Edit this file to customize how the AI agent works with your project.");
            println!("The instructions will be loaded automatically when you run minimax.");
        }
        Err(e) => {
            println!("{} Failed to create AGENTS.md: {}", "✗".red(), e);
        }
    }

    Ok(())
}

async fn run_one_shot(config: &Config, model: &str, prompt: &str) -> Result<()> {
    use crate::client::AnthropicClient;
    use crate::models::{ContentBlock, Message, MessageRequest};

    let client = AnthropicClient::new(config)?;

    let request = MessageRequest {
        model: model.to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: prompt.to_string(),
                cache_control: None,
            }],
        }],
        max_tokens: 4096,
        system: None,
        tools: None,
        tool_choice: None,
        metadata: None,
        thinking: None,
        stream: Some(false),
        temperature: None,
        top_p: None,
    };

    let response = client.create_message(request).await?;

    for block in response.content {
        if let ContentBlock::Text { text, .. } = block {
            println!("{text}");
        }
    }

    Ok(())
}
