//! Git-aware tools: /diff, /commit, /pr helpers

use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_str, required_str,
};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::process::Command;

/// Run a git command and return output
fn run_git(args: &[&str], cwd: &std::path::Path) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("Failed to run git: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Check if git is available in the workspace
fn is_git_repo(cwd: &std::path::Path) -> bool {
    run_git(&["rev-parse", "--git-dir"], cwd).is_ok()
}

// === GitDiffTool ===

/// Tool for showing git diff
pub struct GitDiffTool;

#[async_trait]
impl ToolSpec for GitDiffTool {
    fn name(&self) -> &'static str {
        "git_diff"
    }

    fn description(&self) -> &'static str {
        "Show git diff for staged and unstaged changes. Use target='staged' for staged changes, 'unstaged' for unstaged, or 'all' for both."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "enum": ["staged", "unstaged", "all"],
                    "description": "Which changes to show (default: all)"
                },
                "path": {
                    "type": "string",
                    "description": "Optional specific file or directory to diff"
                }
            },
            "required": []
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        if !is_git_repo(&context.workspace) {
            return Ok(ToolResult::error("Not a git repository"));
        }

        let target = optional_str(&input, "target").unwrap_or("all");
        let path = optional_str(&input, "path");

        let output = match target.as_str() {
            "staged" => {
                let mut args = vec!["diff", "--cached"];
                if let Some(p) = path {
                    args.push(&p);
                }
                run_git(&args, &context.workspace)
            }
            "unstaged" => {
                let mut args = vec!["diff"];
                if let Some(p) = path {
                    args.push(&p);
                }
                run_git(&args, &context.workspace)
            }
            "all" | _ => {
                let mut output = String::new();
                
                // Unstaged changes
                let unstaged = if let Some(p) = &path {
                    run_git(&["diff", p], &context.workspace)
                } else {
                    run_git(&["diff"], &context.workspace)
                };
                if let Ok(diff) = unstaged {
                    if !diff.is_empty() {
                        output.push_str("=== Unstaged Changes ===\n");
                        output.push_str(&diff);
                        output.push('\n');
                    }
                }

                // Staged changes
                let staged = if let Some(p) = &path {
                    run_git(&["diff", "--cached", p], &context.workspace)
                } else {
                    run_git(&["diff", "--cached"], &context.workspace)
                };
                if let Ok(diff) = staged {
                    if !diff.is_empty() {
                        output.push_str("=== Staged Changes ===\n");
                        output.push_str(&diff);
                        output.push('\n');
                    }
                }

                if output.is_empty() {
                    Ok("No changes to display".to_string())
                } else {
                    Ok(output)
                }
            }
        };

        match output {
            Ok(content) => Ok(ToolResult::success(content)),
            Err(e) => Ok(ToolResult::error(format!("Git diff failed: {e}"))),
        }
    }
}

// === GitStatusTool ===

/// Tool for showing git status
pub struct GitStatusTool;

#[async_trait]
impl ToolSpec for GitStatusTool {
    fn name(&self) -> &'static str {
        "git_status"
    }

    fn description(&self) -> &'static str {
        "Show current git status (branch, modified files, staged changes)"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    async fn execute(&self, _input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        if !is_git_repo(&context.workspace) {
            return Ok(ToolResult::error("Not a git repository"));
        }

        let mut output = String::new();

        // Current branch
        match run_git(&["branch", "--show-current"], &context.workspace) {
            Ok(branch) => output.push_str(&format!("Branch: {}\n", branch.trim())),
            Err(e) => return Ok(ToolResult::error(format!("Failed to get branch: {e}"))),
        }

        // Status
        match run_git(&["status", "--short"], &context.workspace) {
            Ok(status) => {
                if status.trim().is_empty() {
                    output.push_str("Working tree clean\n");
                } else {
                    output.push_str("\nChanges:\n");
                    output.push_str(&status);
                }
            }
            Err(e) => return Ok(ToolResult::error(format!("Failed to get status: {e}"))),
        }

        // Recent commits
        match run_git(&["log", "--oneline", "-5"], &context.workspace) {
            Ok(log) => {
                output.push_str("\nRecent commits:\n");
                output.push_str(&log);
            }
            Err(_) => {} // Silently ignore
        }

        Ok(ToolResult::success(output))
    }
}

