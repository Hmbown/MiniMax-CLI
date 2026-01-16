//! Core engine for `MiniMax` CLI.
//!
//! The engine handles all AI interactions in a background task,
//! communicating with the UI via channels. This enables:
//! - Non-blocking UI during API calls
//! - Real-time streaming updates
//! - Proper cancellation support
//! - Tool execution orchestration

use std::path::PathBuf;
use std::pin::pin;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use futures_util::StreamExt;
use serde_json::json;
use tokio::sync::{Mutex as AsyncMutex, RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use crate::client::AnthropicClient;
use crate::config::Config;
use crate::duo::{DuoSession, SharedDuoSession, session_summary as duo_session_summary};
use crate::mcp::McpPool;
use crate::models::{
    ContentBlock, ContentBlockStart, Delta, Message, MessageRequest, StreamEvent, Tool, Usage,
};
use crate::prompts;
use crate::rlm::{RlmSession, SharedRlmSession, session_summary as rlm_session_summary};
use crate::tools::plan::PlanState;
use crate::tools::spec::{ApprovalLevel, ToolError, ToolResult};
use crate::tools::subagent::{
    SharedSubAgentManager, SubAgentRuntime, SubAgentType, new_shared_subagent_manager,
};
use crate::tools::todo::TodoList;
use crate::tools::{ToolContext, ToolRegistryBuilder};
use crate::tui::app::AppMode;

use super::events::Event;
use super::ops::Op;
use super::session::Session;
use super::tool_parser;
use super::turn::{TurnContext, TurnToolCall};

// === Types ===

/// Configuration for the engine
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Model identifier to use for responses.
    pub model: String,
    /// Workspace root for tool execution and file operations.
    pub workspace: PathBuf,
    /// Allow shell tool execution when true.
    pub allow_shell: bool,
    /// Enable trust mode (skip approvals) when true.
    pub trust_mode: bool,
    /// Path to the notes file used by the notes tool.
    pub notes_path: PathBuf,
    /// Path to the MCP configuration file.
    pub mcp_config_path: PathBuf,
    /// Maximum number of assistant steps before stopping.
    pub max_steps: u32,
    /// Maximum number of concurrently active subagents.
    pub max_subagents: usize,
    /// Shared RLM session state.
    pub rlm_session: SharedRlmSession,
    /// Shared Duo session state.
    pub duo_session: SharedDuoSession,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            model: "MiniMax-M2.1".to_string(),
            workspace: PathBuf::from("."),
            allow_shell: false,
            trust_mode: false,
            notes_path: PathBuf::from("notes.txt"),
            mcp_config_path: PathBuf::from("mcp.json"),
            max_steps: 100,
            max_subagents: 5,
            rlm_session: Arc::new(Mutex::new(RlmSession::default())),
            duo_session: Arc::new(Mutex::new(DuoSession::new())),
        }
    }
}

/// Handle to communicate with the engine
#[derive(Clone)]
pub struct EngineHandle {
    /// Send operations to the engine
    pub tx_op: mpsc::Sender<Op>,
    /// Receive events from the engine
    pub rx_event: Arc<RwLock<mpsc::Receiver<Event>>>,
    /// Cancellation token for the current request
    cancel_token: CancellationToken,
    /// Send approval decisions to the engine
    tx_approval: mpsc::Sender<ApprovalDecision>,
}

impl EngineHandle {
    /// Send an operation to the engine
    pub async fn send(&self, op: Op) -> Result<()> {
        self.tx_op.send(op).await?;
        Ok(())
    }

    /// Cancel the current request
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }

    /// Check if a request is currently cancelled
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// Approve a pending tool call
    pub async fn approve_tool_call(&self, id: impl Into<String>) -> Result<()> {
        self.tx_approval
            .send(ApprovalDecision::Approved { id: id.into() })
            .await?;
        Ok(())
    }

    /// Deny a pending tool call
    pub async fn deny_tool_call(&self, id: impl Into<String>) -> Result<()> {
        self.tx_approval
            .send(ApprovalDecision::Denied { id: id.into() })
            .await?;
        Ok(())
    }
}

