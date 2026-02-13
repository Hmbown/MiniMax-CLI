//! Events emitted by the core engine to the UI.
//!
//! These events flow from the engine to the TUI via a channel,
//! enabling non-blocking, real-time updates.

use serde_json::Value;

use crate::error_hints::{ErrorHint, is_recoverable};
use crate::models::{Message, SystemPrompt, Usage};
use crate::tools::spec::{ToolError, ToolResult};
use crate::tools::subagent::SubAgentResult;

/// Outcome status for a completed turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOutcomeStatus {
    Completed,
    Interrupted,
    Failed,
}

/// Events emitted by the engine to update the UI.
#[derive(Debug, Clone)]
pub enum Event {
    // === Streaming Events ===
    /// A new message block has started
    MessageStarted { index: usize },

    /// Incremental text content delta
    MessageDelta { index: usize, content: String },

    /// Message block completed
    MessageComplete { index: usize },

    /// Thinking block started
    ThinkingStarted { index: usize },

    /// Incremental thinking content delta
    ThinkingDelta { index: usize, content: String },

    /// Thinking block completed
    ThinkingComplete { index: usize },

    // === Tool Events ===
    /// Tool call initiated
    ToolCallStarted {
        id: String,
        name: String,
        input: Value,
    },

    /// Tool execution progress (for long-running tools)
    ToolCallProgress { id: String, output: String },

    /// Tool call completed
    ToolCallComplete {
        id: String,
        name: String,
        result: Result<ToolResult, ToolError>,
    },

    // === Turn Lifecycle ===
    /// A new turn has started (user sent a message)
    TurnStarted,

    /// The turn is complete (no more tool calls)
    TurnComplete {
        usage: Usage,
        status: TurnOutcomeStatus,
        error: Option<String>,
    },

    // === Sub-Agent Events (for RLM mode) ===
    /// A sub-agent has been spawned
    AgentSpawned { id: String, prompt: String },

    /// Sub-agent progress update
    AgentProgress { id: String, status: String },

    /// Sub-agent completed
    AgentComplete { id: String, result: String },

    /// Sub-agent listing
    AgentList { agents: Vec<SubAgentResult> },

    // === System Events ===
    /// An error occurred
    Error {
        message: String,
        recoverable: bool,
        hint: Option<ErrorHint>,
    },

    /// Status message for UI display
    Status { message: String },

    /// Engine session messages/system prompt were updated (e.g., compaction)
    SessionUpdated {
        messages: Vec<Message>,
        system_prompt: Option<SystemPrompt>,
    },

    /// Pause terminal input events (for interactive subprocesses)
    PauseEvents,

    /// Resume terminal input events after subprocess completion
    ResumeEvents,

    /// Request user approval for a tool call
    ApprovalRequired {
        id: String,
        tool_name: String,
        description: String,
    },

    /// Auto/manual context compaction started.
    CompactionStarted {
        id: String,
        auto: bool,
        message: String,
    },

    /// Context compaction completed.
    CompactionCompleted {
        id: String,
        auto: bool,
        message: String,
    },

    /// Context compaction failed.
    CompactionFailed {
        id: String,
        auto: bool,
        message: String,
    },

    /// A tool required elevated sandbox permissions.
    ElevationRequired {
        tool_id: String,
        tool_name: String,
        denial_reason: String,
        command: Option<String>,
    },
}

impl Event {
    /// Create a new error event
    pub fn error(message: impl Into<String>, recoverable: bool) -> Self {
        let message = message.into();
        let recoverable = recoverable || is_recoverable(&message);
        let hint = crate::error_hints::get_error_hint(&message);
        Event::Error {
            message,
            recoverable,
            hint,
        }
    }

    /// Create a new error event with a pre-computed hint
    pub fn error_with_hint(
        message: impl Into<String>,
        recoverable: bool,
        hint: Option<ErrorHint>,
    ) -> Self {
        Event::Error {
            message: message.into(),
            recoverable,
            hint,
        }
    }

    /// Create a new status event
    pub fn status(message: impl Into<String>) -> Self {
        Event::Status {
            message: message.into(),
        }
    }
}
