//! Configuration loading and defaults for minimax-cli.

use std::collections::HashMap;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::hooks::HooksConfig;

// === Types ===

/// Raw retry configuration loaded from config files.
#[derive(Debug, Clone, Deserialize)]
pub struct RetryConfig {
    pub enabled: Option<bool>,
    pub max_retries: Option<u32>,
    pub initial_delay: Option<f64>,
    pub max_delay: Option<f64>,
    pub exponential_base: Option<f64>,
}

/// Resolved retry policy with defaults applied.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub enabled: bool,
    pub max_retries: u32,
    pub initial_delay: f64,
    pub max_delay: f64,
    pub exponential_base: f64,
}

impl RetryPolicy {
    /// Compute the backoff delay for a retry attempt.
    #[must_use]
    pub fn delay_for_attempt(&self, attempt: u32) -> std::time::Duration {
        let exponent = i32::try_from(attempt).unwrap_or(i32::MAX);
        let delay = self.initial_delay * self.exponential_base.powi(exponent);
        let delay = delay.min(self.max_delay);
        std::time::Duration::from_secs_f64(delay)
    }
}

/// Resolved CLI configuration, including defaults and environment overrides.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    pub api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub base_url: Option<String>,
    pub anthropic_base_url: Option<String>,
    pub default_text_model: Option<String>,
    pub default_image_model: Option<String>,
    pub default_video_model: Option<String>,
    pub default_audio_model: Option<String>,
    pub default_music_model: Option<String>,
    pub output_dir: Option<String>,
    pub tools_file: Option<String>,
    pub skills_dir: Option<String>,
    pub mcp_config_path: Option<String>,
    pub notes_path: Option<String>,
    pub memory_path: Option<String>,
    pub allow_shell: Option<bool>,
    pub max_subagents: Option<usize>,
    pub retry: Option<RetryConfig>,

    /// Lifecycle hooks configuration
    #[serde(default)]
    pub hooks: Option<HooksConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ConfigFile {
    #[serde(flatten)]
    base: Config,
    profiles: Option<HashMap<String, Config>>,
}

// === Config Loading ===

impl Config {
    /// Load configuration from disk and merge with environment overrides.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use crate::config::Config;
    /// let config = Config::load(None, None)?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn load(path: Option<PathBuf>, profile: Option<&str>) -> Result<Self> {
        let path = path.or_else(default_config_path);
        let mut config = if let Some(path) = path.as_ref() {
            if path.exists() {
                let contents = fs::read_to_string(path)
                    .with_context(|| format!("Failed to read config file: {}", path.display()))?;
                let parsed: ConfigFile = toml::from_str(&contents)
                    .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
                apply_profile(parsed, profile)
            } else {
                Config::default()
            }
        } else {
            Config::default()
        };

