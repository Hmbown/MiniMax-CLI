//! CLI entry point for the `MiniMax` client.

use std::io;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use dotenvy::dotenv;

mod client;
mod command_safety;
mod commands;
mod compaction;
mod config;
mod core;
mod duo;
mod execpolicy;
mod features;
mod hooks;
mod llm_client;
mod logging;
mod mcp;
mod models;
mod modules;
mod palette;
mod pricing;
mod project_context;
mod project_doc;
mod prompts;
mod responses_api_proxy;
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
use crate::llm_client::LlmClient;

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

    #[command(flatten)]
    feature_toggles: FeatureToggles,

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
    /// Execpolicy tooling
    Execpolicy(ExecpolicyCommand),
    /// Inspect feature flags
    Features(FeaturesCli),
    /// Run a command inside the sandbox
    Sandbox(SandboxArgs),
    /// Internal: run the responses API proxy.
    #[command(hide = true)]
    ResponsesApiProxy(responses_api_proxy::Args),
}

#[derive(Args, Debug, Default, Clone)]
struct FeatureToggles {
    /// Enable a feature (repeatable). Equivalent to `features.<name>=true`.
    #[arg(long = "enable", value_name = "FEATURE", action = clap::ArgAction::Append, global = true)]
    enable: Vec<String>,

    /// Disable a feature (repeatable). Equivalent to `features.<name>=false`.
    #[arg(long = "disable", value_name = "FEATURE", action = clap::ArgAction::Append, global = true)]
    disable: Vec<String>,
}

impl FeatureToggles {
    fn apply(&self, config: &mut Config) -> Result<()> {
        for feature in &self.enable {
            config.set_feature(feature, true)?;
        }
        for feature in &self.disable {
            config.set_feature(feature, false)?;
        }
        Ok(())
    }
}

#[derive(Args, Debug, Clone)]
struct ExecpolicyCommand {
    #[command(subcommand)]
    command: ExecpolicySubcommand,
}

#[derive(Subcommand, Debug, Clone)]
enum ExecpolicySubcommand {
    /// Check execpolicy files against a command
    Check(execpolicy::ExecPolicyCheckCommand),
}

#[derive(Args, Debug, Clone)]
struct FeaturesCli {
    #[command(subcommand)]
    command: FeaturesSubcommand,
}

#[derive(Subcommand, Debug, Clone)]
enum FeaturesSubcommand {
    /// List known feature flags and their state
    List,
}

#[derive(Args, Debug, Clone)]
struct SandboxArgs {
    #[command(subcommand)]
    command: SandboxCommand,
}

#[derive(Subcommand, Debug, Clone)]
enum SandboxCommand {
    /// Run a command with sandboxing
    Run {
        /// Sandbox policy (danger-full-access, read-only, external-sandbox, workspace-write)
        #[arg(long, default_value = "workspace-write")]
        policy: String,
        /// Allow outbound network access
        #[arg(long)]
        network: bool,
        /// Additional writable roots (repeatable)
        #[arg(long, value_name = "PATH")]
        writable_root: Vec<PathBuf>,
        /// Exclude TMPDIR from writable paths
        #[arg(long)]
        exclude_tmpdir: bool,
        /// Exclude /tmp from writable paths
        #[arg(long)]
        exclude_slash_tmp: bool,
        /// Command working directory
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Timeout in milliseconds
        #[arg(long, default_value_t = 60_000)]
        timeout_ms: u64,
        /// Command and arguments to run
        #[arg(required = true, trailing_var_arg = true)]
        command: Vec<String>,
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
                run_doctor().await;
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

                let config = load_config_from_cli(&cli)?;
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
            Commands::Execpolicy(command) => run_execpolicy_command(command),
            Commands::Features(command) => {
                let config = load_config_from_cli(&cli)?;
                run_features_command(&config, command)
            }
            Commands::Sandbox(args) => run_sandbox_command(args),
            Commands::ResponsesApiProxy(args) => responses_api_proxy::run_main(args),
        };
    }

    let config = load_config_from_cli(&cli)?;

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

fn load_config_from_cli(cli: &Cli) -> Result<Config> {
    let profile = cli
        .profile
        .clone()
        .or_else(|| std::env::var("MINIMAX_PROFILE").ok());
    let mut config = Config::load(cli.config.clone(), profile.as_deref())?;
    cli.feature_toggles.apply(&mut config)?;
    Ok(config)
}

