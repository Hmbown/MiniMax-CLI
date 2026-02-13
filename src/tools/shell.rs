//! Advanced shell execution with background process support and sandboxing.
//!
//! Provides:
//! - Synchronous command execution with timeout
//! - Background process execution
//! - Process output retrieval
//! - Process termination
//! - Sandbox support (macOS Seatbelt)
//! - Streaming output (future)

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;
use wait_timeout::ChildExt;

use crate::sandbox::{
    CommandSpec,
    ExecEnv,
    SandboxManager,
    SandboxPolicy as ExecutionSandboxPolicy, // Rename to avoid conflict with spec::SandboxPolicy
    SandboxType,
};

/// Maximum output size before truncation (30KB like Claude Code)
const MAX_OUTPUT_SIZE: usize = 30_000;

static SHELL_MANAGERS: OnceLock<Mutex<HashMap<PathBuf, SharedShellManager>>> = OnceLock::new();

/// Status of a shell process
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ShellStatus {
    Running,
    Completed,
    Failed,
    Killed,
    TimedOut,
    Orphaned,
}

/// Result from a shell command execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellResult {
    pub task_id: Option<String>,
    pub status: ShellStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    /// Whether the command was executed in a sandbox.
    #[serde(default)]
    pub sandboxed: bool,
    /// Type of sandbox used (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_type: Option<String>,
    /// Whether the command was blocked by sandbox restrictions.
    #[serde(default)]
    pub sandbox_denied: bool,
}

/// A background shell process being tracked
pub struct BackgroundShell {
    pub id: String,
    pub command: String,
    pub working_dir: PathBuf,
    pub status: ShellStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub started_at: Instant,
    pub started_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    pub sandbox_type: SandboxType,
    child: Option<Child>,
    stdout_thread: Option<std::thread::JoinHandle<Vec<u8>>>,
    stderr_thread: Option<std::thread::JoinHandle<Vec<u8>>>,
}

impl BackgroundShell {
    /// Check if the process has completed and update status
    fn poll(&mut self) -> bool {
        if self.status != ShellStatus::Running {
            return true;
        }

        if let Some(ref mut child) = self.child {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.exit_code = status.code();
                    self.status = if status.success() {
                        ShellStatus::Completed
                    } else {
                        ShellStatus::Failed
                    };
                    self.collect_output();
                    self.updated_at_utc = Utc::now();
                    true
                }
                Ok(None) => false, // Still running
                Err(_) => {
                    self.status = ShellStatus::Failed;
                    self.updated_at_utc = Utc::now();
                    true
                }
            }
        } else {
            true
        }
    }

    /// Collect output from the background threads
    fn collect_output(&mut self) {
        if let Some(handle) = self.stdout_thread.take()
            && let Ok(data) = handle.join()
        {
            self.stdout = String::from_utf8_lossy(&data).to_string();
        }
        if let Some(handle) = self.stderr_thread.take()
            && let Ok(data) = handle.join()
        {
            self.stderr = String::from_utf8_lossy(&data).to_string();
        }
    }

    /// Kill the process
    fn kill(&mut self) -> Result<()> {
        if let Some(ref mut child) = self.child {
            child.kill().context("Failed to kill process")?;
            let _ = child.wait(); // Reap the zombie
            self.status = ShellStatus::Killed;
            self.collect_output();
            self.updated_at_utc = Utc::now();
        }
        Ok(())
    }

    /// Get a snapshot of the current state
    pub fn snapshot(&self) -> ShellResult {
        let sandboxed = !matches!(self.sandbox_type, SandboxType::None);
        ShellResult {
            task_id: Some(self.id.clone()),
            status: self.status.clone(),
            exit_code: self.exit_code,
            stdout: truncate_output(&self.stdout),
            stderr: truncate_output(&self.stderr),
            duration_ms: u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
            sandboxed,
            sandbox_type: if sandboxed {
                Some(self.sandbox_type.to_string())
            } else {
                None
            },
            sandbox_denied: false, // Determined after completion
        }
    }
}

