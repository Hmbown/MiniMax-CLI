//! Operations submitted by the UI to the core engine.
//!
//! These operations flow from the TUI to the engine via a channel,
//! allowing the UI to remain responsive while the engine processes requests.

use crate::tui::app::AppMode;

/// Operations that can be submitted to the engine.
#[derive(Debug, Clone)]
pub enum Op {
    /// Send a message to the AI
    SendMessage {
        content: String,
        mode: AppMode,
        model: String,
        allow_shell: bool,
        trust_mode: bool,
    },

    /// Cancel the current request
    CancelRequest,

    /// Approve a tool call that requires permission
    ApproveToolCall { id: String },

    /// Deny a tool call that requires permission
    DenyToolCall { id: String },

    /// Spawn a sub-agent (for RLM mode)
    SpawnSubAgent { prompt: String },

    /// List current sub-agents and their status
    ListSubAgents,

    /// Change the operating mode
    ChangeMode { mode: AppMode },

    /// Update the model being used
    SetModel { model: String },

    /// Shutdown the engine
    Shutdown,
}

impl Op {
    /// Create a send message operation
    pub fn send(
        content: impl Into<String>,
        mode: AppMode,
        model: impl Into<String>,
        allow_shell: bool,
        trust_mode: bool,
    ) -> Self {
        Op::SendMessage {
            content: content.into(),
            mode,
            model: model.into(),
            allow_shell,
            trust_mode,
        }
    }

    /// Create a cancel operation
    pub fn cancel() -> Self {
        Op::CancelRequest
    }
}