// === Engine ===

/// The core engine that processes operations and emits events
pub struct Engine {
    config: EngineConfig,
    anthropic_client: Option<AnthropicClient>,
    anthropic_client_error: Option<String>,
    session: Session,
    subagent_manager: SharedSubAgentManager,
    mcp_pool: Option<Arc<AsyncMutex<McpPool>>>,
    rx_op: mpsc::Receiver<Op>,
    rx_approval: mpsc::Receiver<ApprovalDecision>,
    tx_event: mpsc::Sender<Event>,
    cancel_token: CancellationToken,
}

#[derive(Debug, Clone)]
enum ApprovalDecision {
    Approved { id: String },
    Denied { id: String },
}

// === Internal stream helpers ===

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContentBlockKind {
    Text,
    Thinking,
    ToolUse,
}

#[derive(Debug, Clone)]
struct ToolUseState {
    id: String,
    name: String,
    input: serde_json::Value,
    input_buffer: String,
}

const TOOL_CALL_START_MARKERS: [&str; 5] = [
    "[TOOL_CALL]",
    "<minimax:tool_call",
    "<tool_call",
    "<invoke ",
    "<function_calls>",
];
const TOOL_CALL_END_MARKERS: [&str; 5] = [
    "[/TOOL_CALL]",
    "</minimax:tool_call>",
    "</tool_call>",
    "</invoke>",
    "</function_calls>",
];

fn find_first_marker(text: &str, markers: &[&str]) -> Option<(usize, usize)> {
    markers
        .iter()
        .filter_map(|marker| text.find(marker).map(|idx| (idx, marker.len())))
        .min_by_key(|(idx, _)| *idx)
}

fn filter_tool_call_delta(delta: &str, in_tool_call: &mut bool) -> String {
    if delta.is_empty() {
        return String::new();
    }

    let mut output = String::new();
    let mut rest = delta;

    loop {
        if *in_tool_call {
            let Some((idx, len)) = find_first_marker(rest, &TOOL_CALL_END_MARKERS) else {
                break;
            };
            rest = &rest[idx + len..];
            *in_tool_call = false;
        } else {
            let Some((idx, len)) = find_first_marker(rest, &TOOL_CALL_START_MARKERS) else {
                output.push_str(rest);
                break;
            };
            output.push_str(&rest[..idx]);
            rest = &rest[idx + len..];
            *in_tool_call = true;
        }
    }

    output
}

fn parse_tool_input(buffer: &str) -> Option<serde_json::Value> {
    let trimmed = buffer.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Some(value);
    }
    if let Some(stripped) = strip_code_fences(trimmed)
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&stripped)
    {
        return Some(value);
    }
    if let Ok(serde_json::Value::String(inner)) = serde_json::from_str::<serde_json::Value>(trimmed)
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&inner)
    {
        return Some(value);
    }
    extract_json_segment(trimmed)
        .and_then(|segment| serde_json::from_str::<serde_json::Value>(&segment).ok())
}

fn strip_code_fences(text: &str) -> Option<String> {
    if !text.contains("```") {
        return None;
    }
    let mut lines = Vec::new();
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            continue;
        }
        lines.push(line);
    }
    let stripped = lines.join("\n");
    let stripped = stripped.trim();
    if stripped.is_empty() {
        None
    } else {
        Some(stripped.to_string())
    }
}

fn extract_json_segment(text: &str) -> Option<String> {
    extract_balanced_segment(text, '{', '}').or_else(|| extract_balanced_segment(text, '[', ']'))
}

fn extract_balanced_segment(text: &str, open: char, close: char) -> Option<String> {
    let start = text.find(open)?;
    let mut depth = 0i32;
    let mut end = None;
    for (offset, ch) in text[start..].char_indices() {
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                end = Some(start + offset + ch.len_utf8());
                break;
            }
        }
    }
    end.map(|end_idx| text[start..end_idx].to_string())
}