// === GitCommitTool ===

/// Tool for creating git commits
pub struct GitCommitTool;

#[async_trait]
impl ToolSpec for GitCommitTool {
    fn name(&self) -> &'static str {
        "git_commit"
    }

    fn description(&self) -> &'static str {
        "Create a git commit with the given message. Optionally stage all changes first."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Commit message"
                },
                "stage_all": {
                    "type": "boolean",
                    "description": "Stage all changes before committing (default: false)"
                }
            },
            "required": ["message"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::RequiresApproval]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Required
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        if !is_git_repo(&context.workspace) {
            return Ok(ToolResult::error("Not a git repository"));
        }

        let message = required_str(&input, "message")?;
        let stage_all = input.get("stage_all")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Stage all if requested
        if stage_all {
            if let Err(e) = run_git(&["add", "-A"], &context.workspace) {
                return Ok(ToolResult::error(format!("Failed to stage changes: {e}")));
            }
        }

        // Create commit
        match run_git(&["commit", "-m", message], &context.workspace) {
            Ok(output) => Ok(ToolResult::success(format!("Commit created:\n{}", output))),
            Err(e) => Ok(ToolResult::error(format!("Failed to create commit: {e}"))),
        }
    }
}

// === GitLogTool ===

/// Tool for viewing git log
pub struct GitLogTool;

#[async_trait]
impl ToolSpec for GitLogTool {
    fn name(&self) -> &'static str {
        "git_log"
    }

    fn description(&self) -> &'static str {
        "Show recent git commit history"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Number of commits to show (default: 10)"
                },
                "path": {
                    "type": "string",
                    "description": "Optional path to filter commits"
                }
            },
            "required": []
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        if !is_git_repo(&context.workspace) {
            return Ok(ToolResult::error("Not a git repository"));
        }

        let limit = input.get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(10)
            .max(1)
            .min(100);
        let path = optional_str(&input, "path");

        let mut args = vec!["log", "--oneline", &format!("-{}", limit)];
        if let Some(p) = path {
            args.push("--");
            args.push(&p);
        }

        match run_git(&args, &context.workspace) {
            Ok(output) => Ok(ToolResult::success(output)),
            Err(e) => Ok(ToolResult::error(format!("Failed to get log: {e}"))),
        }
    }
}

// === GitBranchTool ===

/// Tool for listing and managing branches
pub struct GitBranchTool;

#[async_trait]
impl ToolSpec for GitBranchTool {
    fn name(&self) -> &'static str {
        "git_branch"
    }

    fn description(&self) -> &'static str {
        "List git branches or create a new branch"
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "create": {
                    "type": "string",
                    "description": "Create a new branch with this name"
                }
            },
            "required": []
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::RequiresApproval]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Suggest
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        if !is_git_repo(&context.workspace) {
            return Ok(ToolResult::error("Not a git repository"));
        }

        if let Some(new_branch) = input.get("create").and_then(|v| v.as_str()) {
            // Create new branch
            match run_git(&["checkout", "-b", new_branch], &context.workspace) {
                Ok(output) => Ok(ToolResult::success(format!(
                    "Created and switched to branch '{}':\n{}",
                    new_branch, output
                ))),
                Err(e) => Ok(ToolResult::error(format!("Failed to create branch: {e}"))),
            }
        } else {
            // List branches
            match run_git(&["branch", "-a"], &context.workspace) {
                Ok(output) => Ok(ToolResult::success(output)),
                Err(e) => Ok(ToolResult::error(format!("Failed to list branches: {e}"))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_properties() {
        let diff_tool = GitDiffTool;
        assert_eq!(diff_tool.name(), "git_diff");
        assert!(diff_tool.is_read_only());

        let commit_tool = GitCommitTool;
        assert_eq!(commit_tool.name(), "git_commit");
        assert!(!commit_tool.is_read_only());
    }
}