        apply_env_overrides(&mut config);
        Ok(config)
    }

    /// Return the `MiniMax` base URL (normalized).
    #[must_use]
    pub fn minimax_base_url(&self) -> String {
        let base = self
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.minimax.io".to_string());
        normalize_base_url(&base)
    }

    /// Return the Anthropic base URL (normalized).
    #[must_use]
    pub fn anthropic_base_url(&self) -> String {
        if let Some(base) = self.anthropic_base_url.clone() {
            return base;
        }
        let root = normalize_base_url(
            &self
                .base_url
                .clone()
                .unwrap_or_else(|| "https://api.minimax.io".to_string()),
        );
        format!("{}/anthropic", root.trim_end_matches('/'))
    }

    /// Read the `MiniMax` API key from config/environment.
    pub fn minimax_api_key(&self) -> Result<String> {
        self.api_key
            .clone()
            .context(
                "Failed to load MiniMax API key: MINIMAX_API_KEY missing. Set it in config.toml or environment.",
            )
    }

    pub fn anthropic_api_key(&self) -> Result<String> {
        self.anthropic_api_key
            .clone()
            .or_else(|| self.api_key.clone())
            .context(
                "Failed to load Anthropic API key: ANTHROPIC_API_KEY missing. Set it in config.toml or environment.",
            )
    }

    #[allow(dead_code)]
    pub fn output_dir(&self) -> PathBuf {
        self.output_dir
            .clone()
            .map_or_else(|| PathBuf::from("./outputs"), PathBuf::from)
    }

    /// Resolve the skills directory path.
    #[must_use]
    pub fn skills_dir(&self) -> PathBuf {
        self.skills_dir
            .clone()
            .map(PathBuf::from)
            .or_else(default_skills_dir)
            .unwrap_or_else(|| PathBuf::from("./skills"))
    }

    /// Resolve the MCP config path.
    #[must_use]
    pub fn mcp_config_path(&self) -> PathBuf {
        self.mcp_config_path
            .clone()
            .map(PathBuf::from)
            .or_else(default_mcp_config_path)
            .unwrap_or_else(|| PathBuf::from("./mcp.json"))
    }

    /// Resolve the notes file path.
    #[must_use]
    pub fn notes_path(&self) -> PathBuf {
        self.notes_path
            .clone()
            .map(PathBuf::from)
            .or_else(default_notes_path)
            .unwrap_or_else(|| PathBuf::from("./notes.txt"))
    }

    /// Resolve the memory file path.
    #[must_use]
    pub fn memory_path(&self) -> PathBuf {
        self.memory_path
            .clone()
            .map(PathBuf::from)
            .or_else(default_memory_path)
            .unwrap_or_else(|| PathBuf::from("./memory.md"))
    }

    /// Return whether shell execution is allowed.
    #[must_use]
    pub fn allow_shell(&self) -> bool {
        self.allow_shell.unwrap_or(false)
    }

    /// Return the maximum number of concurrent sub-agents.
    #[must_use]
    pub fn max_subagents(&self) -> usize {
        self.max_subagents.unwrap_or(5).clamp(1, 5)
    }

    /// Get hooks configuration, returning default if not configured.
    pub fn hooks_config(&self) -> HooksConfig {
        self.hooks.clone().unwrap_or_default()
    }

    /// Resolve the effective retry policy with defaults applied.
    #[must_use]
    pub fn retry_policy(&self) -> RetryPolicy {
        let defaults = RetryPolicy {
            enabled: true,
            max_retries: 3,
            initial_delay: 1.0,
            max_delay: 60.0,
            exponential_base: 2.0,
        };

        let Some(cfg) = &self.retry else {
            return defaults;
        };

        RetryPolicy {
            enabled: cfg.enabled.unwrap_or(defaults.enabled),
            max_retries: cfg.max_retries.unwrap_or(defaults.max_retries),
            initial_delay: cfg.initial_delay.unwrap_or(defaults.initial_delay),
            max_delay: cfg.max_delay.unwrap_or(defaults.max_delay),
            exponential_base: cfg.exponential_base.unwrap_or(defaults.exponential_base),
        }
    }
}

// === Defaults ===

fn default_config_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("MINIMAX_CONFIG_PATH") {
        if !path.trim().is_empty() {
            return Some(PathBuf::from(path));
        }
    }
    dirs::home_dir().map(|home| home.join(".minimax").join("config.toml"))
}

fn default_skills_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".minimax").join("skills"))
}

fn default_mcp_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".minimax").join("mcp.json"))
}

fn default_notes_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".minimax").join("notes.txt"))
}

fn default_memory_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".minimax").join("memory.md"))
}

// === Environment Overrides ===

