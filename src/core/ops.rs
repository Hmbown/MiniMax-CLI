//! Operations submitted by the UI to the core engine.
//!
//! These operations flow from the TUI to the engine via a channel,
//! allowing the UI to remain responsive while the engine processes requests.

use crate::compaction::CompactionConfig;
use crate::models::{Message, SystemPrompt};
use crate::tui::app::AppMode;
use std::path::PathBuf;

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

    /// Steer an active turn with additional guidance.
    Steer { content: String },

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

    /// Get one sub-agent status/result, optionally waiting for completion.
    GetSubAgent {
        agent_id: String,
        block: bool,
        timeout_ms: u64,
    },

    /// Cancel a running sub-agent.
    CancelSubAgent { agent_id: String },

    /// Clean completed sub-agents older than max_age_ms.
    CleanSubAgents { max_age_ms: u64 },

    /// Change the operating mode
    ChangeMode { mode: AppMode },

    /// Update the model being used
    SetModel { model: String },

    /// Sync engine session state (used for resume/load)
    SyncSession {
        messages: Vec<Message>,
        system_prompt: Option<SystemPrompt>,
        model: String,
        workspace: PathBuf,
    },

    /// Reload persisted runtime lifecycle state (jobs/subagents).
    ReloadRuntimeState,

    /// Shutdown the engine
    Shutdown,

    /// Trigger manual context compaction
    CompactContext,

    /// Enable or disable automatic context compaction.
    SetAutoCompact { enabled: bool },

    /// Update compaction thresholds/prompt.
    SetCompactionConfig { config: CompactionConfig },
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