impl Engine {
    /// Create a new engine with the given configuration
    pub fn new(config: EngineConfig, api_config: &Config) -> (Self, EngineHandle) {
        let (tx_op, rx_op) = mpsc::channel(32);
        let (tx_event, rx_event) = mpsc::channel(256);
        let (tx_approval, rx_approval) = mpsc::channel(64);
        let cancel_token = CancellationToken::new();

        // Create clients for both providers
        let (anthropic_client, anthropic_client_error) = match AnthropicClient::new(api_config) {
            Ok(client) => (Some(client), None),
            Err(err) => (None, Some(err.to_string())),
        };

        let mut session = Session::new(
            config.model.clone(),
            config.workspace.clone(),
            config.allow_shell,
            config.trust_mode,
            config.notes_path.clone(),
            config.mcp_config_path.clone(),
        );

        // Set up system prompt with project context (default to agent mode)
        let system_prompt = prompts::system_prompt_for_mode_with_context(
            AppMode::Agent,
            &config.workspace,
            None,
            None,
        );
        session.system_prompt = Some(system_prompt);

        let subagent_manager =
            new_shared_subagent_manager(config.workspace.clone(), config.max_subagents);

        let engine = Engine {
            config,
            anthropic_client,
            anthropic_client_error,
            session,
            subagent_manager,
            mcp_pool: None,
            rx_op,
            rx_approval,
            tx_event,
            cancel_token: cancel_token.clone(),
        };

        let handle = EngineHandle {
            tx_op,
            rx_event: Arc::new(RwLock::new(rx_event)),
            cancel_token,
            tx_approval,
        };

        (engine, handle)
    }

    /// Run the engine event loop
    #[allow(clippy::too_many_lines)]
    pub async fn run(mut self) {
        while let Some(op) = self.rx_op.recv().await {
            match op {
                Op::SendMessage {
                    content,
                    mode,
                    model,
                    allow_shell,
                    trust_mode,
                } => {
                    self.handle_send_message(content, mode, model, allow_shell, trust_mode)
                        .await;
                }
                Op::CancelRequest => {
                    self.cancel_token.cancel();
                    // Create a new token for the next request
                    self.cancel_token = CancellationToken::new();
                }
                Op::ApproveToolCall { id } => {
                    // Tool approval handling will be implemented in tools module
                    let _ = self
                        .tx_event
                        .send(Event::status(format!("Approved tool call: {id}")))
                        .await;
                }
                Op::DenyToolCall { id } => {
                    let _ = self
                        .tx_event
                        .send(Event::status(format!("Denied tool call: {id}")))
                        .await;
                }
                Op::SpawnSubAgent { prompt } => {
                    let Some(client) = self.anthropic_client.clone() else {
                        let message = self
                            .anthropic_client_error
                            .as_deref()
                            .map(|err| format!("Failed to spawn sub-agent: {err}"))
                            .unwrap_or_else(|| {
                                "Failed to spawn sub-agent: API client not configured".to_string()
                            });
                        let _ = self.tx_event.send(Event::error(message, false)).await;
                        continue;
                    };

                    let runtime = SubAgentRuntime::new(
                        client,
                        self.session.model.clone(),
                        self.build_tool_context(),
                        self.session.allow_shell,
                        Some(self.tx_event.clone()),
                    );

                    let result = self
                        .subagent_manager
                        .lock()
                        .map_err(|_| anyhow::anyhow!("Failed to lock sub-agent manager"))
                        .and_then(|mut manager| {
                            manager.spawn_background(
                                Arc::clone(&self.subagent_manager),
                                runtime,
                                SubAgentType::General,
                                prompt.clone(),
                                None,
                            )
                        });

                    match result {
                        Ok(snapshot) => {
                            let _ = self
                                .tx_event
                                .send(Event::status(format!(
                                    "Spawned sub-agent {}",
                                    snapshot.agent_id
                                )))
                                .await;
                        }
                        Err(err) => {
                            let _ = self
                                .tx_event
                                .send(Event::error(
                                    format!("Failed to spawn sub-agent: {err}"),
                                    false,
                                ))
                                .await;
                        }
                    }
                }
                Op::ListSubAgents => {
                    let result = self
                        .subagent_manager
                        .lock()
                        .map(|manager| manager.list())
                        .map_err(|_| anyhow::anyhow!("Failed to lock sub-agent manager"));

                    match result {
                        Ok(agents) => {
                            let _ = self.tx_event.send(Event::AgentList { agents }).await;
                        }
                        Err(err) => {
                            let _ = self
                                .tx_event
                                .send(Event::error(
                                    format!("Failed to list sub-agents: {err}"),
                                    true,
                                ))
                                .await;
                        }
                    }
                }
                Op::ChangeMode { mode } => {
                    let _ = self
                        .tx_event
                        .send(Event::status(format!("Mode changed to: {mode:?}")))
                        .await;
                }
                Op::SetModel { model } => {
                    self.session.model = model;
                    self.config.model.clone_from(&self.session.model);
                    let _ = self
                        .tx_event
                        .send(Event::status(format!(
                            "Model set to: {}",
                            self.session.model
                        )))
                        .await;
                }
                Op::SyncSession {
                    messages,
                    system_prompt,
                    model,
                    workspace,
                } => {
                    self.session.messages = messages;
                    self.session.system_prompt = system_prompt;
                    self.session.model = model;
                    self.session.workspace = workspace.clone();
                    self.config.model.clone_from(&self.session.model);
                    self.config.workspace = workspace.clone();
                    let ctx = crate::project_context::load_project_context_with_parents(&workspace);
                    self.session.project_context = if ctx.has_instructions() {
                        Some(ctx)
                    } else {
                        None
                    };
                    let _ = self
                        .tx_event
                        .send(Event::status("Session context synced".to_string()))
                        .await;
                }
                Op::Shutdown => {
                    break;
                }
            }
        }
    }

