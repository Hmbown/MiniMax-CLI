//! Tool approval system for `MiniMax` CLI
//!
//! Provides types and overlay widget for requesting user approval before
//! executing tools that may have costs or side effects.

use crate::pricing::CostEstimate;
use crate::tui::views::{ModalKind, ModalView, ViewAction, ViewEvent};
use crate::tui::widgets::Renderable;
use crossterm::event::{KeyCode, KeyEvent};
use serde_json::Value;
use std::time::{Duration, Instant};

/// Determines when tool executions require user approval
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApprovalMode {
    /// Auto-approve all tools (YOLO mode / --yolo flag)
    Auto,
    /// Suggest approval for non-safe tools (Normal/Plan modes)
    #[default]
    Suggest,
    /// Never execute tools requiring approval
    Never,
}

impl ApprovalMode {
    pub fn label(self) -> &'static str {
        match self {
            ApprovalMode::Auto => "AUTO",
            ApprovalMode::Suggest => "SUGGEST",
            ApprovalMode::Never => "NEVER",
        }
    }
}

/// User's decision for a pending approval
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewDecision {
    /// Execute this tool once
    Approved,
    /// Approve and don't ask again for this tool type this session
    ApprovedForSession,
    /// Reject the tool execution
    Denied,
    /// Abort the entire turn
    Abort,
}

/// Categorizes tools by cost/risk level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCategory {
    /// Free, read-only operations (`list_dir`, `read_file`, todo_*)
    Safe,
    /// File modifications (`write_file`, `edit_file`)
    FileWrite,
    /// Shell execution (`exec_shell`)
    Shell,
    /// Paid multimedia APIs (`generate_image`, `tts`, etc.)
    PaidMultimedia,
}

/// Request for user approval of a tool execution
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    /// Unique ID for this tool use
    pub id: String,
    /// Tool being executed
    pub tool_name: String,
    /// Tool category
    pub category: ToolCategory,
    /// Tool parameters (for display)
    pub params: Value,
    /// Estimated cost (for paid tools)
    pub estimated_cost: Option<CostEstimate>,
}

impl ApprovalRequest {
    pub fn new(id: &str, tool_name: &str, params: &Value) -> Self {
        let category = get_tool_category(tool_name);
        let estimated_cost = crate::pricing::estimate_tool_cost(tool_name, params);

        Self {
            id: id.to_string(),
            tool_name: tool_name.to_string(),
            category,
            params: params.clone(),
            estimated_cost,
        }
    }

    /// Format parameters for display (truncated)
    pub fn params_display(&self) -> String {
        let truncated = truncate_params_value(&self.params, 200);
        serde_json::to_string(&truncated).unwrap_or_else(|_| truncated.to_string())
    }
}

/// Get the category for a tool by name
pub fn get_tool_category(name: &str) -> ToolCategory {
    if matches!(name, "write_file" | "edit_file" | "apply_patch") {
        ToolCategory::FileWrite
    } else if name == "exec_shell" {
        ToolCategory::Shell
    } else if matches!(
        name,
        "analyze_image"
            | "generate_image"
            | "generate_video"
            | "generate_music"
            | "tts"
            | "tts_async_create"
            | "tts_async_query"
            | "voice_clone"
            | "voice_list"
            | "voice_delete"
            | "voice_design"
            | "upload_file"
            | "list_files"
            | "retrieve_file"
            | "download_file"
            | "delete_file"
            | "query_video"
            | "generate_video_template"
            | "query_video_template"
    ) {
        ToolCategory::PaidMultimedia
    } else {
        // Default to safe (includes read/list/todo/note/update_plan and unknown tools)
        ToolCategory::Safe
    }
}

/// Approval overlay state managed by the modal view stack
#[derive(Debug, Clone)]
pub struct ApprovalView {
    request: ApprovalRequest,
    selected: usize,
    timeout: Option<Duration>,
    requested_at: Instant,
    expanded: bool,
}

