//! Tool approval system for `MiniMax` CLI
//!
//! Provides types and overlay widget for requesting user approval before
//! executing tools that may have costs or side effects.

#![allow(dead_code)]

use crate::pricing::CostEstimate;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use serde_json::Value;
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// Determines when tool executions require user approval
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApprovalMode {
    /// Auto-approve all tools (Agent mode / --yolo flag)
    Auto,
    /// Suggest approval for non-safe tools (Normal/Edit/Plan modes)
    #[default]
    Suggest,
    /// Never execute tools requiring approval
    #[allow(dead_code)]
    Never,
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
    pub _id: String,
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
            _id: id.to_string(),
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

/// Check if a tool requires approval in the given mode
pub fn requires_approval(
    tool_name: &str,
    mode: ApprovalMode,
    session_approved: &HashSet<String>,
) -> bool {
    match mode {
        ApprovalMode::Auto => false,
        ApprovalMode::Never => {
            let category = get_tool_category(tool_name);
            category != ToolCategory::Safe
        }
        ApprovalMode::Suggest => {
            // Already approved for session?
            if session_approved.contains(tool_name) {
                return false;
            }

            let category = get_tool_category(tool_name);
            match category {
                ToolCategory::Safe => false,
                ToolCategory::FileWrite | ToolCategory::Shell | ToolCategory::PaidMultimedia => {
                    true
                }
            }
        }
    }
}

/// Approval overlay state held in App
#[derive(Debug, Clone, Default)]
pub struct ApprovalState {
    /// Pending approval request
    pub pending: Option<ApprovalRequest>,
    /// Currently visible
    pub visible: bool,
    /// Selected option index (0-3)
    pub selected: usize,
    /// Optional timeout for approval requests
    pub timeout: Option<Duration>,
    /// When the current approval request started
    pub requested_at: Option<Instant>,
    /// Tools approved for this session
    pub session_approved: HashSet<String>,
}

impl ApprovalState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Show approval overlay for a request
    pub fn request(&mut self, req: ApprovalRequest) {
        self.pending = Some(req);
        self.visible = true;
        self.selected = 0;
        self.requested_at = Some(Instant::now());
    }

    /// Clear the overlay
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.pending = None;
        self.visible = false;
        self.selected = 0;
        self.requested_at = None;
    }

    /// Navigate selection up
    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Navigate selection down
    pub fn select_next(&mut self) {
        self.selected = (self.selected + 1).min(3);
    }

    /// Get the decision for the current selection
    pub fn current_decision(&self) -> ReviewDecision {
        match self.selected {
            0 => ReviewDecision::Approved,
            1 => ReviewDecision::ApprovedForSession,
            2 => ReviewDecision::Denied,
            _ => ReviewDecision::Abort,
        }
    }

    /// Apply a decision and return it
    pub fn apply_decision(&mut self, decision: ReviewDecision) -> Option<(String, ReviewDecision)> {
        let req = self.pending.take()?;
        let tool_name = req.tool_name.clone();

        if decision == ReviewDecision::ApprovedForSession {
            self.session_approved.insert(tool_name.clone());
        }

        self.visible = false;
        self.selected = 0;
        self.requested_at = None;

        Some((tool_name, decision))
    }

    pub fn is_timed_out(&self) -> bool {
        match (self.requested_at, self.timeout) {
            (Some(start), Some(timeout)) => start.elapsed() >= timeout,
            _ => false,
        }
    }
}

/// Render the approval overlay
pub fn render_approval_overlay(f: &mut Frame, state: &ApprovalState) {
    let Some(request) = &state.pending else {
        return;
    };

    let area = f.area();

    // Calculate popup dimensions
    let popup_width = 65.min(area.width.saturating_sub(4));
    let popup_height = 18.min(area.height.saturating_sub(4));
    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    // Clear background
    f.render_widget(Clear, popup_area);

    // Build content lines
    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  Tool: "),
            Span::styled(
                &request.tool_name,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    // Category indicator
    let category_label = match request.category {
        ToolCategory::Safe => ("Safe", Color::Green),
        ToolCategory::FileWrite => ("File Write", Color::Yellow),
        ToolCategory::Shell => ("Shell Command", Color::Red),
        ToolCategory::PaidMultimedia => ("Paid API", Color::Magenta),
    };
    lines.push(Line::from(vec![
        Span::raw("  Type: "),
        Span::styled(
            category_label.0,
            Style::default()
                .fg(category_label.1)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    // Cost estimate
    if let Some(cost) = &request.estimated_cost {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw("  Cost: "),
            Span::styled(
                cost.display(),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            format!("  {}", &cost.breakdown),
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  No cost (free operation)",
            Style::default().fg(Color::DarkGray),
        )));
    }

    // Parameters (truncated)
    lines.push(Line::from(""));
    let params_str = request.params_display();
    let params_truncated = crate::utils::truncate_with_ellipsis(&params_str, 50, "...");
    lines.push(Line::from(Span::styled(
        format!("  Params: {params_truncated}"),
        Style::default().fg(Color::DarkGray),
    )));

    // Divider
    lines.push(Line::from(""));

    // Options
    let options = [
        ("y", "Approve (this time)"),
        ("a", "Approve for session"),
        ("n", "Deny"),
        ("Esc", "Abort turn"),
    ];

    for (i, (key, label)) in options.iter().enumerate() {
        let is_selected = i == state.selected;
        let style = if is_selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };

        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("[{key}] "), Style::default().fg(Color::Green)),
            Span::styled(*label, style),
        ]));
    }

    // Build block and paragraph
    let title = format!(" Approve Tool: {} ", &request.tool_name);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, popup_area);
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