fn apply_env_overrides(config: &mut Config) {
    if let Ok(value) = std::env::var("MINIMAX_API_KEY") {
        config.api_key = Some(value);
    }
    if let Ok(value) = std::env::var("ANTHROPIC_API_KEY") {
        config.anthropic_api_key = Some(value);
    }
    if let Ok(value) = std::env::var("MINIMAX_BASE_URL") {
        config.base_url = Some(value);
    }
    if let Ok(value) = std::env::var("ANTHROPIC_BASE_URL") {
        config.anthropic_base_url = Some(value);
    }
    if let Ok(value) = std::env::var("MINIMAX_OUTPUT_DIR") {
        config.output_dir = Some(value);
    }
    if let Ok(value) = std::env::var("MINIMAX_SKILLS_DIR") {
        config.skills_dir = Some(value);
    }
    if let Ok(value) = std::env::var("MINIMAX_MCP_CONFIG") {
        config.mcp_config_path = Some(value);
    }
    if let Ok(value) = std::env::var("MINIMAX_NOTES_PATH") {
        config.notes_path = Some(value);
    }
    if let Ok(value) = std::env::var("MINIMAX_MEMORY_PATH") {
        config.memory_path = Some(value);
    }
    if let Ok(value) = std::env::var("MINIMAX_ALLOW_SHELL") {
        config.allow_shell = Some(value == "1" || value.eq_ignore_ascii_case("true"));
    }
    if let Ok(value) = std::env::var("MINIMAX_MAX_SUBAGENTS")
        && let Ok(parsed) = value.parse::<usize>()
    {
        config.max_subagents = Some(parsed.clamp(1, 5));
    }
}

fn normalize_base_url(base: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    let minimax_domains = ["api.minimax.io", "api.minimaxi.com"];
    if minimax_domains
        .iter()
        .any(|domain| trimmed.contains(domain))
    {
        return trimmed
            .trim_end_matches("/anthropic")
            .trim_end_matches("/v1")
            .to_string();
    }
    trimmed.to_string()
}

fn apply_profile(config: ConfigFile, profile: Option<&str>) -> Config {
    if let Some(profile) = profile
        && let Some(profiles) = config.profiles
        && let Some(override_cfg) = profiles.get(profile)
    {
        return merge_config(config.base, override_cfg.clone());
    }
    config.base
}

fn merge_config(base: Config, override_cfg: Config) -> Config {
    Config {
        api_key: override_cfg.api_key.or(base.api_key),
        anthropic_api_key: override_cfg.anthropic_api_key.or(base.anthropic_api_key),
        base_url: override_cfg.base_url.or(base.base_url),
        anthropic_base_url: override_cfg.anthropic_base_url.or(base.anthropic_base_url),
        default_text_model: override_cfg.default_text_model.or(base.default_text_model),
        default_image_model: override_cfg
            .default_image_model
            .or(base.default_image_model),
        default_video_model: override_cfg
            .default_video_model
            .or(base.default_video_model),
        default_audio_model: override_cfg
            .default_audio_model
            .or(base.default_audio_model),
        default_music_model: override_cfg
            .default_music_model
            .or(base.default_music_model),
        output_dir: override_cfg.output_dir.or(base.output_dir),
        tools_file: override_cfg.tools_file.or(base.tools_file),
        skills_dir: override_cfg.skills_dir.or(base.skills_dir),
        mcp_config_path: override_cfg.mcp_config_path.or(base.mcp_config_path),
        notes_path: override_cfg.notes_path.or(base.notes_path),
        memory_path: override_cfg.memory_path.or(base.memory_path),
        allow_shell: override_cfg.allow_shell.or(base.allow_shell),
        max_subagents: override_cfg.max_subagents.or(base.max_subagents),
        retry: override_cfg.retry.or(base.retry),
        hooks: override_cfg.hooks.or(base.hooks),
    }
}

pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }
    Ok(())
}