impl ApprovalView {
    pub fn new(request: ApprovalRequest) -> Self {
        Self {
            request,
            selected: 0,
            timeout: None,
            requested_at: Instant::now(),
            expanded: false,
        }
    }

    fn toggle_expanded(&mut self) {
        self.expanded = !self.expanded;
    }

    /// Format parameters for display, respecting expanded state
    pub fn params_display_expanded(&self) -> String {
        if self.expanded {
            serde_json::to_string(&self.request.params)
                .unwrap_or_else(|_| self.request.params.to_string())
        } else {
            self.request.params_display()
        }
    }

    fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn select_next(&mut self) {
        self.selected = (self.selected + 1).min(3);
    }

    fn current_decision(&self) -> ReviewDecision {
        match self.selected {
            0 => ReviewDecision::Approved,
            1 => ReviewDecision::ApprovedForSession,
            2 => ReviewDecision::Denied,
            _ => ReviewDecision::Abort,
        }
    }

    fn emit_decision(&self, decision: ReviewDecision, timed_out: bool) -> ViewAction {
        ViewAction::EmitAndClose(ViewEvent::ApprovalDecision {
            tool_id: self.request.id.clone(),
            tool_name: self.request.tool_name.clone(),
            decision,
            timed_out,
        })
    }

    fn is_timed_out(&self) -> bool {
        match self.timeout {
            Some(timeout) => self.requested_at.elapsed() >= timeout,
            None => false,
        }
    }
}

impl ModalView for ApprovalView {
    fn kind(&self) -> ModalKind {
        ModalKind::Approval
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn handle_key(&mut self, key: KeyEvent) -> ViewAction {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_prev();
                ViewAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
                ViewAction::None
            }
            KeyCode::Enter => self.emit_decision(self.current_decision(), false),
            KeyCode::Char('y') => self.emit_decision(ReviewDecision::Approved, false),
            KeyCode::Char('a') => self.emit_decision(ReviewDecision::ApprovedForSession, false),
            KeyCode::Char('n') => self.emit_decision(ReviewDecision::Denied, false),
            KeyCode::Esc => self.emit_decision(ReviewDecision::Abort, false),
            KeyCode::Char('e')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.toggle_expanded();
                ViewAction::None
            }
            _ => ViewAction::None,
        }
    }

    fn render(&self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
        let params_display = self.params_display_expanded();
        let approval_widget = crate::tui::widgets::ApprovalWidget::with_expanded(
            &self.request,
            self.selected,
            &params_display,
        );
        approval_widget.render(area, buf);
    }

    fn tick(&mut self) -> ViewAction {
        if self.is_timed_out() {
            return self.emit_decision(ReviewDecision::Denied, true);
        }
        ViewAction::None
    }

    fn has_expandable_content(&self) -> bool {
        true
    }

    fn is_expanded(&self) -> bool {
        self.expanded
    }
}

fn truncate_params_value(value: &Value, max_len: usize) -> Value {
    match value {
        Value::Object(map) => {
            let truncated = map
                .iter()
                .map(|(key, val)| (key.clone(), truncate_params_value(val, max_len)))
                .collect();
            Value::Object(truncated)
        }
        Value::Array(items) => {
            let truncated_items = items
                .iter()
                .map(|val| truncate_params_value(val, max_len))
                .collect();
            Value::Array(truncated_items)
        }
        Value::String(text) => Value::String(truncate_string_value(text, max_len)),
        other => {
            let rendered = other.to_string();
            if rendered.chars().count() > max_len {
                Value::String(truncate_string_value(&rendered, max_len))
            } else {
                other.clone()
            }
        }
    }
}

fn truncate_string_value(value: &str, max_len: usize) -> String {
    if value.chars().count() <= max_len {
        return value.to_string();
    }
    let truncated: String = value.chars().take(max_len).collect();
    format!("{truncated}...")
}