const SHELL_STATE_FILE_NAME: &str = "background_jobs.json";
const SHELL_STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedShellState {
    version: u32,
    tasks: Vec<PersistedShellTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedShellTask {
    id: String,
    command: String,
    working_dir: PathBuf,
    status: ShellStatus,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    started_at_unix_ms: i64,
    updated_at_unix_ms: i64,
    sandbox_type: String,
    pid: Option<u32>,
}

impl PersistedShellTask {
    fn from_background(shell: &BackgroundShell) -> Self {
        Self {
            id: shell.id.clone(),
            command: shell.command.clone(),
            working_dir: shell.working_dir.clone(),
            status: shell.status.clone(),
            exit_code: shell.exit_code,
            stdout: shell.stdout.clone(),
            stderr: shell.stderr.clone(),
            started_at_unix_ms: shell.started_at_utc.timestamp_millis(),
            updated_at_unix_ms: shell.updated_at_utc.timestamp_millis(),
            sandbox_type: shell.sandbox_type.to_string(),
            pid: shell.child.as_ref().map(std::process::Child::id),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellRestoreReport {
    pub restored_jobs: usize,
    pub orphaned_jobs: usize,
    pub warnings: Vec<String>,
}

/// Manages background shell processes with optional sandboxing.
pub struct ShellManager {
    processes: HashMap<String, BackgroundShell>,
    default_workspace: PathBuf,
    sandbox_manager: SandboxManager,
    sandbox_policy: ExecutionSandboxPolicy,
    state_loaded: bool,
}

impl ShellManager {
    /// Create a new `ShellManager` with default (no sandbox) policy.
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            processes: HashMap::new(),
            default_workspace: workspace,
            sandbox_manager: SandboxManager::new(),
            sandbox_policy: ExecutionSandboxPolicy::default(),
            state_loaded: false,
        }
    }

    /// Create a new `ShellManager` with a specific sandbox policy.
    pub fn with_sandbox(workspace: PathBuf, policy: ExecutionSandboxPolicy) -> Self {
        Self {
            processes: HashMap::new(),
            default_workspace: workspace,
            sandbox_manager: SandboxManager::new(),
            sandbox_policy: policy,
            state_loaded: false,
        }
    }

    /// Set the sandbox policy for future commands.
    pub fn set_sandbox_policy(&mut self, policy: ExecutionSandboxPolicy) {
        self.sandbox_policy = policy;
    }

    /// Get the current sandbox policy.
    pub fn sandbox_policy(&self) -> &ExecutionSandboxPolicy {
        &self.sandbox_policy
    }

    /// Check if sandboxing is available on this platform.
    pub fn is_sandbox_available(&mut self) -> bool {
        self.sandbox_manager.is_available()
    }

    fn state_file_path(&self) -> PathBuf {
        self.default_workspace
            .join(".minimax")
            .join("state")
            .join(SHELL_STATE_FILE_NAME)
    }

    fn save_state(&self) -> Result<()> {
        let state_path = self.state_file_path();
        if let Some(parent) = state_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }

        let state = PersistedShellState {
            version: SHELL_STATE_VERSION,
            tasks: self
                .processes
                .values()
                .map(PersistedShellTask::from_background)
                .collect(),
        };
        let content = serde_json::to_string_pretty(&state)
            .with_context(|| format!("Failed to serialize {}", state_path.display()))?;
        fs::write(&state_path, content)
            .with_context(|| format!("Failed to write {}", state_path.display()))?;
        Ok(())
    }

    fn save_state_soft(&self) {
        if let Err(err) = self.save_state() {
            eprintln!("Warning: failed to persist shell state: {err}");
        }
    }

    pub fn ensure_state_loaded(&mut self) -> ShellRestoreReport {
        if self.state_loaded {
            ShellRestoreReport::default()
        } else {
            self.load_state()
        }
    }

    pub fn reload_state(&mut self) -> ShellRestoreReport {
        self.state_loaded = false;
        self.load_state()
    }

    fn load_state(&mut self) -> ShellRestoreReport {
        self.state_loaded = true;
        let mut report = ShellRestoreReport::default();
        let state_path = self.state_file_path();
        let content = match fs::read_to_string(&state_path) {
            Ok(content) => content,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return report,
            Err(err) => {
                report.warnings.push(format!(
                    "Failed to read shell state {}: {err}",
                    state_path.display()
                ));
                return report;
            }
        };

        let persisted = match serde_json::from_str::<PersistedShellState>(&content) {
            Ok(persisted) => persisted,
            Err(err) => {
                report.warnings.push(format!(
                    "Failed to parse shell state {}: {err}",
                    state_path.display()
                ));
                return report;
            }
        };

        for task in persisted.tasks {
            if self.processes.contains_key(&task.id) {
                continue;
            }

            let mut status = task.status;
            let mut stderr = task.stderr;
            if status == ShellStatus::Running {
                let reason = match task.pid {
                    Some(pid) if is_process_alive(pid) => format!(
                        "orphaned: process {pid} is still running but cannot be reattached after restart"
                    ),
                    Some(pid) => {
                        format!("orphaned: process {pid} was no longer alive during restore")
                    }
                    None => "orphaned: running task had no recorded pid during restore".to_string(),
                };
                status = ShellStatus::Orphaned;
                if stderr.trim().is_empty() {
                    stderr = reason;
                } else {
                    stderr = format!("{stderr}\n{reason}");
                }
                report.orphaned_jobs += 1;
            }

            let started_at_utc = datetime_from_unix_millis(task.started_at_unix_ms);
            let updated_at_utc = datetime_from_unix_millis(task.updated_at_unix_ms);
            let bg_shell = BackgroundShell {
                id: task.id.clone(),
                command: task.command,
                working_dir: task.working_dir,
                status,
                exit_code: task.exit_code,
                stdout: task.stdout,
                stderr,
                started_at: instant_from_unix_ms(task.started_at_unix_ms),
                started_at_utc,
                updated_at_utc,
                sandbox_type: sandbox_type_from_string(&task.sandbox_type),
                child: None,
                stdout_thread: None,
                stderr_thread: None,
            };
            self.processes.insert(task.id, bg_shell);
            report.restored_jobs += 1;
        }

        if report.restored_jobs > 0 || report.orphaned_jobs > 0 {
            self.save_state_soft();
        }

        report
    }

    /// Execute a shell command with the configured sandbox policy.
    pub fn execute(
        &mut self,
        command: &str,
        working_dir: Option<&str>,
        timeout_ms: u64,
        background: bool,
    ) -> Result<ShellResult> {
        self.execute_with_policy(command, working_dir, timeout_ms, background, None)
    }

    /// Execute a shell command with a specific sandbox policy (overrides default).
    pub fn execute_with_policy(
        &mut self,
        command: &str,
        working_dir: Option<&str>,
        timeout_ms: u64,
        background: bool,
        policy_override: Option<ExecutionSandboxPolicy>,
    ) -> Result<ShellResult> {
        let work_dir = working_dir.map_or_else(|| self.default_workspace.clone(), PathBuf::from);

        // Clamp timeout to max 10 minutes (600000ms)
        let timeout_ms = timeout_ms.clamp(1000, 600_000);

        // Use override policy if provided, otherwise use the manager's policy
        let policy = policy_override.unwrap_or_else(|| self.sandbox_policy.clone());

        // Create command spec and prepare sandboxed environment
        let spec = CommandSpec::shell(command, work_dir.clone(), Duration::from_millis(timeout_ms))
            .with_policy(policy);
        let exec_env = self.sandbox_manager.prepare(&spec);

        if background {
            self.spawn_background_sandboxed(command, &work_dir, &exec_env)
        } else {
            Self::execute_sync_sandboxed(command, &work_dir, timeout_ms, &exec_env)
        }
    }

    /// Execute a shell command interactively (stdin/stdout/stderr inherit from terminal).
    pub fn execute_interactive(
        &mut self,
        command: &str,
        working_dir: Option<&str>,
        timeout_ms: u64,
    ) -> Result<ShellResult> {
        self.execute_interactive_with_policy(command, working_dir, timeout_ms, None)
    }

    /// Execute a shell command interactively with a specific sandbox policy override.
    pub fn execute_interactive_with_policy(
        &mut self,
        command: &str,
        working_dir: Option<&str>,
        timeout_ms: u64,
        policy_override: Option<ExecutionSandboxPolicy>,
    ) -> Result<ShellResult> {
        let work_dir = working_dir.map_or_else(|| self.default_workspace.clone(), PathBuf::from);

        let timeout_ms = timeout_ms.clamp(1000, 600_000);
        let policy = policy_override.unwrap_or_else(|| self.sandbox_policy.clone());

        let spec = CommandSpec::shell(command, work_dir.clone(), Duration::from_millis(timeout_ms))
            .with_policy(policy);
        let exec_env = self.sandbox_manager.prepare(&spec);

        Self::execute_interactive_sandboxed(command, &work_dir, timeout_ms, &exec_env)
    }

    /// Execute command synchronously with timeout (sandboxed).
    fn execute_sync_sandboxed(
        original_command: &str,
        working_dir: &std::path::Path,
        timeout_ms: u64,
        exec_env: &ExecEnv,
    ) -> Result<ShellResult> {
        let started = Instant::now();
        let timeout = Duration::from_millis(timeout_ms);
        let sandbox_type = exec_env.sandbox_type;
        let sandboxed = exec_env.is_sandboxed();

        // Build the command from ExecEnv
        let program = exec_env.program();
        let args = exec_env.args();

        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Set environment variables from exec_env
        for (key, value) in &exec_env.env {
            cmd.env(key, value);
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to execute: {original_command}"))?;

        let stdout_handle = child.stdout.take().context("Failed to capture stdout")?;
        let stderr_handle = child.stderr.take().context("Failed to capture stderr")?;

        // Spawn threads to read output
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

        // Wait with timeout
        if let Some(status) = child.wait_timeout(timeout)? {
            let stdout = stdout_thread.join().unwrap_or_default();
            let stderr = stderr_thread.join().unwrap_or_default();
            let stderr_str = String::from_utf8_lossy(&stderr);
            let exit_code = status.code().unwrap_or(-1);

            // Check if sandbox denied the operation
            let sandbox_denied = SandboxManager::was_denied(sandbox_type, exit_code, &stderr_str);

            Ok(ShellResult {
                task_id: None,
                status: if status.success() {
                    ShellStatus::Completed
                } else {
                    ShellStatus::Failed
                },
                exit_code: status.code(),
                stdout: truncate_output(&String::from_utf8_lossy(&stdout)),
                stderr: truncate_output(&stderr_str),
                duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                sandboxed,
                sandbox_type: if sandboxed {
                    Some(sandbox_type.to_string())
                } else {
                    None
                },
                sandbox_denied,
            })
        } else {
            // Timeout - kill the process
            let _ = child.kill();
            let status = child.wait().ok();
            let stdout = stdout_thread.join().unwrap_or_default();
            let stderr = stderr_thread.join().unwrap_or_default();

            Ok(ShellResult {
                task_id: None,
                status: ShellStatus::TimedOut,
                exit_code: status.and_then(|s| s.code()),
                stdout: truncate_output(&String::from_utf8_lossy(&stdout)),
                stderr: truncate_output(&String::from_utf8_lossy(&stderr)),
                duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                sandboxed,
                sandbox_type: if sandboxed {
                    Some(sandbox_type.to_string())
                } else {
                    None
                },
                sandbox_denied: false,
            })
        }
    }

    /// Execute command interactively with timeout (sandboxed).
    fn execute_interactive_sandboxed(
        original_command: &str,
        working_dir: &std::path::Path,
        timeout_ms: u64,
        exec_env: &ExecEnv,
    ) -> Result<ShellResult> {
        let started = Instant::now();
        let timeout = Duration::from_millis(timeout_ms);
        let sandbox_type = exec_env.sandbox_type;
        let sandboxed = exec_env.is_sandboxed();

        let program = exec_env.program();
        let args = exec_env.args();

        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(working_dir)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        for (key, value) in &exec_env.env {
            cmd.env(key, value);
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to execute: {original_command}"))?;

        if let Some(status) = child.wait_timeout(timeout)? {
            Ok(ShellResult {
                task_id: None,
                status: if status.success() {
                    ShellStatus::Completed
                } else {
                    ShellStatus::Failed
                },
                exit_code: status.code(),
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                sandboxed,
                sandbox_type: if sandboxed {
                    Some(sandbox_type.to_string())
                } else {
                    None
                },
                sandbox_denied: false,
            })
        } else {
            let _ = child.kill();
            let status = child.wait().ok();

            Ok(ShellResult {
                task_id: None,
                status: ShellStatus::TimedOut,
                exit_code: status.and_then(|s| s.code()),
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                sandboxed,
                sandbox_type: if sandboxed {
                    Some(sandbox_type.to_string())
                } else {
                    None
                },
                sandbox_denied: false,
            })
        }
    }

    /// Spawn a background process (sandboxed).
    fn spawn_background_sandboxed(
        &mut self,
        original_command: &str,
        working_dir: &std::path::Path,
        exec_env: &ExecEnv,
    ) -> Result<ShellResult> {
        let task_id = format!("shell_{}", &Uuid::new_v4().to_string()[..8]);
        let started = Instant::now();
        let started_utc = Utc::now();
        let sandbox_type = exec_env.sandbox_type;
        let sandboxed = exec_env.is_sandboxed();

        // Build the command from ExecEnv
        let program = exec_env.program();
        let args = exec_env.args();

        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Set environment variables from exec_env
        for (key, value) in &exec_env.env {
            cmd.env(key, value);
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn background: {original_command}"))?;

        let stdout_handle = child.stdout.take();
        let stderr_handle = child.stderr.take();

        // Spawn threads to collect output
        let stdout_thread = stdout_handle.map(|handle| {
            std::thread::spawn(move || {
                let mut reader = handle;
                let mut buf = Vec::new();
                let _ = reader.read_to_end(&mut buf);
                buf
            })
        });

        let stderr_thread = stderr_handle.map(|handle| {
            std::thread::spawn(move || {
                let mut reader = handle;
                let mut buf = Vec::new();
                let _ = reader.read_to_end(&mut buf);
                buf
            })
        });

        let bg_shell = BackgroundShell {
            id: task_id.clone(),
            command: original_command.to_string(),
            working_dir: working_dir.to_path_buf(),
            status: ShellStatus::Running,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            started_at: started,
            started_at_utc: started_utc,
            updated_at_utc: started_utc,
            sandbox_type,
            child: Some(child),
            stdout_thread,
            stderr_thread,
        };

        self.processes.insert(task_id.clone(), bg_shell);
        self.save_state_soft();

        Ok(ShellResult {
            task_id: Some(task_id),
            status: ShellStatus::Running,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: 0,
            sandboxed,
            sandbox_type: if sandboxed {
                Some(sandbox_type.to_string())
            } else {
                None
            },
            sandbox_denied: false,
        })
    }

    /// Get output from a background process
    pub fn get_output(
        &mut self,
        task_id: &str,
        block: bool,
        timeout_ms: u64,
    ) -> Result<ShellResult> {
        let shell = self
            .processes
            .get_mut(task_id)
            .ok_or_else(|| anyhow!("Task {task_id} not found"))?;

        if block && shell.status == ShellStatus::Running {
            let timeout = Duration::from_millis(timeout_ms.clamp(1000, 600_000));
            let deadline = Instant::now() + timeout;

            while shell.status == ShellStatus::Running && Instant::now() < deadline {
                if shell.poll() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }

            // If still running after timeout
            if shell.status == ShellStatus::Running {
                return Ok(shell.snapshot());
            }
        } else {
            shell.poll();
        }

        let snapshot = shell.snapshot();
        self.save_state_soft();
        Ok(snapshot)
    }

    /// Kill a running background process
    pub fn kill(&mut self, task_id: &str) -> Result<ShellResult> {
        let shell = self
            .processes
            .get_mut(task_id)
            .ok_or_else(|| anyhow!("Task {task_id} not found"))?;

        shell.kill()?;
        let snapshot = shell.snapshot();
        self.save_state_soft();
        Ok(snapshot)
    }

    /// List all background processes
    pub fn list(&mut self) -> Vec<ShellResult> {
        // Poll all processes first
        for shell in self.processes.values_mut() {
            shell.poll();
        }
        self.save_state_soft();

        self.processes
            .values()
            .map(BackgroundShell::snapshot)
            .collect()
    }

    /// Clean up completed processes older than the given duration
    pub fn cleanup(&mut self, max_age: Duration) {
        let _now = Instant::now();
        self.processes.retain(|_, shell| {
            if shell.status == ShellStatus::Running {
                true
            } else {
                shell.started_at.elapsed() < max_age
            }
        });
        self.save_state_soft();
    }
}

fn instant_from_unix_ms(timestamp_ms: i64) -> Instant {
    let timestamp_ms = u64::try_from(timestamp_ms).unwrap_or(0);
    let then = UNIX_EPOCH + Duration::from_millis(timestamp_ms);
    let elapsed = SystemTime::now()
        .duration_since(then)
        .unwrap_or_else(|_| Duration::from_secs(0));
    Instant::now()
        .checked_sub(elapsed)
        .unwrap_or_else(Instant::now)
}

fn datetime_from_unix_millis(timestamp_ms: i64) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp_millis(timestamp_ms).unwrap_or_else(Utc::now)
}

fn sandbox_type_from_string(value: &str) -> SandboxType {
    match value {
        #[cfg(target_os = "macos")]
        "macos-seatbelt" => SandboxType::MacosSeatbelt,
        #[cfg(target_os = "linux")]
        "linux-landlock" => SandboxType::LinuxLandlock,
        _ => SandboxType::None,
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn is_process_alive(pid: u32) -> bool {
    let pid = i32::try_from(pid).unwrap_or(i32::MAX);
    // SAFETY: kill(pid, 0) only probes process existence and sends no signal.
    let result = unsafe { libc::kill(pid, 0) };
    if result == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn is_process_alive(_pid: u32) -> bool {
    false
}

/// Truncate output to `MAX_OUTPUT_SIZE`
fn truncate_output(output: &str) -> String {
    if output.len() <= MAX_OUTPUT_SIZE {
        output.to_string()
    } else {
        let truncated = truncate_to_boundary(output, MAX_OUTPUT_SIZE);
        format!(
            "{}...\n\n[Output truncated at {} characters. {} characters omitted.]",
            truncated,
            MAX_OUTPUT_SIZE,
            output.len() - MAX_OUTPUT_SIZE
        )
    }
}

/// Thread-safe wrapper for `ShellManager`
pub type SharedShellManager = Arc<Mutex<ShellManager>>;

/// Create a new shared shell manager with default sandbox policy.
pub fn new_shared_shell_manager(workspace: PathBuf) -> SharedShellManager {
    Arc::new(Mutex::new(ShellManager::new(workspace)))
}

/// Create a new shared shell manager with a specific sandbox policy.
pub fn new_shared_shell_manager_with_sandbox(
    workspace: PathBuf,
    policy: ExecutionSandboxPolicy,
) -> SharedShellManager {
    Arc::new(Mutex::new(ShellManager::with_sandbox(workspace, policy)))
}

fn shell_manager_key(workspace: &std::path::Path) -> PathBuf {
    workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf())
}

fn shared_shell_manager(workspace: &std::path::Path) -> SharedShellManager {
    let key = shell_manager_key(workspace);
    let managers = SHELL_MANAGERS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut managers = managers.lock().expect("lock shell manager registry");
    managers
        .entry(key.clone())
        .or_insert_with(|| new_shared_shell_manager(key))
        .clone()
}

/// Get the shared shell manager for a workspace.
pub fn shared_shell_manager_for_workspace(workspace: &std::path::Path) -> SharedShellManager {
    let manager = shared_shell_manager(workspace);
    if let Ok(mut guard) = manager.lock() {
        let _ = guard.ensure_state_loaded();
    }
    manager
}

/// Restore persisted background shell state for a workspace.
#[must_use]
pub fn restore_shell_state_for_workspace(
    workspace: &std::path::Path,
    force_reload: bool,
) -> ShellRestoreReport {
    let manager = shared_shell_manager(workspace);
    let Ok(mut manager) = manager.lock() else {
        return ShellRestoreReport {
            restored_jobs: 0,
            orphaned_jobs: 0,
            warnings: vec!["Failed to lock shell manager".to_string()],
        };
    };

    if force_reload {
        manager.reload_state()
    } else {
        manager.ensure_state_loaded()
    }
}

// === ToolSpec Implementations ===

use crate::command_safety::{SafetyLevel, analyze_command};
use crate::tools::spec::{
    ApprovalRequirement, SandboxPolicy as ToolSandboxPolicy, ToolCapability, ToolContext,
    ToolError, ToolResult, ToolSpec, optional_bool, optional_u64, required_str,
};
use crate::utils::truncate_to_boundary;
use async_trait::async_trait;
use serde_json::json;

fn policy_override_from_context(context: &ToolContext) -> Option<ExecutionSandboxPolicy> {
    match &context.sandbox_policy {
        ToolSandboxPolicy::None => None,
        ToolSandboxPolicy::Standard {
            writable_roots,
            allow_network,
        } => Some(ExecutionSandboxPolicy::WorkspaceWrite {
            writable_roots: writable_roots.clone(),
            network_access: *allow_network,
            exclude_tmpdir: false,
            exclude_slash_tmp: false,
        }),
    }
}

fn format_shell_result(result: &ShellResult, interactive: bool, wait_mode: bool) -> (String, bool) {
    let output = if interactive {
        format!(
            "Interactive command completed (exit code: {:?})",
            result.exit_code
        )
    } else if result.status == ShellStatus::Completed {
        if result.stdout.is_empty() && result.stderr.is_empty() {
            "(no output)".to_string()
        } else if result.stderr.is_empty() {
            result.stdout.clone()
        } else {
            format!("{}\n\nSTDERR:\n{}", result.stdout, result.stderr)
        }
    } else if result.status == ShellStatus::Running {
        let task_id_str = result.task_id.clone().unwrap_or_default();
        if wait_mode {
            format!("Background task still running: {task_id_str}")
        } else {
            format!("Background task started: {task_id_str}")
        }
    } else {
        format!(
            "Command failed (exit code: {:?})\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
            result.exit_code, result.stdout, result.stderr
        )
    };

    let success = result.status == ShellStatus::Completed || result.status == ShellStatus::Running;
    (output, success)
}

/// Tool for executing shell commands.
pub struct ExecShellTool;

#[async_trait]
impl ToolSpec for ExecShellTool {
    fn name(&self) -> &'static str {
        "exec_shell"
    }

    fn description(&self) -> &'static str {
        "Execute a shell command in the workspace directory. Returns stdout, stderr, and exit code."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (default: 120000, max: 600000)"
                },
                "background": {
                    "type": "boolean",
                    "description": "Run in background and return task_id (default: false)"
                },
                "interactive": {
                    "type": "boolean",
                    "description": "Run interactively with terminal IO (default: false)"
                }
            },
            "required": ["command"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![
            ToolCapability::ExecutesCode,
            ToolCapability::Sandboxable,
            ToolCapability::RequiresApproval,
        ]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let command = required_str(&input, "command")?;
        let timeout_ms = optional_u64(&input, "timeout_ms", 120_000).min(600_000);
        let background = optional_bool(&input, "background", false);
        let interactive = optional_bool(&input, "interactive", false);

        if interactive && background {
            return Ok(ToolResult::error(
                "Interactive commands cannot run in background mode.",
            ));
        }

        // Safety analysis before execution
        let safety = analyze_command(command);
        match safety.level {
            SafetyLevel::Dangerous => {
                let reasons = safety.reasons.join("; ");
                let suggestions = if safety.suggestions.is_empty() {
                    String::new()
                } else {
                    format!("\nSuggestions: {}", safety.suggestions.join("; "))
                };
                return Ok(ToolResult {
                    content: format!(
                        "BLOCKED: This command was blocked for safety reasons.\n\nReasons: {reasons}{suggestions}"
                    ),
                    success: false,
                    metadata: Some(json!({
                        "safety_level": "dangerous",
                        "blocked": true,
                        "reasons": safety.reasons,
                        "suggestions": safety.suggestions,
                    })),
                });
            }
            SafetyLevel::RequiresApproval | SafetyLevel::Safe | SafetyLevel::WorkspaceSafe => {
                // Proceed normally
            }
        }

        let policy_override = policy_override_from_context(context);
        let result = if background {
            let manager = shared_shell_manager_for_workspace(&context.workspace);
            let mut manager = manager
                .lock()
                .map_err(|_| ToolError::execution_failed("Failed to lock shell manager"))?;
            manager.execute_with_policy(command, None, timeout_ms, true, policy_override)
        } else {
            let mut manager = ShellManager::new(context.workspace.clone());
            if interactive {
                manager.execute_interactive_with_policy(command, None, timeout_ms, policy_override)
            } else {
                manager.execute_with_policy(command, None, timeout_ms, false, policy_override)
            }
        };

        match result {
            Ok(result) => {
                let (output, success) = format_shell_result(&result, interactive, false);

                Ok(ToolResult {
                    content: output,
                    success,
                    metadata: Some(json!({
                        "exit_code": result.exit_code,
                        "status": format!("{:?}", result.status),
                        "duration_ms": result.duration_ms,
                        "sandboxed": result.sandboxed,
                        "sandbox_denied": result.sandbox_denied,
                        "task_id": result.task_id,
                        "safety_level": format!("{:?}", safety.level),
                        "interactive": interactive,
                    })),
                })
            }
            Err(e) => Ok(ToolResult::error(format!("Shell execution failed: {e}"))),
        }
    }
}

/// Tool for waiting on background shell commands.
pub struct ExecShellWaitTool {
    name: &'static str,
}

impl ExecShellWaitTool {
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self { name }
    }
}

#[async_trait]
impl ToolSpec for ExecShellWaitTool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        "Wait for a background exec_shell task and return its output."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Background task id from exec_shell"
                },
                "block": {
                    "type": "boolean",
                    "description": "Whether to wait for completion (default: true)"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Wait timeout in milliseconds (default: 120000, max: 600000)"
                }
            },
            "required": ["task_id"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let task_id = required_str(&input, "task_id")?;
        let block = optional_bool(&input, "block", true);
        let timeout_ms = optional_u64(&input, "timeout_ms", 120_000).min(600_000);

        let manager = shared_shell_manager_for_workspace(&context.workspace);
        let mut manager = manager
            .lock()
            .map_err(|_| ToolError::execution_failed("Failed to lock shell manager"))?;
        let result = manager
            .get_output(task_id, block, timeout_ms)
            .map_err(|e| ToolError::execution_failed(format!("Shell wait failed: {e}")))?;

        let (output, success) = format_shell_result(&result, false, true);
        Ok(ToolResult {
            content: output,
            success,
            metadata: Some(json!({
                "exit_code": result.exit_code,
                "status": format!("{:?}", result.status),
                "duration_ms": result.duration_ms,
                "sandboxed": result.sandboxed,
                "sandbox_denied": result.sandbox_denied,
                "task_id": result.task_id,
            })),
        })
    }
}

