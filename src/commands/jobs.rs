//! Background shell jobs command.

use super::CommandResult;
use crate::tools::shell::{ShellResult, ShellStatus, shared_shell_manager_for_workspace};
use crate::tui::app::App;
use std::time::Duration;

/// Manage background shell jobs.
///
/// Usage:
/// - `/jobs` or `/jobs list`
/// - `/jobs wait <task_id> [timeout_ms]`
/// - `/jobs kill <task_id>`
/// - `/jobs clean [max_age_ms]`
pub fn jobs(app: &mut App, arg: Option<&str>) -> CommandResult {
    let mut parts = arg.unwrap_or("list").split_whitespace();
    let subcommand = parts.next().unwrap_or("list").to_lowercase();

    let manager = shared_shell_manager_for_workspace(&app.workspace);
    let mut manager = match manager.lock() {
        Ok(guard) => guard,
        Err(_) => return CommandResult::error("Failed to lock shell manager"),
    };

    match subcommand.as_str() {
        "list" => {
            let tasks = manager.list();
            if tasks.is_empty() {
                return CommandResult::message("No background shell jobs.");
            }
            CommandResult::message(format_tasks(&tasks))
        }
        "wait" => {
            let Some(task_id) = parts.next() else {
                return CommandResult::error("Usage: /jobs wait <task_id> [timeout_ms]");
            };
            let timeout_ms = parts
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(120_000)
                .clamp(1_000, 600_000);

            match manager.get_output(task_id, true, timeout_ms) {
                Ok(task) => CommandResult::message(format_single_task(&task, true)),
                Err(e) => CommandResult::error(format!("Failed to wait on job: {e}")),
            }
        }
        "kill" => {
            let Some(task_id) = parts.next() else {
                return CommandResult::error("Usage: /jobs kill <task_id>");
            };
            match manager.kill(task_id) {
                Ok(task) => CommandResult::message(format!(
                    "Killed job {}\n{}",
                    task.task_id.as_deref().unwrap_or(task_id),
                    format_single_task(&task, true)
                )),
                Err(e) => CommandResult::error(format!("Failed to kill job: {e}")),
            }
        }
        "clean" => {
            let max_age_ms = parts
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(300_000)
                .min(86_400_000);
            let before = manager.list();
            manager.cleanup(Duration::from_millis(max_age_ms));
            let after = manager.list();
            let removed = before.len().saturating_sub(after.len());
            CommandResult::message(format!(
                "Cleaned background jobs.\nRemoved: {removed}\nRemaining: {}",
                after.len()
            ))
        }
        _ => CommandResult::error(
            "Usage: /jobs [list|wait <task_id> [timeout_ms]|kill <task_id>|clean [max_age_ms]]",
        ),
    }
}

fn format_tasks(tasks: &[ShellResult]) -> String {
    let mut lines = vec![format!("Background shell jobs: {}", tasks.len())];
    for task in tasks {
        lines.push(format_single_task(task, false));
    }
    lines.join("\n")
}

fn format_single_task(task: &ShellResult, include_output: bool) -> String {
    let status = match task.status {
        ShellStatus::Running => "running",
        ShellStatus::Completed => "completed",
        ShellStatus::Failed => "failed",
        ShellStatus::Killed => "killed",
        ShellStatus::TimedOut => "timedout",
        ShellStatus::Orphaned => "orphaned",
    };
    let task_id = task.task_id.as_deref().unwrap_or("unknown");
    let mut line = format!(
        "- {task_id} | {status} | exit={:?} | {}ms",
        task.exit_code, task.duration_ms
    );
    if include_output {
        if !task.stdout.is_empty() {
            line.push_str(&format!("\nSTDOUT:\n{}", task.stdout));
        }
        if !task.stderr.is_empty() {
            line.push_str(&format!("\nSTDERR:\n{}", task.stderr));
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::tui::app::{App, TuiOptions};
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn create_test_app(workspace: PathBuf) -> App {
        let options = TuiOptions {
            model: "test-model".to_string(),
            workspace,
            allow_shell: false,
            max_subagents: 1,
            skills_dir: PathBuf::from("./skills"),
            memory_path: PathBuf::from("memory.md"),
            notes_path: PathBuf::from("notes.txt"),
            mcp_config_path: PathBuf::from("mcp.json"),
            use_memory: false,
            start_in_agent_mode: false,
            yolo: false,
            resume_session_id: None,
        };
        App::new(options, &Config::default())
    }

    #[test]
    fn jobs_list_when_empty() {
        let tmp = tempdir().expect("tempdir");
        let mut app = create_test_app(tmp.path().to_path_buf());
        let result = jobs(&mut app, None);
        assert!(
            result
                .message
                .unwrap_or_default()
                .contains("No background shell jobs")
        );
    }

    #[test]
    fn jobs_list_restores_persisted_state() {
        let tmp = tempdir().expect("tempdir");
        let state_path = tmp
            .path()
            .join(".minimax")
            .join("state")
            .join("background_jobs.json");
        std::fs::create_dir_all(state_path.parent().expect("state parent")).expect("mkdir");
        std::fs::write(
            &state_path,
            serde_json::json!({
                "version": 1,
                "tasks": [{
                    "id": "shell_test123",
                    "command": "echo hi",
                    "working_dir": tmp.path(),
                    "status": "Running",
                    "exit_code": null,
                    "stdout": "",
                    "stderr": "",
                    "started_at_unix_ms": chrono::Utc::now().timestamp_millis(),
                    "updated_at_unix_ms": chrono::Utc::now().timestamp_millis(),
                    "sandbox_type": "none",
                    "pid": 4294967295u32
                }]
            })
            .to_string(),
        )
        .expect("write");

        let _ = crate::tools::shell::restore_shell_state_for_workspace(tmp.path(), true);

        let mut app = create_test_app(tmp.path().to_path_buf());
        let result = jobs(&mut app, Some("list"));
        let output = result.message.unwrap_or_default();
        assert!(output.contains("shell_test123"));
        assert!(output.contains("orphaned"));
    }
}
