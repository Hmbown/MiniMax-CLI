//! RLM commands for the TUI (load/status/repl/save-session).

use std::fs;
use std::path::{Path, PathBuf};

use crate::rlm::RlmSession;
use crate::tui::app::{App, AppMode};

use super::CommandResult;

pub fn welcome_message() -> String {
    [
        "MiniMax RLM Sandbox",
        "Type expressions directly in the input bar.",
        "",
        "Commands:",
        "  /load <path>         Load a file into context",
        "  /status              Show loaded contexts",
        "  /save-session <path> Save RLM session to file",
        "  /repl                Show this help again",
        "",
        "Expressions:",
        "  len(ctx)",
        "  search(\"pattern\")",
        "  lines(1, 20)",
        "  chunk(2000, 200)",
    ]
    .join("\n")
}

pub fn repl(app: &mut App) -> CommandResult {
    if app.mode != AppMode::Rlm {
        app.set_mode(AppMode::Rlm);
    }
    CommandResult::message(welcome_message())
}

pub fn status(app: &mut App) -> CommandResult {
    if app.rlm_session.contexts.is_empty() {
        return CommandResult::message("No RLM contexts loaded. Use /load <path>.");
    }

    let mut lines = Vec::new();
    lines.push("RLM Session".to_string());
    lines.push(format!(
        "Active context: {}",
        app.rlm_session.active_context
    ));
    lines.push(format!(
        "Loaded contexts: {}",
        app.rlm_session.contexts.len()
    ));

    let mut ids: Vec<_> = app.rlm_session.contexts.keys().collect();
    ids.sort();
    for id in ids {
        if let Some(ctx) = app.rlm_session.contexts.get(id) {
            let source = ctx
                .source_path
                .as_ref()
                .map(|s| format!(" (source: {s})"))
                .unwrap_or_default();
            lines.push(format!(
                "- {id}: {} lines, {} chars{source}",
                ctx.line_count, ctx.char_count
            ));
        }
    }

    CommandResult::message(lines.join("\n"))
}

pub fn load(app: &mut App, path: Option<&str>) -> CommandResult {
    let Some(raw) = path else {
        return CommandResult::error("Usage: /load <path>");
    };

    let resolved = match resolve_path(app, raw) {
        Ok(path) => path,
        Err(err) => return CommandResult::error(err),
    };

    let content = match fs::read_to_string(&resolved) {
        Ok(content) => content,
        Err(err) => {
            return CommandResult::error(format!(
                "Failed to read file {}: {err}",
                resolved.display()
            ));
        }
    };

    let base_id = context_id_from_path(&resolved);
    let id = unique_context_id(&app.rlm_session, base_id);
    let source = resolved.to_string_lossy().to_string();
    app.rlm_session.load_context(&id, content, Some(source));

    let ctx = app
        .rlm_session
        .contexts
        .get(&id)
        .expect("context should exist after load");

    CommandResult::message(format!(
        "Loaded {} ({} lines, {} chars)",
        resolved.display(),
        ctx.line_count,
        ctx.char_count
    ))
}

pub fn save_session(app: &mut App, path: Option<&str>) -> CommandResult {
    let save_path = if let Some(p) = path {
        PathBuf::from(p)
    } else {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        PathBuf::from(format!("rlm_session_{timestamp}.json"))
    };

    let parent_dir = save_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(std::path::Path::to_path_buf);
    if let Some(dir) = parent_dir {
        if let Err(err) = fs::create_dir_all(&dir) {
            return CommandResult::error(format!(
                "Failed to create directory {}: {err}",
                dir.display()
            ));
        }
    }

    let json = match serde_json::to_string_pretty(&app.rlm_session) {
        Ok(json) => json,
        Err(err) => return CommandResult::error(format!("Failed to serialize session: {err}")),
    };

    match fs::write(&save_path, json) {
        Ok(()) => CommandResult::message(format!("RLM session saved to {}", save_path.display())),
        Err(err) => CommandResult::error(format!("Failed to save session: {err}")),
    }
}

fn resolve_path(app: &App, raw: &str) -> Result<PathBuf, String> {
    let candidate = if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        app.workspace.join(raw)
    };
    let canonical = candidate
        .canonicalize()
        .map_err(|err| format!("Failed to resolve path {}: {err}", candidate.display()))?;
    if !app.trust_mode && !canonical.starts_with(&app.workspace) {
        return Err("Path is outside workspace. Use /trust to allow access.".to_string());
    }
    Ok(canonical)
}

fn context_id_from_path(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("context")
        .to_string()
}

fn unique_context_id(session: &RlmSession, base: String) -> String {
    if !session.contexts.contains_key(&base) {
        return base;
    }

    for idx in 2..=99 {
        let candidate = format!("{base}-{idx}");
        if !session.contexts.contains_key(&candidate) {
            return candidate;
        }
    }

    format!("{base}-{}", session.contexts.len() + 1)
}