/// Tool for killing a background shell command.
pub struct ExecShellKillTool {
    name: &'static str,
}

impl ExecShellKillTool {
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self { name }
    }
}

/// Tool for listing background shell commands.
pub struct ExecShellListTool {
    name: &'static str,
}

impl ExecShellListTool {
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self { name }
    }
}

#[async_trait]
impl ToolSpec for ExecShellListTool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        "List tracked background exec_shell tasks."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "description": "Optional status filter: running|completed|failed|killed|timedout|orphaned"
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let status_filter = input
            .get("status")
            .and_then(serde_json::Value::as_str)
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty());

        let manager = shared_shell_manager_for_workspace(&context.workspace);
        let mut manager = manager
            .lock()
            .map_err(|_| ToolError::execution_failed("Failed to lock shell manager"))?;
        let mut tasks = manager.list();

        if let Some(filter) = status_filter.as_deref() {
            tasks.retain(|task| {
                let label = match task.status {
                    ShellStatus::Running => "running",
                    ShellStatus::Completed => "completed",
                    ShellStatus::Failed => "failed",
                    ShellStatus::Killed => "killed",
                    ShellStatus::TimedOut => "timedout",
                    ShellStatus::Orphaned => "orphaned",
                };
                label == filter
            });
        }

        if tasks.is_empty() {
            return Ok(ToolResult::success("No background shell tasks."));
        }

        let mut lines = vec![format!("Background shell tasks: {}", tasks.len())];
        for task in &tasks {
            let task_id = task.task_id.as_deref().unwrap_or("unknown");
            lines.push(format!(
                "- {task_id} | {:?} | exit={:?} | {}ms",
                task.status, task.exit_code, task.duration_ms
            ));
        }

        Ok(ToolResult {
            content: lines.join("\n"),
            success: true,
            metadata: Some(json!({
                "count": tasks.len(),
                "tasks": tasks,
            })),
        })
    }
}