/// Generate shell completions for the given shell
fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut io::stdout());
}

fn run_execpolicy_command(command: ExecpolicyCommand) -> Result<()> {
    match command.command {
        ExecpolicySubcommand::Check(args) => args.run(),
    }
}

fn run_features_command(config: &Config, command: FeaturesCli) -> Result<()> {
    match command.command {
        FeaturesSubcommand::List => run_features_list(config),
    }
}

fn stage_str(stage: features::Stage) -> &'static str {
    match stage {
        features::Stage::Experimental => "experimental",
        features::Stage::Beta => "beta",
        features::Stage::Stable => "stable",
        features::Stage::Deprecated => "deprecated",
        features::Stage::Removed => "removed",
    }
}

fn run_features_list(config: &Config) -> Result<()> {
    let features = config.features();
    println!("feature\tstage\tenabled");
    for spec in features::FEATURES {
        let enabled = features.enabled(spec.id);
        println!("{}\t{}\t{enabled}", spec.key, stage_str(spec.stage));
    }
    Ok(())
}

fn run_sandbox_command(args: SandboxArgs) -> Result<()> {
    use crate::sandbox::{CommandSpec, SandboxManager};
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::Duration;
    use wait_timeout::ChildExt;

    let SandboxCommand::Run {
        policy,
        network,
        writable_root,
        exclude_tmpdir,
        exclude_slash_tmp,
        cwd,
        timeout_ms,
        command,
    } = args.command;

    let policy = parse_sandbox_policy(
        &policy,
        network,
        writable_root,
        exclude_tmpdir,
        exclude_slash_tmp,
    )?;
    let cwd = cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let timeout = Duration::from_millis(timeout_ms.clamp(1000, 600_000));

    let (program, args) = command
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("Command is required"))?;
    let spec =
        CommandSpec::program(program, args.to_vec(), cwd.clone(), timeout).with_policy(policy);
    let manager = SandboxManager::new();
    let exec_env = manager.prepare(&spec);

    let mut cmd = Command::new(exec_env.program());
    cmd.args(exec_env.args())
        .current_dir(&exec_env.cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in &exec_env.env {
        cmd.env(key, value);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to run command: {e}"))?;
    let stdout_handle = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("stdout unavailable"))?;
    let stderr_handle = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("stderr unavailable"))?;

    let timeout = exec_env.timeout;
    let stdout_thread = std::thread::spawn(move || {
        let mut reader = stdout_handle;
        let mut buf = Vec::new();
        let _ = reader.read_to_end(&mut buf);
        buf
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut reader = stderr_handle;
        let mut buf = Vec::new();
        let _ = reader.read_to_end(&mut buf);
        buf
    });

    if let Some(status) = child.wait_timeout(timeout)? {
        let stdout = stdout_thread.join().unwrap_or_default();
        let stderr = stderr_thread.join().unwrap_or_default();
        let stderr_str = String::from_utf8_lossy(&stderr);
        let exit_code = status.code().unwrap_or(-1);
        let sandbox_type = exec_env.sandbox_type;
        let sandbox_denied = SandboxManager::was_denied(sandbox_type, exit_code, &stderr_str);

        if !stdout.is_empty() {
            print!("{}", String::from_utf8_lossy(&stdout));
        }
        if !stderr.is_empty() {
            eprint!("{}", stderr_str);
        }
        if sandbox_denied {
            eprintln!(
                "{}",
                SandboxManager::denial_message(sandbox_type, &stderr_str)
            );
        }

        if !status.success() {
            anyhow::bail!("Command failed with exit code {exit_code}");
        }
    } else {
        let _ = child.kill();
        let _ = child.wait();
        anyhow::bail!("Command timed out after {}ms", timeout.as_millis());
    }
    Ok(())
}

