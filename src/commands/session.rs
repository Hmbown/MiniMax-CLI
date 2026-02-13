//! Session commands: save, load, compact, export, reset

use std::fmt::Write;
use std::path::PathBuf;

use crate::session_manager::create_saved_session;
use crate::tui::app::App;
use crate::tui::history::{HistoryCell, history_cells_from_message};

use super::CommandResult;
use crate::tools::plan::PlanState;

/// Save session to file
pub fn save(app: &mut App, path: Option<&str>) -> CommandResult {
    let save_path = if let Some(p) = path {
        PathBuf::from(p)
    } else {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        PathBuf::from(format!("session_{timestamp}.json"))
    };

    let messages = app.api_messages.clone();
    let pinned_messages = app.pinned_messages.clone();
    let session = create_saved_session(
        &messages,
        &app.model,
        &app.workspace,
        u64::from(app.total_tokens),
        app.system_prompt.as_ref(),
        pinned_messages,
    );

    let sessions_dir = save_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| app.workspace.clone(), std::path::Path::to_path_buf);

    match std::fs::create_dir_all(&sessions_dir) {
        Ok(()) => {
            match std::fs::write(&save_path, serde_json::to_string_pretty(&session).unwrap()) {
                Ok(()) => {
                    app.current_session_id = Some(session.metadata.id.clone());
                    CommandResult::message(format!(
                        "Session saved to {} (ID: {})",
                        save_path.display(),
                        &session.metadata.id[..8]
                    ))
                }
                Err(e) => CommandResult::error(format!("Failed to save session: {e}")),
            }
        }
        Err(e) => CommandResult::error(format!("Failed to create directory: {e}")),
    }
}

/// Load session from file
pub fn load(app: &mut App, path: Option<&str>) -> CommandResult {
    let load_path = if let Some(p) = path {
        if p.contains('/') || p.contains('\\') {
            PathBuf::from(p)
        } else {
            app.workspace.join(p)
        }
    } else {
        return CommandResult::error("Usage: /load <path>");
    };

    let content = match std::fs::read_to_string(&load_path) {
        Ok(c) => c,
        Err(e) => {
            return CommandResult::error(format!("Failed to read session file: {e}"));
        }
    };

    let session: crate::session_manager::SavedSession = match serde_json::from_str(&content) {
        Ok(s) => s,
        Err(e) => {
            return CommandResult::error(format!("Failed to parse session file: {e}"));
        }
    };

    app.api_messages.clone_from(&session.messages);
    app.history.clear();
    for msg in &app.api_messages {
        app.history.extend(history_cells_from_message(msg));
    }
    app.mark_history_updated();
    app.transcript_selection.clear();
    app.model.clone_from(&session.metadata.model);
    app.workspace.clone_from(&session.metadata.workspace);
    app.total_tokens = u32::try_from(session.metadata.total_tokens).unwrap_or(u32::MAX);
    app.current_session_id = Some(session.metadata.id.clone());
    if let Some(sp) = session.system_prompt {
        app.system_prompt = Some(crate::models::SystemPrompt::Text(sp));
    }
    app.pinned_messages = session.pinned_messages;
    app.recalculate_context_tokens();
    app.scroll_to_bottom();

    CommandResult::with_message_and_action(
        format!(
            "Session loaded from {} (ID: {}, {} messages)",
            load_path.display(),
            &session.metadata.id[..8],
            session.metadata.message_count
        ),
        crate::tui::app::AppAction::SyncSession {
            messages: app.api_messages.clone(),
            system_prompt: app.system_prompt.clone(),
            model: app.model.clone(),
            workspace: app.workspace.clone(),
        },
    )
}

/// Toggle auto-compaction or trigger manual compaction
pub fn compact(app: &mut App, arg: Option<&str>) -> CommandResult {
    match arg {
        Some("now") => {
            // Trigger manual compaction via engine
            CommandResult::action(crate::tui::app::AppAction::CompactContext)
        }
        _ => {
            // Toggle auto-compact setting
            app.auto_compact = !app.auto_compact;
            CommandResult::with_message_and_action(
                format!(
                    "Auto-compact: {}\n\nTip: Use '/compact now' to trigger manual compaction",
                    if app.auto_compact { "ON" } else { "OFF" }
                ),
                crate::tui::app::AppAction::SetAutoCompact(app.auto_compact),
            )
        }
    }
}