/// Tool for cleaning tracked background shell commands.
pub struct ExecShellCleanTool {
    name: &'static str,
}

impl ExecShellCleanTool {
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self { name }
    }
}

#[async_trait]
impl ToolSpec for ExecShellCleanTool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        "Clean tracked background exec_shell tasks. Optionally kills running tasks first."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "max_age_ms": {
                    "type": "integer",
                    "description": "Remove completed tasks older than this age (default: 300000)"
                },
                "kill_running": {
                    "type": "boolean",
                    "description": "Kill running tasks before cleanup (default: false)"
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::RequiresApproval]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let max_age_ms = optional_u64(&input, "max_age_ms", 300_000).min(86_400_000);
        let kill_running = optional_bool(&input, "kill_running", false);

        let manager = shared_shell_manager_for_workspace(&context.workspace);
        let mut manager = manager
            .lock()
            .map_err(|_| ToolError::execution_failed("Failed to lock shell manager"))?;

        let before = manager.list();

        let mut killed = 0usize;
        if kill_running {
            let running_ids: Vec<String> = before
                .iter()
                .filter(|task| task.status == ShellStatus::Running)
                .filter_map(|task| task.task_id.clone())
                .collect();
            for task_id in running_ids {
                if manager.kill(&task_id).is_ok() {
                    killed += 1;
                }
            }
            manager.cleanup(Duration::from_millis(0));
        } else {
            manager.cleanup(Duration::from_millis(max_age_ms));
        }

        let after = manager.list();
        let removed = before.len().saturating_sub(after.len());

        Ok(ToolResult {
            content: format!(
                "Background task cleanup complete.\nRemoved: {removed}\nKilled: {killed}\nRemaining: {}",
                after.len()
            ),
            success: true,
            metadata: Some(json!({
                "removed": removed,
                "killed": killed,
                "remaining": after.len(),
                "max_age_ms": max_age_ms,
                "kill_running": kill_running,
            })),
        })
    }
}