fn parse_sandbox_policy(
    policy: &str,
    network: bool,
    writable_root: Vec<PathBuf>,
    exclude_tmpdir: bool,
    exclude_slash_tmp: bool,
) -> Result<crate::sandbox::SandboxPolicy> {
    use crate::sandbox::SandboxPolicy;

    match policy {
        "danger-full-access" => Ok(SandboxPolicy::DangerFullAccess),
        "read-only" => Ok(SandboxPolicy::ReadOnly),
        "external-sandbox" => Ok(SandboxPolicy::ExternalSandbox {
            network_access: network,
        }),
        "workspace-write" => Ok(SandboxPolicy::WorkspaceWrite {
            writable_roots: writable_root,
            network_access: network,
            exclude_tmpdir,
            exclude_slash_tmp,
        }),
        other => anyhow::bail!("Unknown sandbox policy: {other}"),
    }
}

/// Run system diagnostics
async fn run_doctor() {
    use colored::Colorize;

    let (blue_r, blue_g, blue_b) = palette::MINIMAX_BLUE_RGB;
    let (green_r, green_g, green_b) = palette::MINIMAX_GREEN_RGB;
    let (orange_r, orange_g, orange_b) = palette::MINIMAX_ORANGE_RGB;
    let (red_r, red_g, red_b) = palette::MINIMAX_RED_RGB;
    let (muted_r, muted_g, muted_b) = palette::MINIMAX_SILVER_RGB;

    println!(
        "{}",
        "MiniMax CLI Doctor"
            .truecolor(blue_r, blue_g, blue_b)
            .bold()
    );
    println!("{}", "==================".truecolor(blue_r, blue_g, blue_b));
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
            "✓".truecolor(green_r, green_g, green_b),
            config_file.display()
        );
    } else {
        println!(
            "  {} config.toml not found (will use defaults)",
            "!".truecolor(orange_r, orange_g, orange_b)
        );
    }

    // Check API keys
    println!();
    println!("{}", "API Keys:".bold());
    let has_api_key = if std::env::var("MINIMAX_API_KEY").is_ok() {
        println!(
            "  {} MINIMAX_API_KEY is set",
            "✓".truecolor(green_r, green_g, green_b)
        );
        true
    } else {
        let key_in_config = Config::load(None, None)
            .ok()
            .and_then(|c| c.minimax_api_key().ok())
            .is_some();
        if key_in_config {
            println!(
                "  {} MiniMax API key found in config",
                "✓".truecolor(green_r, green_g, green_b)
            );
            true
        } else {
            println!(
                "  {} MiniMax API key not configured",
                "✗".truecolor(red_r, red_g, red_b)
            );
            println!("    Run 'minimax' to configure interactively, or set MINIMAX_API_KEY");
            false
        }
    };

    // API connectivity test
    println!();
    println!("{}", "API Connectivity:".bold());
    if has_api_key {
        print!(
            "  {} Testing connection to MiniMax API...",
            "·".truecolor(muted_r, muted_g, muted_b)
        );
        // Flush to show progress immediately
        use std::io::Write;
        std::io::stdout().flush().ok();

        match test_api_connectivity().await {
            Ok(model) => {
                println!(
                    "\r  {} API connection successful (model: {})",
                    "✓".truecolor(green_r, green_g, green_b),
                    model
                );
            }
            Err(e) => {
                let error_msg = e.to_string();
                println!(
                    "\r  {} API connection failed",
                    "✗".truecolor(red_r, red_g, red_b)
                );
                // Provide helpful diagnostics based on error type
                if error_msg.contains("401") || error_msg.contains("Unauthorized") {
                    println!("    {}", "✗ Invalid API key".truecolor(red_r, red_g, red_b));
                    println!("    → Check your MINIMAX_API_KEY or config.toml");
                    println!("    → Verify your API key is active at https://platform.minimax.io");
                    println!("    → Keys look like: sk-api-...");
                } else if error_msg.contains("403") || error_msg.contains("Forbidden") {
                    println!(
                        "    {}",
                        "✗ API key lacks permissions".truecolor(red_r, red_g, red_b)
                    );
                    println!("    → Verify your API key is active at https://platform.minimax.io");
                    println!("    → You may need to generate a new API key");
                } else if error_msg.contains("timeout") || error_msg.contains("Timeout") {
                    println!(
                        "    {}",
                        "✗ Connection timed out".truecolor(red_r, red_g, red_b)
                    );
                    println!("    → Check your network connection");
                    println!("    → Try again - this may be a temporary issue");
                    println!(
                        "    → China users: try setting MINIMAX_BASE_URL=https://api.minimaxi.com"
                    );
                } else if error_msg.contains("dns") || error_msg.contains("resolve") {
                    println!(
                        "    {}",
                        "✗ DNS resolution failed".truecolor(red_r, red_g, red_b)
                    );
                    println!("    → Check your network connection");
                    println!("    → Verify you can reach api.minimax.io");
                } else if error_msg.contains("certificate") || error_msg.contains("SSL") {
                    println!(
                        "    {}",
                        "✗ SSL/certificate error".truecolor(red_r, red_g, red_b)
                    );
                    println!("    → Check your system clock and date");
                    println!("    → Your SSL certificates may be outdated");
                } else if error_msg.contains("connection refused") {
                    println!(
                        "    {}",
                        "✗ Connection refused".truecolor(red_r, red_g, red_b)
                    );
                    println!("    → The API server may be down");
                    println!("    → Check https://status.minimax.io for outages");
                    println!("    → Try again later");
                } else if error_msg.contains("429") {
                    println!("    {}", "✗ Rate limited".truecolor(red_r, red_g, red_b));
                    println!("    → You've made too many requests");
                    println!("    → Wait a moment and try again");
                } else {
                    // Show truncated error with helpful prefix
                    println!("    {}", "✗ Error:".truecolor(red_r, red_g, red_b));
                    // Truncate very long error messages
                    let truncated = if error_msg.len() > 200 {
                        &error_msg[..200]
                    } else {
                        &error_msg
                    };
                    println!("    {}", truncated);
                    println!();
                    println!(
                        "    {} Need more help?",
                        "→".truecolor(blue_r, blue_g, blue_b).bold()
                    );
                    println!("    → Run with -v for verbose logging");
                    println!("    → Check https://github.com/Hmbown/MiniMax-CLI/issues");
                }

                // Quick fix section
                println!();
                println!("    {}", "Quick fixes:".bold());
                println!("    → export MINIMAX_API_KEY='your-key-here'");
                println!(
                    "    → Run {} again to verify",
                    "minimax doctor".truecolor(blue_r, blue_g, blue_b)
                );
            }
        }
    } else {
        println!(
            "  {} Skipped (no API key configured)",
            "·".truecolor(muted_r, muted_g, muted_b)
        );
        // Help users who don't have an API key
        println!();
        println!("    {}", "To get started:".bold());
        println!("    1. Get an API key from https://platform.minimax.io");
        println!("    2. Either:");
        println!("       → Set environment variable: export MINIMAX_API_KEY='your-key'");
        println!("       → Or create config: ~/.minimax/config.toml");
        println!(
            "    3. Run {} to verify",
            "minimax doctor".truecolor(blue_r, blue_g, blue_b)
        );
    }

    // Check MCP configuration
    println!();
    println!("{}", "MCP Servers:".bold());
    let mcp_config = config_dir.join("mcp.json");
    if mcp_config.exists() {
        println!(
            "  {} mcp.json found",
            "✓".truecolor(green_r, green_g, green_b)
        );
        if let Ok(content) = std::fs::read_to_string(&mcp_config)
            && let Ok(config) = serde_json::from_str::<crate::mcp::McpConfig>(&content)
        {
            if config.servers.is_empty() {
                println!(
                    "  {} 0 server(s) configured",
                    "·".truecolor(muted_r, muted_g, muted_b)
                );
            } else {
                println!(
                    "  {} {} server(s) configured",
                    "·".truecolor(muted_r, muted_g, muted_b),
                    config.servers.len()
                );
                for name in config.servers.keys() {
                    println!("    - {name}");
                }
            }
        }
    } else {
        println!(
            "  {} mcp.json not found (no MCP servers)",
            "·".truecolor(muted_r, muted_g, muted_b)
        );
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
            "✓".truecolor(green_r, green_g, green_b),
            skill_count
        );
    } else {
        println!(
            "  {} skills directory not found",
            "·".truecolor(muted_r, muted_g, muted_b)
        );
    }

    // Platform-specific checks
    println!();
    println!("{}", "Platform:".bold());
    println!("  OS: {}", std::env::consts::OS);
    println!("  Arch: {}", std::env::consts::ARCH);

    #[cfg(target_os = "macos")]
    {
        if std::path::Path::new("/usr/bin/sandbox-exec").exists() {
            println!(
                "  {} macOS sandbox available",
                "✓".truecolor(green_r, green_g, green_b)
            );
        } else {
            println!(
                "  {} macOS sandbox not available",
                "!".truecolor(orange_r, orange_g, orange_b)
            );
        }
    }

    println!();
    println!(
        "{}",
        "All checks complete!"
            .truecolor(green_r, green_g, green_b)
            .bold()
    );
}