/// Export conversation to markdown
pub fn export(app: &mut App, path: Option<&str>) -> CommandResult {
    let export_path = path.map_or_else(
        || {
            let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
            PathBuf::from(format!("chat_export_{timestamp}.md"))
        },
        PathBuf::from,
    );

    let mut content = String::new();
    content.push_str("# Chat Export\n\n");
    let _ = write!(
        content,
        "**Model:** {}\n**Workspace:** {}\n**Date:** {}\n\n---\n\n",
        app.model,
        app.workspace.display(),
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );

    for cell in &app.history {
        let (role, body) = match cell {
            HistoryCell::User { content } => ("**You:**", content.clone()),
            HistoryCell::Assistant { content, .. } => ("**Assistant:**", content.clone()),
            HistoryCell::System { content } => ("*System:*", content.clone()),
            HistoryCell::ThinkingSummary { summary } => ("*Thinking:*", summary.clone()),
            HistoryCell::Tool(tool) => ("**Tool:**", render_tool_cell(tool, 80)),
            HistoryCell::Error { message, .. } => ("*Error:*", message.clone()),
        };

        let _ = write!(content, "{}\n\n{}\n\n---\n\n", role, body.trim());
    }

    match std::fs::write(&export_path, content) {
        Ok(()) => CommandResult::message(format!("Exported to {}", export_path.display())),
        Err(e) => CommandResult::error(format!("Failed to export: {e}")),
    }
}

/// Hard reset of the session - clears everything to fresh state
pub fn reset(app: &mut App) -> CommandResult {
    // 1. Clear conversation history (like /clear)
    app.history.clear();
    app.mark_history_updated();
    app.api_messages.clear();
    app.transcript_selection.clear();
    app.total_conversation_tokens = 0;
    app.total_tokens = 0;

    // 2. Reset all todos
    app.clear_todos();

    // 3. Reset plan state
    if let Ok(mut plan) = app.plan_state.lock() {
        *plan = PlanState::default();
    }

    // 4. Clear any queued messages
    app.queued_messages.clear();
    app.queued_draft = None;

    // 4b. Clear any active suggestions
    app.suggestion_engine.clear();

    // 5. Reset context/compaction state
    app.session_cost = 0.0;
    app.tool_log.clear();
    app.last_prompt_tokens = None;
    app.last_completion_tokens = None;
    app.last_usage_at = None;

    // Clear reasoning and tool state
    app.reasoning_buffer.clear();
    app.reasoning_header = None;
    app.last_reasoning = None;
    app.pending_tool_uses.clear();
    app.tool_cells.clear();
    app.exploring_cell = None;
    app.exploring_entries.clear();
    app.ignored_tool_calls.clear();
    app.last_exec_wait_command = None;
    app.streaming_message_index = None;

    // Clear recent files and process state
    app.recent_files.clear();
    app.current_process = None;

    // Reset turn state
    app.turn_started_at = None;
    app.is_loading = false;

    // Clear pinned messages
    app.clear_pins();

    // 6. Sync cleared state back to engine/session runtime
    CommandResult::with_message_and_action(
        "Session reset - all history, todos, and state cleared",
        crate::tui::app::AppAction::SyncSession {
            messages: app.api_messages.clone(),
            system_prompt: app.system_prompt.clone(),
            model: app.model.clone(),
            workspace: app.workspace.clone(),
        },
    )
}

fn render_tool_cell(tool: &crate::tui::history::ToolCell, width: u16) -> String {
    tool.lines(width)
        .into_iter()
        .map(line_to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn line_to_string(line: ratatui::text::Line<'static>) -> String {
    line.spans
        .into_iter()
        .map(|span| span.content.to_string())
        .collect::<String>()
}