#[async_trait]
impl ToolSpec for ExecShellKillTool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        "Terminate a background exec_shell task."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Background task id from exec_shell"
                }
            },
            "required": ["task_id"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::RequiresApproval]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let task_id = required_str(&input, "task_id")?;

        let manager = shared_shell_manager_for_workspace(&context.workspace);
        let mut manager = manager
            .lock()
            .map_err(|_| ToolError::execution_failed("Failed to lock shell manager"))?;
        let result = manager
            .kill(task_id)
            .map_err(|e| ToolError::execution_failed(format!("Shell kill failed: {e}")))?;

        let (output, success) = format_shell_result(&result, false, true);
        Ok(ToolResult {
            content: output,
            success,
            metadata: Some(json!({
                "exit_code": result.exit_code,
                "status": format!("{:?}", result.status),
                "duration_ms": result.duration_ms,
                "sandboxed": result.sandboxed,
                "sandbox_denied": result.sandbox_denied,
                "task_id": result.task_id,
            })),
        })
    }
}

/// Tool for running a command interactively in the terminal.
pub struct ExecShellInteractTool {
    name: &'static str,
}

impl ExecShellInteractTool {
    #[must_use]
    pub fn new(name: &'static str) -> Self {
        Self { name }
    }
}

#[async_trait]
impl ToolSpec for ExecShellInteractTool {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        "Execute a shell command interactively (inherits terminal IO)."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "timeout_ms": {
                    "type": "integer",
                    "description": "Timeout in milliseconds (default: 120000, max: 600000)"
                }
            },
            "required": ["command"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![
            ToolCapability::ExecutesCode,
            ToolCapability::RequiresApproval,
        ]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let mut input_obj = input.as_object().cloned().unwrap_or_default();
        input_obj.insert("interactive".to_string(), serde_json::Value::Bool(true));
        input_obj.insert("background".to_string(), serde_json::Value::Bool(false));
        ExecShellTool
            .execute(serde_json::Value::Object(input_obj), context)
            .await
    }
}