/// Test API connectivity by making a minimal request
async fn test_api_connectivity() -> Result<String> {
    use crate::client::AnthropicClient;
    use crate::models::{ContentBlock, Message, MessageRequest};

    let config = Config::load(None, None)?;
    let client = AnthropicClient::new(&config)?;
    let model = client.model().to_string();

    // Minimal request: single word prompt, 1 max token
    let request = MessageRequest {
        model: model.clone(),
        messages: vec![Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: "hi".to_string(),
                cache_control: None,
            }],
        }],
        max_tokens: 1,
        system: None,
        tools: None,
        tool_choice: None,
        metadata: None,
        thinking: None,
        stream: Some(false),
        temperature: None,
        top_p: None,
    };

    // Use tokio timeout to catch hanging requests
    let timeout_duration = std::time::Duration::from_secs(15);
    match tokio::time::timeout(timeout_duration, client.create_message(request)).await {
        Ok(Ok(_response)) => Ok(model),
        Ok(Err(e)) => Err(e),
        Err(_) => anyhow::bail!("Request timeout after 15 seconds"),
    }
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

    let (blue_r, blue_g, blue_b) = palette::MINIMAX_BLUE_RGB;
    let (green_r, green_g, green_b) = palette::MINIMAX_GREEN_RGB;
    let (orange_r, orange_g, orange_b) = palette::MINIMAX_ORANGE_RGB;
    let (muted_r, muted_g, muted_b) = palette::MINIMAX_SILVER_RGB;

    let manager = SessionManager::default_location()?;

    let sessions = if let Some(query) = search {
        manager.search_sessions(&query)?
    } else {
        manager.list_sessions()?
    };

    if sessions.is_empty() {
        println!(
            "{}",
            "No sessions found.".truecolor(orange_r, orange_g, orange_b)
        );
        println!(
            "Start a new session with: {}",
            "minimax".truecolor(blue_r, blue_g, blue_b)
        );
        return Ok(());
    }

    println!(
        "{}",
        "Saved Sessions".truecolor(blue_r, blue_g, blue_b).bold()
    );
    println!("{}", "==============".truecolor(blue_r, blue_g, blue_b));
    println!();

    for (i, session) in sessions.iter().take(limit).enumerate() {
        let line = format_session_line(session);
        if i == 0 {
            println!("  {} {}", "*".truecolor(green_r, green_g, green_b), line);
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
        "minimax --resume".truecolor(blue_r, blue_g, blue_b),
        "<session-id>".truecolor(muted_r, muted_g, muted_b)
    );
    println!(
        "Continue latest: {}",
        "minimax --continue".truecolor(blue_r, blue_g, blue_b)
    );

    Ok(())
}

/// Initialize a new project with AGENTS.md
fn init_project() -> Result<()> {
    use colored::Colorize;
    use project_context::create_default_agents_md;

    let (green_r, green_g, green_b) = palette::MINIMAX_GREEN_RGB;
    let (orange_r, orange_g, orange_b) = palette::MINIMAX_ORANGE_RGB;
    let (red_r, red_g, red_b) = palette::MINIMAX_RED_RGB;

    let workspace = std::env::current_dir()?;
    let agents_path = workspace.join("AGENTS.md");

    if agents_path.exists() {
        println!(
            "{} AGENTS.md already exists at {}",
            "!".truecolor(orange_r, orange_g, orange_b),
            agents_path.display()
        );
        return Ok(());
    }

    match create_default_agents_md(&workspace) {
        Ok(path) => {
            println!(
                "{} Created {}",
                "✓".truecolor(green_r, green_g, green_b),
                path.display()
            );
            println!();
            println!("Edit this file to customize how the AI agent works with your project.");
            println!("The instructions will be loaded automatically when you run minimax.");
        }
        Err(e) => {
            println!(
                "{} Failed to create AGENTS.md: {}",
                "✗".truecolor(red_r, red_g, red_b),
                e
            );
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