    /// Handle a send message operation
    async fn handle_send_message(
        &mut self,
        content: String,
        mode: AppMode,
        model: String,
        allow_shell: bool,
        trust_mode: bool,
    ) {
        // Emit turn started event
        let _ = self.tx_event.send(Event::TurnStarted).await;

        // Check if we have the appropriate client
        if self.anthropic_client.is_none() {
            let message = self
                .anthropic_client_error
                .as_deref()
                .map(|err| format!("Failed to send message: {err}"))
                .unwrap_or_else(|| "Failed to send message: API client not configured".to_string());
            let _ = self.tx_event.send(Event::error(message, false)).await;
            return;
        }

        // Add user message to session
        let user_msg = Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: content,
                cache_control: None,
            }],
        };
        self.session.add_message(user_msg);

        // Create turn context
        let mut turn = TurnContext::new(self.config.max_steps);

        self.session.model = model;
        self.config.model.clone_from(&self.session.model);
        self.session.allow_shell = allow_shell;
        self.config.allow_shell = allow_shell;
        self.session.trust_mode = trust_mode;
        self.config.trust_mode = trust_mode;

        // Update system prompt to match the current mode
        let rlm_summary = if mode == AppMode::Rlm {
            self.config
                .rlm_session
                .lock()
                .ok()
                .map(|session| rlm_session_summary(&session))
        } else {
            None
        };
        let duo_summary = if mode == AppMode::Duo {
            self.config
                .duo_session
                .lock()
                .ok()
                .map(|s| duo_session_summary(&s))
        } else {
            None
        };
        self.session.system_prompt = Some(prompts::system_prompt_for_mode_with_context(
            mode,
            &self.config.workspace,
            rlm_summary.as_deref(),
            duo_summary.as_deref(),
        ));

        // Build tool registry and tool list for the current mode
        let todo_list = Arc::new(Mutex::new(TodoList::new()));
        let plan_state = Arc::new(Mutex::new(PlanState::default()));

        let tool_context = self.build_tool_context();
        let mut builder = ToolRegistryBuilder::new().with_full_agent_tools(
            self.session.allow_shell,
            todo_list.clone(),
            plan_state.clone(),
        );
        if mode == AppMode::Rlm {
            builder = builder.with_rlm_tools(
                self.config.rlm_session.clone(),
                self.anthropic_client.clone(),
                self.session.model.clone(),
            );
        }
        if mode == AppMode::Duo {
            builder = builder.with_duo_tools(self.config.duo_session.clone());
        }

        let tool_registry = match mode {
            AppMode::Agent | AppMode::Yolo | AppMode::Rlm | AppMode::Duo => {
                let runtime = if let Some(client) = self.anthropic_client.clone() {
                    Some(SubAgentRuntime::new(
                        client,
                        self.session.model.clone(),
                        tool_context.clone(),
                        self.session.allow_shell,
                        Some(self.tx_event.clone()),
                    ))
                } else {
                    None
                };
                Some(
                    builder
                        .with_subagent_tools(
                            self.subagent_manager.clone(),
                            runtime.expect("sub-agent runtime should exist with active client"),
                        )
                        .build(tool_context),
                )
            }
            _ => Some(builder.build(tool_context)),
        };

        let mcp_tools = self.mcp_tools().await;
        let tools = tool_registry.as_ref().map(|registry| {
            let mut tools = registry.to_api_tools();
            tools.extend(mcp_tools);
            tools
        });

        // Main turn loop
        self.handle_anthropic_turn(&mut turn, tool_registry.as_ref(), tools, mode)
            .await;

        // Update session usage
        self.session.total_usage.add(&turn.usage);

        // Emit turn complete event
        let _ = self
            .tx_event
            .send(Event::TurnComplete { usage: turn.usage })
            .await;
    }

    fn build_tool_context(&self) -> ToolContext {
        ToolContext::with_options(
            self.session.workspace.clone(),
            self.session.trust_mode,
            self.session.notes_path.clone(),
            self.session.mcp_config_path.clone(),
        )
    }

    async fn ensure_mcp_pool(&mut self) -> Result<Arc<AsyncMutex<McpPool>>, ToolError> {
        if let Some(pool) = self.mcp_pool.as_ref() {
            return Ok(Arc::clone(pool));
        }
        let pool = McpPool::from_config_path(&self.session.mcp_config_path)
            .map_err(|e| ToolError::execution_failed(format!("Failed to load MCP config: {e}")))?;
        let pool = Arc::new(AsyncMutex::new(pool));
        self.mcp_pool = Some(Arc::clone(&pool));
        Ok(pool)
    }

    async fn mcp_tools(&mut self) -> Vec<Tool> {
        let pool = match self.ensure_mcp_pool().await {
            Ok(pool) => pool,
            Err(err) => {
                let _ = self.tx_event.send(Event::status(err.to_string())).await;
                return Vec::new();
            }
        };

        let mut pool = pool.lock().await;
        let errors = pool.connect_all().await;
        for (server, err) in errors {
            let _ = self
                .tx_event
                .send(Event::status(format!(
                    "Failed to connect MCP server '{server}': {err}"
                )))
                .await;
        }

        pool.to_api_tools()
    }

    async fn execute_mcp_tool(
        &mut self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        let pool = self.ensure_mcp_pool().await?;
        let mut pool = pool.lock().await;
        let result = pool
            .call_tool(name, input)
            .await
            .map_err(|e| ToolError::execution_failed(format!("MCP tool failed: {e}")))?;
        let content = serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
        Ok(ToolResult::success(content))
    }

    async fn await_tool_approval(&mut self, tool_id: &str) -> Result<bool, ToolError> {
        loop {
            tokio::select! {
                _ = self.cancel_token.cancelled() => {
                    return Err(ToolError::execution_failed(
                        "Request cancelled while awaiting approval".to_string(),
                    ));
                }
                decision = self.rx_approval.recv() => {
                    let Some(decision) = decision else {
                        return Err(ToolError::execution_failed(
                            "Approval channel closed".to_string(),
                        ));
                    };
                    match decision {
                        ApprovalDecision::Approved { id } if id == tool_id => return Ok(true),
                        ApprovalDecision::Denied { id } if id == tool_id => return Ok(false),
                        _ => continue,
                    }
                }
            }
        }
    }

    /// Handle a turn using the Anthropic API (original implementation)
    #[allow(clippy::too_many_lines)]
    async fn handle_anthropic_turn(
        &mut self,
        turn: &mut TurnContext,
        tool_registry: Option<&crate::tools::ToolRegistry>,
        tools: Option<Vec<Tool>>,
        _mode: AppMode,
    ) {
        let client = self
            .anthropic_client
            .clone()
            .expect("anthropic client should be configured");

        loop {
            if self.cancel_token.is_cancelled() {
                let _ = self.tx_event.send(Event::status("Request cancelled")).await;
                break;
            }

            if turn.at_max_steps() {
                let _ = self
                    .tx_event
                    .send(Event::status("Reached maximum steps"))
                    .await;
                break;
            }

            // Build the request
            let request = MessageRequest {
                model: self.session.model.clone(),
                messages: self.session.messages.clone(),
                max_tokens: 4096,
                system: self.session.system_prompt.clone(),
                tools: tools.clone(),
                tool_choice: if tools.is_some() {
                    Some(json!({ "type": "auto" }))
                } else {
                    None
                },
                metadata: None,
                thinking: None,
                stream: Some(true),
                temperature: None,
                top_p: None,
            };

            // Stream the response
            let stream_result = client.create_message_stream(request).await;
            let stream = match stream_result {
                Ok(s) => s,
                Err(e) => {
                    let _ = self.tx_event.send(Event::error(e.to_string(), true)).await;
                    break;
                }
            };
            let mut stream = pin!(stream);

            // Track content blocks
            let mut content_blocks: Vec<ContentBlock> = Vec::new();
            let mut current_text_raw = String::new();
            let mut current_text_visible = String::new();
            let mut current_thinking = String::new();
            let mut tool_uses: Vec<ToolUseState> = Vec::new();
            let mut usage = Usage {
                input_tokens: 0,
                output_tokens: 0,
            };
            let mut current_block_kind: Option<ContentBlockKind> = None;
            let mut current_tool_index: Option<usize> = None;
            let mut in_tool_call_block = false;
            let mut pending_message_complete = false;
            let mut last_text_index: Option<usize> = None;
            let mut stream_errors = 0u32;

            // Process stream events
            while let Some(event_result) = stream.next().await {
                if self.cancel_token.is_cancelled() {
                    break;
                }

                let event = match event_result {
                    Ok(e) => e,
                    Err(e) => {
                        stream_errors = stream_errors.saturating_add(1);
                        let _ = self.tx_event.send(Event::error(e.to_string(), true)).await;
                        if stream_errors >= 3 {
                            break;
                        }
                        continue;
                    }
                };

                match event {
                    StreamEvent::MessageStart { message } => {
                        usage = message.usage;
                    }
                    StreamEvent::ContentBlockStart {
                        index,
                        content_block,
                    } => match content_block {
                        ContentBlockStart::Text { text } => {
                            current_text_raw = text;
                            current_text_visible.clear();
                            in_tool_call_block = false;
                            let filtered =
                                filter_tool_call_delta(&current_text_raw, &mut in_tool_call_block);
                            current_text_visible.push_str(&filtered);
                            current_block_kind = Some(ContentBlockKind::Text);
                            last_text_index = Some(index as usize);
                            let _ = self
                                .tx_event
                                .send(Event::MessageStarted {
                                    index: index as usize,
                                })
                                .await;
                        }
                        ContentBlockStart::Thinking { thinking } => {
                            current_thinking = thinking;
                            current_block_kind = Some(ContentBlockKind::Thinking);
                            let _ = self
                                .tx_event
                                .send(Event::ThinkingStarted {
                                    index: index as usize,
                                })
                                .await;
                        }
                        ContentBlockStart::ToolUse { id, name, input } => {
                            current_block_kind = Some(ContentBlockKind::ToolUse);
                            current_tool_index = Some(tool_uses.len());
                            let _ = self
                                .tx_event
                                .send(Event::ToolCallStarted {
                                    id: id.clone(),
                                    name: name.clone(),
                                    input: json!({}),
                                })
                                .await;
                            tool_uses.push(ToolUseState {
                                id,
                                name,
                                input,
                                input_buffer: String::new(),
                            });
                        }
                    },
                    StreamEvent::ContentBlockDelta { index, delta } => match delta {
                        Delta::TextDelta { text } => {
                            current_text_raw.push_str(&text);
                            let filtered = filter_tool_call_delta(&text, &mut in_tool_call_block);
                            if !filtered.is_empty() {
                                current_text_visible.push_str(&filtered);
                                let _ = self
                                    .tx_event
                                    .send(Event::MessageDelta {
                                        index: index as usize,
                                        content: filtered,
                                    })
                                    .await;
                            }
                        }
                        Delta::ThinkingDelta { thinking } => {
                            current_thinking.push_str(&thinking);
                            if !thinking.is_empty() {
                                let _ = self
                                    .tx_event
                                    .send(Event::ThinkingDelta {
                                        index: index as usize,
                                        content: thinking,
                                    })
                                    .await;
                            }
                        }
                        Delta::InputJsonDelta { partial_json } => {
                            if let Some(index) = current_tool_index
                                && let Some(tool_state) = tool_uses.get_mut(index)
                            {
                                tool_state.input_buffer.push_str(&partial_json);
                                if let Some(value) = parse_tool_input(&tool_state.input_buffer) {
                                    tool_state.input = value;
                                }
                            }
                        }
                    },
                    StreamEvent::ContentBlockStop { index } => {
                        let stopped_kind = current_block_kind.take();
                        match stopped_kind {
                            Some(ContentBlockKind::Text) => {
                                pending_message_complete = true;
                                last_text_index = Some(index as usize);
                            }
                            Some(ContentBlockKind::Thinking) => {
                                let _ = self
                                    .tx_event
                                    .send(Event::ThinkingComplete {
                                        index: index as usize,
                                    })
                                    .await;
                            }
                            Some(ContentBlockKind::ToolUse) | None => {}
                        }
                        if matches!(stopped_kind, Some(ContentBlockKind::ToolUse))
                            && let Some(index) = current_tool_index.take()
                            && let Some(tool_state) = tool_uses.get_mut(index)
                            && !tool_state.input_buffer.trim().is_empty()
                            && let Some(value) = parse_tool_input(&tool_state.input_buffer)
                        {
                            tool_state.input = value;
                        }
                    }
                    StreamEvent::MessageDelta {
                        usage: delta_usage, ..
                    } => {
                        if let Some(u) = delta_usage {
                            usage = u;
                        }
                    }
                    StreamEvent::MessageStop | StreamEvent::Ping => {}
                }
            }

            // Update turn usage
            turn.add_usage(&usage);

            // Build content blocks
            if !current_thinking.is_empty() {
                content_blocks.push(ContentBlock::Thinking {
                    thinking: current_thinking.clone(),
                });
            }
            let mut final_text = current_text_visible.clone();
            if tool_uses.is_empty() && tool_parser::has_tool_call_markers(&current_text_raw) {
                let parsed = tool_parser::parse_tool_calls(&current_text_raw);
                final_text = parsed.clean_text;
                for call in parsed.tool_calls {
                    let _ = self
                        .tx_event
                        .send(Event::ToolCallStarted {
                            id: call.id.clone(),
                            name: call.name.clone(),
                            input: call.args.clone(),
                        })
                        .await;
                    tool_uses.push(ToolUseState {
                        id: call.id,
                        name: call.name,
                        input: call.args,
                        input_buffer: String::new(),
                    });
                }
            }

            if !final_text.is_empty() {
                content_blocks.push(ContentBlock::Text {
                    text: final_text,
                    cache_control: None,
                });
            }
            for tool in &tool_uses {
                content_blocks.push(ContentBlock::ToolUse {
                    id: tool.id.clone(),
                    name: tool.name.clone(),
                    input: tool.input.clone(),
                });
            }

            if pending_message_complete {
                let index = last_text_index.unwrap_or(0);
                let _ = self.tx_event.send(Event::MessageComplete { index }).await;
            }

            // Add assistant message to session
            if !content_blocks.is_empty() {
                self.session.add_message(Message {
                    role: "assistant".to_string(),
                    content: content_blocks,
                });
            }

            // If no tool uses, we're done
            if tool_uses.is_empty() {
                break;
            }

            // Execute tools
            for tool in &tool_uses {
                let tool_id = &tool.id;
                let tool_name = &tool.name;
                let tool_input = tool.input.clone();
                let start = Instant::now();

                let mut approval_required = false;
                let mut approval_description = "Tool execution requires approval".to_string();
                if let Some(registry) = tool_registry
                    && let Some(spec) = registry.get(tool_name)
                {
                    approval_required = spec.approval_level() != ApprovalLevel::Auto;
                    approval_description = spec.description().to_string();
                }

                let result = if approval_required {
                    let _ = self
                        .tx_event
                        .send(Event::ApprovalRequired {
                            id: tool_id.clone(),
                            tool_name: tool_name.clone(),
                            description: approval_description,
                        })
                        .await;

                    match self.await_tool_approval(tool_id).await {
                        Ok(true) => {
                            if McpPool::is_mcp_tool(tool_name) {
                                self.execute_mcp_tool(tool_name, tool_input.clone()).await
                            } else if let Some(registry) = tool_registry {
                                registry.execute_full(tool_name, tool_input.clone()).await
                            } else {
                                Err(ToolError::not_available(format!(
                                    "tool '{tool_name}' is not registered"
                                )))
                            }
                        }
                        Ok(false) => Err(ToolError::permission_denied(format!(
                            "Tool '{tool_name}' denied by user"
                        ))),
                        Err(err) => Err(err),
                    }
                } else if McpPool::is_mcp_tool(tool_name) {
                    self.execute_mcp_tool(tool_name, tool_input.clone()).await
                } else if let Some(registry) = tool_registry {
                    registry.execute_full(tool_name, tool_input.clone()).await
                } else {
                    Err(ToolError::not_available(format!(
                        "tool '{tool_name}' is not registered"
                    )))
                };
                let duration = start.elapsed();

                let mut tool_call =
                    TurnToolCall::new(tool_id.clone(), tool_name.clone(), tool_input.clone());

                match result {
                    Ok(output) => {
                        tool_call.set_result(output.content.clone(), duration);
                        let _ = self
                            .tx_event
                            .send(Event::ToolCallComplete {
                                id: tool_id.clone(),
                                name: tool_name.clone(),
                                result: Ok(output.clone()),
                            })
                            .await;

                        self.session.add_message(Message {
                            role: "user".to_string(),
                            content: vec![ContentBlock::ToolResult {
                                tool_use_id: tool_id.clone(),
                                content: output.content,
                            }],
                        });
                    }
                    Err(e) => {
                        let error = e.to_string();
                        tool_call.set_error(error.clone(), duration);
                        let _ = self
                            .tx_event
                            .send(Event::ToolCallComplete {
                                id: tool_id.clone(),
                                name: tool_name.clone(),
                                result: Err(e),
                            })
                            .await;

                        self.session.add_message(Message {
                            role: "user".to_string(),
                            content: vec![ContentBlock::ToolResult {
                                tool_use_id: tool_id.clone(),
                                content: format!("Error: {error}"),
                            }],
                        });
                    }
                }

                turn.record_tool_call(tool_call);
            }

            turn.next_step();
        }
    }

    /// Get a reference to the session
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Get a mutable reference to the session
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }
}

/// Spawn the engine in a background task
pub fn spawn_engine(config: EngineConfig, api_config: &Config) -> EngineHandle {
    let (engine, handle) = Engine::new(config, api_config);

    tokio::spawn(async move {
        engine.run().await;
    });

    handle
}