/// Tool for appending notes to a notes file.
pub struct NoteTool;

#[async_trait]
impl ToolSpec for NoteTool {
    fn name(&self) -> &'static str {
        "note"
    }

    fn description(&self) -> &'static str {
        "Append a note to the agent notes file for persistent context across sessions."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "content": {
                    "type": "string",
                    "description": "The note content to append"
                }
            },
            "required": ["content"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::WritesFiles]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto // Notes are low-risk
    }

    async fn execute(
        &self,
        input: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        let note_content = required_str(&input, "content")?;

        // Ensure parent directory exists
        if let Some(parent) = context.notes_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ToolError::execution_failed(format!("Failed to create notes directory: {e}"))
            })?;
        }

        // Append to notes file
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&context.notes_path)
            .map_err(|e| ToolError::execution_failed(format!("Failed to open notes file: {e}")))?;

        writeln!(file, "\n---\n{note_content}")
            .map_err(|e| ToolError::execution_failed(format!("Failed to write note: {e}")))?;

        Ok(ToolResult::success(format!(
            "Note appended to {}",
            context.notes_path.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn echo_command(message: &str) -> String {
        format!("echo {message}")
    }

    fn sleep_command(seconds: u64) -> String {
        #[cfg(windows)]
        {
            let ping_count = seconds.saturating_add(1);
            let ps_path = r#"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"#;
            format!(
                "\"{ps_path}\" -NoProfile -Command \"Start-Sleep -Seconds {seconds}\" || ping 127.0.0.1 -n {ping_count} > NUL"
            )
        }
        #[cfg(not(windows))]
        {
            format!("sleep {seconds}")
        }
    }

    fn safe_sleep_command(seconds: u64) -> String {
        #[cfg(windows)]
        {
            let ps_path = r#"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"#;
            format!("\"{ps_path}\" -NoProfile -Command \"Start-Sleep -Seconds {seconds}\"")
        }
        #[cfg(not(windows))]
        {
            format!("sleep {seconds}")
        }
    }

    fn sleep_then_echo_command(seconds: u64, message: &str) -> String {
        #[cfg(windows)]
        {
            let ping_count = seconds.saturating_add(1);
            let ps_path = r#"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe"#;
            format!(
                "\"{ps_path}\" -NoProfile -Command \"Start-Sleep -Seconds {seconds}; Write-Output {message}\" || (ping 127.0.0.1 -n {ping_count} > NUL && echo {message})"
            )
        }
        #[cfg(not(windows))]
        {
            format!("sleep {seconds} && echo {message}")
        }
    }

    #[test]
    fn test_sync_execution() {
        let tmp = tempdir().expect("tempdir");
        let mut manager = ShellManager::new(tmp.path().to_path_buf());

        let result = manager
            .execute(&echo_command("hello"), None, 5000, false)
            .expect("execute");

        assert_eq!(result.status, ShellStatus::Completed);
        assert!(result.stdout.contains("hello"));
        assert!(result.task_id.is_none());
    }

    #[test]
    fn test_background_execution() {
        let tmp = tempdir().expect("tempdir");
        let mut manager = ShellManager::new(tmp.path().to_path_buf());

        let result = manager
            .execute(&sleep_then_echo_command(1, "done"), None, 5000, true)
            .expect("execute");

        assert_eq!(result.status, ShellStatus::Running);
        assert!(result.task_id.is_some());

        let task_id = result
            .task_id
            .expect("background execution should return task_id");

        // Wait for completion
        let final_result = manager
            .get_output(&task_id, true, 5000)
            .expect("get_output");

        assert_eq!(final_result.status, ShellStatus::Completed);
        assert!(final_result.stdout.contains("done"));
    }

    #[test]
    fn test_timeout() {
        let tmp = tempdir().expect("tempdir");
        let mut manager = ShellManager::new(tmp.path().to_path_buf());

        let result = manager
            .execute(&sleep_command(10), None, 1000, false)
            .expect("execute");

        assert_eq!(result.status, ShellStatus::TimedOut);
    }

    #[test]
    fn test_kill() {
        let tmp = tempdir().expect("tempdir");
        let mut manager = ShellManager::new(tmp.path().to_path_buf());

        let result = manager
            .execute(&sleep_command(60), None, 5000, true)
            .expect("execute");

        let task_id = result
            .task_id
            .expect("background execution should return task_id");

        // Kill it
        let killed = manager.kill(&task_id).expect("kill");
        assert_eq!(killed.status, ShellStatus::Killed);
    }

    #[test]
    fn test_output_truncation() {
        let long_output = "x".repeat(50_000);
        let truncated = truncate_output(&long_output);

        assert!(truncated.len() < long_output.len());
        assert!(truncated.contains("truncated"));
    }

    #[test]
    fn test_state_roundtrip_serialization() {
        let tmp = tempdir().expect("tempdir");
        let mut manager = ShellManager::new(tmp.path().to_path_buf());
        let result = manager
            .execute(&echo_command("persist"), None, 5_000, true)
            .expect("execute");
        let task_id = result.task_id.expect("task id");

        let _ = manager
            .get_output(&task_id, true, 5_000)
            .expect("wait for completion");
        manager.save_state().expect("persist state");

        let mut restored = ShellManager::new(tmp.path().to_path_buf());
        let report = restored.reload_state();
        assert!(report.warnings.is_empty());
        assert_eq!(report.restored_jobs, 1);
        assert_eq!(report.orphaned_jobs, 0);
        assert_eq!(restored.list().len(), 1);
    }

    #[test]
    fn test_restore_marks_running_jobs_orphaned() {
        let tmp = tempdir().expect("tempdir");
        let state_path = tmp
            .path()
            .join(".minimax")
            .join("state")
            .join(SHELL_STATE_FILE_NAME);
        std::fs::create_dir_all(state_path.parent().expect("state parent")).expect("mkdir");

        let persisted = PersistedShellState {
            version: SHELL_STATE_VERSION,
            tasks: vec![PersistedShellTask {
                id: "shell_deadbeef".to_string(),
                command: "sleep 60".to_string(),
                working_dir: tmp.path().to_path_buf(),
                status: ShellStatus::Running,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                started_at_unix_ms: Utc::now().timestamp_millis(),
                updated_at_unix_ms: Utc::now().timestamp_millis(),
                sandbox_type: "none".to_string(),
                pid: Some(u32::MAX),
            }],
        };
        std::fs::write(
            &state_path,
            serde_json::to_string_pretty(&persisted).expect("json"),
        )
        .expect("write");

        let mut restored = ShellManager::new(tmp.path().to_path_buf());
        let report = restored.reload_state();
        assert_eq!(report.restored_jobs, 1);
        assert_eq!(report.orphaned_jobs, 1);

        let jobs = restored.list();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, ShellStatus::Orphaned);
        assert!(jobs[0].stderr.contains("orphaned"));
    }

    #[test]
    fn test_restore_handles_corrupt_state_file() {
        let tmp = tempdir().expect("tempdir");
        let state_path = tmp
            .path()
            .join(".minimax")
            .join("state")
            .join(SHELL_STATE_FILE_NAME);
        std::fs::create_dir_all(state_path.parent().expect("state parent")).expect("mkdir");
        std::fs::write(&state_path, "{not valid json").expect("write");

        let mut manager = ShellManager::new(tmp.path().to_path_buf());
        let report = manager.reload_state();
        assert!(report.restored_jobs == 0);
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.contains("Failed to parse"))
        );
    }

    #[tokio::test]
    async fn test_exec_shell_wait_tool() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let exec = ExecShellTool;
        let wait_tool = ExecShellWaitTool::new("exec_shell_wait");
        let result = exec
            .execute(
                json!({
                    "command": echo_command("done"),
                    "timeout_ms": 5000,
                    "background": true
                }),
                &ctx,
            )
            .await
            .expect("execute");

        let task_id = result
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("task_id"))
            .and_then(|value| value.as_str())
            .expect("task_id");

        let waited = wait_tool
            .execute(
                json!({
                    "task_id": task_id,
                    "block": true,
                    "timeout_ms": 5000
                }),
                &ctx,
            )
            .await
            .expect("wait");

        assert!(waited.success);
        assert!(waited.content.contains("done"));
    }

    #[tokio::test]
    async fn test_exec_shell_kill_tool() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path().to_path_buf());

        let exec = ExecShellTool;
        let kill_tool = ExecShellKillTool::new("exec_shell_kill");
        let result = exec
            .execute(
                json!({
                    "command": safe_sleep_command(60),
                    "timeout_ms": 5000,
                    "background": true
                }),
                &ctx,
            )
            .await
            .expect("execute");

        let task_id = result
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("task_id"))
            .and_then(|value| value.as_str())
            .expect("task_id");

        let killed = kill_tool
            .execute(json!({ "task_id": task_id }), &ctx)
            .await
            .expect("kill");

        assert!(!killed.success);
        assert!(
            killed
                .metadata
                .as_ref()
                .and_then(|meta| meta.get("status"))
                .and_then(|value| value.as_str())
                == Some("Killed")
        );
    }
}