/// Save an API key to the config file. Creates the file if it doesn't exist.
pub fn save_api_key(api_key: &str) -> Result<PathBuf> {
    let config_path = default_config_path()
        .context("Failed to resolve config path: home directory not found.")?;

    ensure_parent_dir(&config_path)?;

    let content = if config_path.exists() {
        // Read existing config and update the api_key line
        let existing = fs::read_to_string(&config_path)?;
        if existing.contains("api_key") {
            // Replace existing api_key line
            let mut result = String::new();
            for line in existing.lines() {
                if line.trim_start().starts_with("api_key")
                    && !line.trim_start().starts_with("anthropic_api_key")
                {
                    let _ = writeln!(result, "api_key = \"{api_key}\"");
                } else {
                    result.push_str(line);
                    result.push('\n');
                }
            }
            result
        } else {
            // Prepend api_key to existing config
            format!("api_key = \"{api_key}\"\n{existing}")
        }
    } else {
        // Create new minimal config
        format!(
            r#"# MiniMax CLI Configuration
# Get your API key from https://platform.minimax.chat

api_key = "{api_key}"

# Base URL (default: https://api.minimax.io)
# base_url = "https://api.minimax.io"

# Default model
default_text_model = "MiniMax-M2.1"
"#
        )
    };

    fs::write(&config_path, content)
        .with_context(|| format!("Failed to write config to {}", config_path.display()))?;

    Ok(config_path)
}

/// Check if an API key is configured (either in config or environment)
pub fn has_api_key(config: &Config) -> bool {
    config.api_key.is_some() || config.anthropic_api_key.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::ffi::OsString;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct EnvGuard {
        home: Option<OsString>,
        userprofile: Option<OsString>,
        minimax_config_path: Option<OsString>,
    }

    impl EnvGuard {
        fn new(home: &Path) -> Self {
            let home_str = OsString::from(home.as_os_str());
            let config_path = home.join(".minimax").join("config.toml");
            let config_str = OsString::from(config_path.as_os_str());
            let home_prev = env::var_os("HOME");
            let userprofile_prev = env::var_os("USERPROFILE");
            let minimax_config_prev = env::var_os("MINIMAX_CONFIG_PATH");
            // Safety: test-only environment mutation guarded by a global mutex.
            unsafe {
                env::set_var("HOME", &home_str);
                env::set_var("USERPROFILE", &home_str);
                env::set_var("MINIMAX_CONFIG_PATH", &config_str);
            }
            Self {
                home: home_prev,
                userprofile: userprofile_prev,
                minimax_config_path: minimax_config_prev,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = self.home.take() {
                // Safety: test-only environment mutation guarded by a global mutex.
                unsafe {
                    env::set_var("HOME", value);
                }
            } else {
                // Safety: test-only environment mutation guarded by a global mutex.
                unsafe {
                    env::remove_var("HOME");
                }
            }
            if let Some(value) = self.userprofile.take() {
                // Safety: test-only environment mutation guarded by a global mutex.
                unsafe {
                    env::set_var("USERPROFILE", value);
                }
            } else {
                // Safety: test-only environment mutation guarded by a global mutex.
                unsafe {
                    env::remove_var("USERPROFILE");
                }
            }
            if let Some(value) = self.minimax_config_path.take() {
                // Safety: test-only environment mutation guarded by a global mutex.
                unsafe {
                    env::set_var("MINIMAX_CONFIG_PATH", value);
                }
            } else {
                // Safety: test-only environment mutation guarded by a global mutex.
                unsafe {
                    env::remove_var("MINIMAX_CONFIG_PATH");
                }
            }
        }
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn save_api_key_writes_config() -> Result<()> {
        let _lock = env_lock().lock().unwrap();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_root =
            env::temp_dir().join(format!("minimax-cli-test-{}-{}", std::process::id(), nanos));
        fs::create_dir_all(&temp_root)?;
        let _guard = EnvGuard::new(&temp_root);

        let path = save_api_key("test-key")?;
        let expected = temp_root.join(".minimax").join("config.toml");
        assert_eq!(path, expected);

        let contents = fs::read_to_string(&path)?;
        assert!(contents.contains("api_key = \"test-key\""));
        Ok(())
    }
}
