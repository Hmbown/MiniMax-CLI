//! TUI event loop and rendering logic for `MiniMax` CLI.

use std::fmt::Write;
use std::fs;
use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
        MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::commands;
use crate::config::Config;
use crate::core::engine::{EngineConfig, EngineHandle, spawn_engine};
use crate::core::events::Event as EngineEvent;
use crate::core::ops::Op;
use crate::hooks::HookEvent;
use crate::models::{ContentBlock, Message, SystemPrompt, context_window_for_model};
use crate::prompts;
use crate::rlm;
use crate::session_manager::{SessionManager, create_saved_session, update_session};
use crate::tools::spec::{ToolError, ToolResult};
use crate::tools::subagent::{SubAgentResult, SubAgentStatus};
use crate::tui::scrolling::{ScrollDirection, TranscriptScroll};
use crate::tui::selection::TranscriptSelectionPoint;
use crate::utils::estimate_message_chars;

use super::app::{App, AppAction, AppMode, OnboardingState, QueuedMessage, TuiOptions};
use super::approval::{ApprovalMode, ApprovalRequest, render_approval_overlay, requires_approval};
use super::history::{
    ExecCell, ExecSource, ExploringCell, ExploringEntry, GenericToolCell, HistoryCell, McpToolCell,
    PatchSummaryCell, PlanStep, PlanUpdateCell, ToolCell, ToolStatus, ViewImageCell, WebSearchCell,
    extract_reasoning_summary, history_cells_from_message, summarize_mcp_output,
    summarize_tool_args, summarize_tool_output,
};

// === Constants ===

const MINIMAX_RED: Color = Color::Rgb(220, 80, 80);
const MINIMAX_CORAL: Color = Color::Rgb(240, 128, 100);
const MINIMAX_ORANGE: Color = Color::Rgb(255, 165, 80);
const MAX_QUEUED_PREVIEW: usize = 3;
const AUTO_RLM_MIN_FILE_BYTES: u64 = 200_000;
const AUTO_RLM_HINT_FILE_BYTES: u64 = 50_000;
const AUTO_RLM_PASTE_MIN_CHARS: usize = 15_000;
const AUTO_RLM_PASTE_HINT_CHARS: usize = 5_000;
const AUTO_RLM_PASTE_QUERY_MAX_CHARS: usize = 800;
const AUTO_RLM_PASTE_FIRST_LINE_MAX_CHARS: usize = 200;
const RLM_BUDGET_WARN_QUERIES: u32 = 8;
const RLM_BUDGET_WARN_INPUT_TOKENS: u64 = 60_000;
const RLM_BUDGET_WARN_OUTPUT_TOKENS: u64 = 20_000;
const RLM_BUDGET_HARD_QUERIES: u32 = 16;
const RLM_BUDGET_HARD_INPUT_TOKENS: u64 = 120_000;
const RLM_BUDGET_HARD_OUTPUT_TOKENS: u64 = 40_000;
const AUTO_RLM_MAX_SCAN_ENTRIES: usize = 50_000;
const AUTO_RLM_EXCLUDED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".codex",
    ".aleph",
    "dist",
    "build",
];

// ASCII logo for onboarding screen only
const LOGO: &str = r"
 ███╗   ███╗██╗███╗   ██╗██╗███╗   ███╗ █████╗ ██╗  ██╗
 ████╗ ████║██║████╗  ██║██║████╗ ████║██╔══██╗╚██╗██╔╝
 ██╔████╔██║██║██╔██╗ ██║██║██╔████╔██║███████║ ╚███╔╝
 ██║╚██╔╝██║██║██║╚██╗██║██║██║╚██╔╝██║██╔══██║ ██╔██╗
 ██║ ╚═╝ ██║██║██║ ╚████║██║██║ ╚═╝ ██║██║  ██║██╔╝ ██╗
 ╚═╝     ╚═╝╚═╝╚═╝  ╚═══╝╚═╝╚═╝     ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝
";

/// Run the interactive TUI event loop.
///
/// # Examples
///
/// ```ignore
/// # use crate::config::Config;
/// # use crate::tui::TuiOptions;
/// # async fn example(config: &Config, options: TuiOptions) -> anyhow::Result<()> {
/// crate::tui::run_tui(config, options).await
/// # }
/// ```
pub async fn run_tui(config: &Config, options: TuiOptions) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(options.clone(), config);

    // Load existing session if resuming
    if let Some(ref session_id) = options.resume_session_id
        && let Ok(manager) = SessionManager::default_location()
    {
        // Try to load by prefix or full ID
        let load_result: std::io::Result<Option<crate::session_manager::SavedSession>> =
            if session_id == "latest" {
                // Special case: resume the most recent session
                match manager.get_latest_session() {
                    Ok(Some(meta)) => manager.load_session(&meta.id).map(Some),
                    Ok(None) => Ok(None),
                    Err(e) => Err(e),
                }
            } else {
                manager.load_session_by_prefix(session_id).map(Some)
            };

        match load_result {
            Ok(Some(saved)) => {
                app.api_messages.clone_from(&saved.messages);
                app.model.clone_from(&saved.metadata.model);
                app.workspace.clone_from(&saved.metadata.workspace);
                app.current_session_id = Some(saved.metadata.id.clone());
                app.total_tokens = u32::try_from(saved.metadata.total_tokens).unwrap_or(u32::MAX);
                app.total_conversation_tokens = app.total_tokens;
                if let Some(prompt) = saved.system_prompt {
                    app.system_prompt = Some(SystemPrompt::Text(prompt));
                }
                // Convert saved messages to HistoryCell format for display
                app.history.clear();
                app.history.push(HistoryCell::System {
                    content: format!(
                        "Resumed session: {} ({})",
                        saved.metadata.title,
                        &saved.metadata.id[..8]
                    ),
                });
                for msg in &saved.messages {
                    app.history.extend(history_cells_from_message(msg));
                }
                app.mark_history_updated();
                app.status_message = Some(format!("Resumed session: {}", &saved.metadata.id[..8]));
            }
            Ok(None) => {
                app.status_message = Some("No sessions found to resume".to_string());
            }
            Err(e) => {
                app.status_message = Some(format!("Failed to load session: {e}"));
            }
        }
    }

    // Create the Engine with configuration from TuiOptions
    let engine_config = EngineConfig {
        model: app.model.clone(),
        workspace: app.workspace.clone(),
        allow_shell: app.allow_shell,
        trust_mode: options.yolo,
        notes_path: config.notes_path(),
        mcp_config_path: config.mcp_config_path(),
        max_steps: 100,
        max_subagents: app.max_subagents,
        rlm_session: app.rlm_session.clone(),
    };

    // Spawn the Engine - it will handle all API communication
    let engine_handle = spawn_engine(engine_config, config);

    if !app.api_messages.is_empty() {
        let _ = engine_handle
            .send(Op::SyncSession {
                messages: app.api_messages.clone(),
                system_prompt: app.system_prompt.clone(),
                model: app.model.clone(),
                workspace: app.workspace.clone(),
            })
            .await;
    }

    // Fire session start hook
    {
        let context = app.base_hook_context();
        let _ = app.execute_hooks(HookEvent::SessionStart, &context);
    }

    let result = run_event_loop(&mut terminal, &mut app, config, engine_handle).await;

    // Fire session end hook
    {
        let context = app.base_hook_context();
        let _ = app.execute_hooks(HookEvent::SessionEnd, &context);
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableBracketedPaste,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

#[allow(clippy::too_many_lines)]
async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    _config: &Config,
    engine_handle: EngineHandle,
) -> Result<()> {
    // Track streaming state
    let mut current_streaming_text = String::new();

    loop {
        // First, poll for engine events (non-blocking)
        let mut queued_to_send: Option<QueuedMessage> = None;
        {
            let mut rx = engine_handle.rx_event.write().await;
            while let Ok(event) = rx.try_recv() {
                match event {
                    EngineEvent::MessageStarted { .. } => {
                        current_streaming_text.clear();
                        app.streaming_message_index = None;
                    }
                    EngineEvent::MessageDelta { content, .. } => {
                        current_streaming_text.push_str(&content);
                        let index = if let Some(index) = app.streaming_message_index {
                            index
                        } else {
                            app.add_message(HistoryCell::Assistant {
                                content: String::new(),
                                streaming: true,
                            });
                            let index = app.history.len().saturating_sub(1);
                            app.streaming_message_index = Some(index);
                            index
                        };

                        if let Some(HistoryCell::Assistant { content, .. }) =
                            app.history.get_mut(index)
                        {
                            content.clone_from(&current_streaming_text);
                            app.mark_history_updated();
                        }
                    }
                    EngineEvent::MessageComplete { .. } => {
                        if let Some(index) = app.streaming_message_index.take()
                            && let Some(HistoryCell::Assistant { streaming, .. }) =
                                app.history.get_mut(index)
                        {
                            *streaming = false;
                            app.mark_history_updated();
                        }

                        if !current_streaming_text.is_empty()
                            || app.last_reasoning.is_some()
                            || !app.pending_tool_uses.is_empty()
                        {
                            let mut blocks = Vec::new();
                            if let Some(thinking) = app.last_reasoning.take() {
                                blocks.push(ContentBlock::Thinking { thinking });
                            }
                            if !current_streaming_text.is_empty() {
                                blocks.push(ContentBlock::Text {
                                    text: current_streaming_text.clone(),
                                    cache_control: None,
                                });
                            }
                            for (id, name, input) in app.pending_tool_uses.drain(..) {
                                blocks.push(ContentBlock::ToolUse { id, name, input });
                            }
                            if !blocks.is_empty() {
                                app.api_messages.push(Message {
                                    role: "assistant".to_string(),
                                    content: blocks,
                                });
                            }
                        }
                    }
                    EngineEvent::ThinkingStarted { .. } => {
                        app.reasoning_buffer.clear();
                        app.reasoning_header = None;
                    }
                    EngineEvent::ThinkingDelta { content, .. } => {
                        app.reasoning_buffer.push_str(&content);
                        if app.reasoning_header.is_none() {
                            app.reasoning_header = extract_reasoning_header(&app.reasoning_buffer);
                        }
                    }
                    EngineEvent::ThinkingComplete { .. } => {
                        if let Some(summary) = extract_reasoning_summary(&app.reasoning_buffer) {
                            app.add_message(HistoryCell::ThinkingSummary { summary });
                        }
                        if !app.reasoning_buffer.is_empty() {
                            app.last_reasoning = Some(app.reasoning_buffer.clone());
                        }
                        app.reasoning_buffer.clear();
                    }
                    EngineEvent::ToolCallStarted { id, name, input } => {
                        app.pending_tool_uses
                            .push((id.clone(), name.clone(), input.clone()));
                        handle_tool_call_started(app, &id, &name, &input);
                    }
                    EngineEvent::ToolCallComplete { id, name, result } => {
                        let tool_content = match &result {
                            Ok(output) => output.content.clone(),
                            Err(err) => format!("Error: {err}"),
                        };
                        app.api_messages.push(Message {
                            role: "user".to_string(),
                            content: vec![ContentBlock::ToolResult {
                                tool_use_id: id.clone(),
                                content: tool_content,
                            }],
                        });
                        handle_tool_call_complete(app, &id, &name, &result);
                    }
                    EngineEvent::TurnStarted => {
                        app.is_loading = true;
                        current_streaming_text.clear();
                        app.turn_started_at = Some(Instant::now());
                        app.reasoning_buffer.clear();
                        app.reasoning_header = None;
                        app.last_reasoning = None;
                        app.pending_tool_uses.clear();
                    }
                    EngineEvent::TurnComplete { usage } => {
                        app.is_loading = false;
                        app.turn_started_at = None;
                        let turn_tokens = usage.input_tokens + usage.output_tokens;
                        app.total_tokens = app.total_tokens.saturating_add(turn_tokens);
                        app.total_conversation_tokens =
                            app.total_conversation_tokens.saturating_add(turn_tokens);
                        app.last_prompt_tokens = Some(usage.input_tokens);
                        app.last_completion_tokens = Some(usage.output_tokens);

                        // Auto-save session after each turn
                        if let Ok(manager) = SessionManager::default_location() {
                            let session = if let Some(ref existing_id) = app.current_session_id {
                                // Update existing session
                                if let Ok(existing) = manager.load_session(existing_id) {
                                    update_session(
                                        existing,
                                        &app.api_messages,
                                        u64::from(app.total_tokens),
                                        app.system_prompt.as_ref(),
                                    )
                                } else {
                                    // Session was deleted, create new
                                    create_saved_session(
                                        &app.api_messages,
                                        &app.model,
                                        &app.workspace,
                                        u64::from(app.total_tokens),
                                        app.system_prompt.as_ref(),
                                    )
                                }
                            } else {
                                // Create new session
                                create_saved_session(
                                    &app.api_messages,
                                    &app.model,
                                    &app.workspace,
                                    u64::from(app.total_tokens),
                                    app.system_prompt.as_ref(),
                                )
                            };

                            if let Err(e) = manager.save_session(&session) {
                                eprintln!("Failed to save session: {e}");
                            } else {
                                app.current_session_id = Some(session.metadata.id.clone());
                            }
                        }

                        if queued_to_send.is_none() {
                            queued_to_send = app.pop_queued_message();
                        }
                    }
                    EngineEvent::Error { message, .. } => {
                        app.add_message(HistoryCell::System {
                            content: format!("Error: {message}"),
                        });
                        app.is_loading = false;
                    }
                    EngineEvent::Status { message } => {
                        app.status_message = Some(message);
                    }
                    EngineEvent::AgentSpawned { id, prompt } => {
                        app.add_message(HistoryCell::System {
                            content: format!(
                                "Sub-agent {id} spawned: {}",
                                summarize_tool_output(&prompt)
                            ),
                        });
                    }
                    EngineEvent::AgentProgress { id, status } => {
                        app.status_message = Some(format!("Sub-agent {id}: {status}"));
                    }
                    EngineEvent::AgentComplete { id, result } => {
                        app.add_message(HistoryCell::System {
                            content: format!(
                                "Sub-agent {id} completed: {}",
                                summarize_tool_output(&result)
                            ),
                        });
                    }
                    EngineEvent::AgentList { agents } => {
                        app.add_message(HistoryCell::System {
                            content: format_subagent_list(&agents),
                        });
                    }
                    EngineEvent::ApprovalRequired {
                        id,
                        tool_name,
                        description,
                    } => {
                        let needs_approval = requires_approval(
                            app.mode,
                            &tool_name,
                            app.approval_mode,
                            &app.approval_state.session_approved,
                        );
                        if !needs_approval {
                            app.approval_state.clear();
                            let _ = engine_handle.approve_tool_call(id.clone()).await;
                        } else if app.approval_mode == ApprovalMode::Never {
                            app.approval_state.clear();
                            let _ = engine_handle.deny_tool_call(id.clone()).await;
                            app.add_message(HistoryCell::System {
                                content: format!(
                                    "Blocked tool '{tool_name}' (approval_mode=never)"
                                ),
                            });
                        } else {
                            // Create approval request and show overlay
                            let request =
                                ApprovalRequest::new(&id, &tool_name, &serde_json::json!({}));
                            app.approval_state.request(request);
                            app.add_message(HistoryCell::System {
                                content: format!(
                                    "Approval required for tool '{tool_name}': {description}"
                                ),
                            });
                        }
                    }
                    EngineEvent::ToolCallProgress { id, output } => {
                        app.status_message =
                            Some(format!("Tool {id}: {}", summarize_tool_output(&output)));
                    }
                }
            }
        }

        if let Some(next) = queued_to_send {
            dispatch_user_message(app, &engine_handle, next).await?;
        }

        if app.approval_state.visible
            && app.approval_state.is_timed_out()
            && let Some((tool_id, _)) = app
                .approval_state
                .apply_decision(crate::tui::approval::ReviewDecision::Denied)
        {
            let _ = engine_handle.deny_tool_call(tool_id).await;
            app.add_message(HistoryCell::System {
                content: "Approval request timed out - denied".to_string(),
            });
        }

        terminal.draw(|f| render(f, app))?; // app is &mut

        if event::poll(std::time::Duration::from_millis(50))? {
            let evt = event::read()?;

            // Handle bracketed paste events
            if let Event::Paste(text) = &evt {
                if app.onboarding == OnboardingState::EnteringKey {
                    // Paste into API key input
                    for c in text.chars() {
                        if !c.is_control() {
                            app.api_key_input.insert(app.api_key_cursor, c);
                            app.api_key_cursor += 1;
                        }
                    }
                } else {
                    // Paste into main input
                    for c in text.chars() {
                        if c != '\n' && c != '\r' {
                            app.input.insert(app.cursor_position, c);
                            app.cursor_position += 1;
                        }
                    }
                }
                continue;
            }

            if let Event::Mouse(mouse) = evt {
                handle_mouse_event(app, mouse);
                continue;
            }

            let Event::Key(key) = evt else {
                continue;
            };

            if key.kind != KeyEventKind::Press {
                continue;
            }

            // Handle onboarding flow
            if app.onboarding != OnboardingState::None {
                match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let _ = engine_handle.send(Op::Shutdown).await;
                        return Ok(());
                    }
                    KeyCode::Esc => {
                        if app.onboarding == OnboardingState::EnteringKey {
                            app.onboarding = OnboardingState::Welcome;
                            app.api_key_input.clear();
                            app.api_key_cursor = 0;
                        }
                    }
                    KeyCode::Enter => match app.onboarding {
                        OnboardingState::Welcome => {
                            app.onboarding = OnboardingState::EnteringKey;
                        }
                        OnboardingState::EnteringKey => match app.submit_api_key() {
                            Ok(path) => {
                                app.status_message =
                                    Some(format!("API key saved to {}", path.display()));
                            }
                            Err(e) => {
                                app.status_message = Some(e.to_string());
                            }
                        },
                        OnboardingState::Success => {
                            app.finish_onboarding();
                        }
                        OnboardingState::None => {}
                    },
                    KeyCode::Backspace if app.onboarding == OnboardingState::EnteringKey => {
                        if app.api_key_cursor > 0 {
                            app.api_key_cursor -= 1;
                            app.api_key_input.remove(app.api_key_cursor);
                        }
                    }
                    KeyCode::Char(c) if app.onboarding == OnboardingState::EnteringKey => {
                        app.api_key_input.insert(app.api_key_cursor, c);
                        app.api_key_cursor += 1;
                    }
                    KeyCode::Char('v')
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && app.onboarding == OnboardingState::EnteringKey =>
                    {
                        // Ctrl+V handled by bracketed paste above
                        app.paste_from_clipboard();
                    }
                    _ => {}
                }
                continue;
            }

            // Handle help popup
            if app.show_help {
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                        app.show_help = false;
                        app.help_scroll = 0;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.help_scroll = app.help_scroll.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        app.help_scroll = app.help_scroll.saturating_add(1);
                    }
                    _ => {}
                }
                continue;
            }

            // Handle approval overlay
            if app.approval_state.visible {
                use crate::tui::approval::ReviewDecision;
                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.approval_state.select_prev();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        app.approval_state.select_next();
                    }
                    KeyCode::Enter => {
                        let decision = app.approval_state.current_decision();
                        if let Some((tool_id, decision)) =
                            app.approval_state.apply_decision(decision.clone())
                        {
                            match decision {
                                ReviewDecision::Approved | ReviewDecision::ApprovedForSession => {
                                    let _ = engine_handle.approve_tool_call(tool_id).await;
                                }
                                ReviewDecision::Denied | ReviewDecision::Abort => {
                                    let _ = engine_handle.deny_tool_call(tool_id).await;
                                }
                            }
                        }
                    }
                    KeyCode::Char('y') => {
                        if let Some((tool_id, _)) =
                            app.approval_state.apply_decision(ReviewDecision::Approved)
                        {
                            let _ = engine_handle.approve_tool_call(tool_id).await;
                        }
                    }
                    KeyCode::Char('a') => {
                        if let Some((tool_id, _)) = app
                            .approval_state
                            .apply_decision(ReviewDecision::ApprovedForSession)
                        {
                            let _ = engine_handle.approve_tool_call(tool_id).await;
                        }
                    }
                    KeyCode::Char('n') => {
                        if let Some((tool_id, _)) =
                            app.approval_state.apply_decision(ReviewDecision::Denied)
                        {
                            let _ = engine_handle.deny_tool_call(tool_id).await;
                        }
                    }
                    KeyCode::Esc => {
                        if let Some((tool_id, _)) =
                            app.approval_state.apply_decision(ReviewDecision::Abort)
                        {
                            let _ = engine_handle.deny_tool_call(tool_id).await;
                        }
                    }
                    _ => {}
                }
                continue;
            }

            // Global keybindings
            match key.code {
                KeyCode::Char('c') | KeyCode::Char('C')
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && app.transcript_selection.is_active() =>
                {
                    copy_active_selection(app);
                }
                KeyCode::Char('c') | KeyCode::Char('C') if is_copy_shortcut(&key) => {
                    copy_active_selection(app);
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    // Cancel current request or exit
                    if app.is_loading {
                        engine_handle.cancel();
                        app.is_loading = false;
                        app.status_message = Some("Request cancelled".to_string());
                    } else {
                        let _ = engine_handle.send(Op::Shutdown).await;
                        return Ok(());
                    }
                }
                KeyCode::F(1) => {
                    app.toggle_help();
                }
                KeyCode::Esc => {
                    if app.is_loading {
                        engine_handle.cancel();
                        app.is_loading = false;
                        app.status_message = Some("Request cancelled".to_string());
                    } else if !app.input.is_empty() {
                        app.clear_input();
                    } else {
                        app.set_mode(AppMode::Normal);
                    }
                }
                KeyCode::Up if key.modifiers.contains(KeyModifiers::ALT) => {
                    app.scroll_up(3);
                }
                KeyCode::Down if key.modifiers.contains(KeyModifiers::ALT) => {
                    app.scroll_down(3);
                }
                KeyCode::PageUp => {
                    let page = app.last_transcript_visible.max(1);
                    app.scroll_up(page);
                }
                KeyCode::PageDown => {
                    let page = app.last_transcript_visible.max(1);
                    app.scroll_down(page);
                }
                KeyCode::Tab => {
                    app.cycle_mode();
                    if app.mode == AppMode::Rlm {
                        app.rlm_repl_active = false;
                        app.add_message(HistoryCell::System {
                            content: crate::commands::rlm::welcome_message(),
                        });
                    }
                }
                // Input handling
                KeyCode::Enter => {
                    if let Some(input) = app.submit_input() {
                        if input.starts_with('/') {
                            // Use the commands module for slash commands
                            let result = commands::execute(&input, app);

                            // Handle command result
                            if let Some(msg) = result.message {
                                app.add_message(HistoryCell::System { content: msg });
                            }

                            if let Some(action) = result.action {
                                match action {
                                    AppAction::Quit => {
                                        let _ = engine_handle.send(Op::Shutdown).await;
                                        return Ok(());
                                    }
                                    AppAction::SaveSession(path) => {
                                        app.status_message =
                                            Some(format!("Session saved to {}", path.display()));
                                    }
                                    AppAction::LoadSession(path) => {
                                        app.status_message =
                                            Some(format!("Session loaded from {}", path.display()));
                                    }
                                    AppAction::SyncSession {
                                        messages,
                                        system_prompt,
                                        model,
                                        workspace,
                                    } => {
                                        let _ = engine_handle
                                            .send(Op::SyncSession {
                                                messages,
                                                system_prompt,
                                                model,
                                                workspace,
                                            })
                                            .await;
                                    }
                                    AppAction::SendMessage(content) => {
                                        let queued = build_queued_message(app, content);
                                        dispatch_user_message(app, &engine_handle, queued).await?;
                                    }
                                    AppAction::ListSubAgents => {
                                        let _ = engine_handle.send(Op::ListSubAgents).await;
                                    }
                                }
                            }
                        } else {
                            if app.mode == AppMode::Rlm && app.rlm_repl_active {
                                if rlm_repl_should_route_to_chat(app, &input) {
                                    app.rlm_repl_active = false;
                                    app.add_message(HistoryCell::System {
                                        content: "RLM REPL paused (no context loaded). Routing to chat so the model can call rlm_load. Use /repl to return.".to_string(),
                                    });
                                } else {
                                    handle_rlm_input(app, input);
                                    continue;
                                }
                            }

                            if app.mode == AppMode::Rlm
                                && let Some(path) = input.trim().strip_prefix('@')
                            {
                                let command = format!("/load @{path}");
                                let result = commands::execute(&command, app);
                                if let Some(msg) = result.message {
                                    app.add_message(HistoryCell::System { content: msg });
                                }
                                continue;
                            }

                            let queued = if let Some(mut draft) = app.queued_draft.take() {
                                draft.display = input;
                                draft
                            } else {
                                build_queued_message(app, input)
                            };
                            if app.is_loading {
                                app.queue_message(queued);
                                app.status_message = Some(format!(
                                    "Queued {} message(s) - /queue to view/edit",
                                    app.queued_message_count()
                                ));
                            } else {
                                dispatch_user_message(app, &engine_handle, queued).await?;
                            }
                        }
                    }
                }
                KeyCode::Backspace => {
                    app.delete_char();
                }
                KeyCode::Delete => {
                    app.delete_char_forward();
                }
                KeyCode::Left => {
                    app.move_cursor_left();
                }
                KeyCode::Right => {
                    app.move_cursor_right();
                }
                KeyCode::Home if key.modifiers.is_empty() => {
                    if let Some(anchor) =
                        TranscriptScroll::anchor_for(app.transcript_cache.line_meta(), 0)
                    {
                        app.transcript_scroll = anchor;
                    }
                }
                KeyCode::End if key.modifiers.is_empty() => {
                    app.scroll_to_bottom();
                }
                KeyCode::Home | KeyCode::Char('a')
                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    app.move_cursor_start();
                }
                KeyCode::End | KeyCode::Char('e')
                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    app.move_cursor_end();
                }
                KeyCode::Up => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        app.history_up();
                    } else if should_scroll_with_arrows(app) {
                        app.scroll_up(1);
                    } else {
                        app.history_up();
                    }
                }
                KeyCode::Down => {
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        app.history_down();
                    } else if should_scroll_with_arrows(app) {
                        app.scroll_down(1);
                    } else {
                        app.history_down();
                    }
                }
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.clear_input();
                }
                KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    app.paste_from_clipboard();
                }
                KeyCode::Char(c) => {
                    app.insert_char(c);
                }
                _ => {}
            }
        }
    }
}

fn build_queued_message(app: &mut App, input: String) -> QueuedMessage {
    let skill_instruction = app.active_skill.take();
    QueuedMessage::new(input, skill_instruction)
}

async fn dispatch_user_message(
    app: &mut App,
    engine_handle: &EngineHandle,
    message: QueuedMessage,
) -> Result<()> {
    let override_query = maybe_auto_switch_to_rlm(app, &message.display);
    let content = if let Some(query) = override_query.as_deref() {
        message.content_with_query(query)
    } else {
        message.content()
    };
    let rlm_summary = if app.mode == AppMode::Rlm {
        app.rlm_session
            .lock()
            .ok()
            .map(|session| rlm::session_summary(&session))
    } else {
        None
    };
    app.system_prompt = Some(prompts::system_prompt_for_mode_with_context(
        app.mode,
        &app.workspace,
        rlm_summary.as_deref(),
    ));
    app.add_message(HistoryCell::User {
        content: message.display.clone(),
    });
    app.api_messages.push(Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text: content.clone(),
            cache_control: None,
        }],
    });

    engine_handle
        .send(Op::SendMessage {
            content,
            mode: app.mode,
            model: app.model.clone(),
            allow_shell: app.allow_shell,
            trust_mode: app.trust_mode,
        })
        .await?;

    Ok(())
}

fn handle_rlm_input(app: &mut App, input: String) {
    if let Some(path) = input.trim().strip_prefix('@') {
        let command = format!("/load @{path}");
        let result = commands::execute(&command, app);
        if let Some(msg) = result.message {
            app.add_message(HistoryCell::System { content: msg });
        }
        return;
    }

    app.add_message(HistoryCell::User {
        content: input.clone(),
    });

    let content = match app.rlm_session.lock() {
        Ok(mut session) => match rlm::eval_in_session(&mut session, &input) {
            Ok(result) => {
                let trimmed = result.trim();
                if trimmed.is_empty() {
                    "RLM: (no output)".to_string()
                } else {
                    format!("RLM:\n{result}")
                }
            }
            Err(err) => format!("RLM error: {err}"),
        },
        Err(_) => "RLM error: failed to access session".to_string(),
    };

    app.add_message(HistoryCell::System { content });
}

struct AutoRlmDecision {
    source: AutoRlmSource,
    reason: String,
}

enum AutoRlmSource {
    File(PathBuf),
    Paste {
        content: String,
        query: Option<String>,
    },
    None,
}

struct AutoRlmLoaded {
    context_id: String,
    line_count: usize,
    char_count: usize,
}

fn maybe_auto_switch_to_rlm(app: &mut App, input: &str) -> Option<String> {
    let already_rlm = app.mode == AppMode::Rlm;
    let decision = auto_rlm_decision(app, input, already_rlm)?;

    if !already_rlm {
        app.set_mode(AppMode::Rlm);
        app.rlm_repl_active = false;
    }

    let mut messages = vec![if already_rlm {
        format!("Auto-loaded RLM context ({})", decision.reason)
    } else {
        format!("Auto-switched to RLM mode ({})", decision.reason)
    }];
    let mut override_query = None;

    match decision.source {
        AutoRlmSource::File(path) => match load_file_into_rlm(app, &path) {
            Ok(loaded) => {
                messages.push(format!(
                    "Loaded {} as '{}' ({} lines, {} chars)",
                    format_load_path(app, &path),
                    loaded.context_id,
                    loaded.line_count,
                    loaded.char_count
                ));
                override_query = Some(format!(
                    "{}\n\nUse RLM context '{}' loaded from {}.",
                    input.trim(),
                    loaded.context_id,
                    format_load_path(app, &path)
                ));
            }
            Err(err) => {
                messages.push(format!("RLM auto-load failed: {err}"));
            }
        },
        AutoRlmSource::Paste { content, query } => match load_paste_into_rlm(app, content) {
            Ok(loaded) => {
                messages.push(format!(
                    "Loaded pasted content as '{}' ({} lines, {} chars)",
                    loaded.context_id, loaded.line_count, loaded.char_count
                ));
                let base_query = query.unwrap_or_else(|| {
                    "Analyze the pasted content and answer the user request.".to_string()
                });
                override_query = Some(format!(
                    "{base_query}\n\nRLM context: '{}'.",
                    loaded.context_id
                ));
            }
            Err(err) => {
                messages.push(format!("RLM auto-load failed: {err}"));
                override_query = Some(
                    "The user pasted a large block, but auto-loading failed. Ask them to retry /load or paste again."
                        .to_string(),
                );
            }
        },
        AutoRlmSource::None => {}
    }

    app.add_message(HistoryCell::System {
        content: messages.join("\n"),
    });

    override_query
}

fn auto_rlm_decision(app: &App, input: &str, already_rlm: bool) -> Option<AutoRlmDecision> {
    let input_lower = input.to_lowercase();
    let wants_largest_file = input_lower.contains("largest file")
        || input_lower.contains("biggest file")
        || input_lower.contains("largest files");
    let explicit_rlm_request = input_lower
        .split_whitespace()
        .any(|word| word.trim_matches(|c: char| !c.is_ascii_alphanumeric()) == "rlm")
        || input_lower.contains("rlm mode");
    let explicit_rlm = already_rlm || explicit_rlm_request;
    let has_hint = input_lower.contains("chunk")
        || input_lower.contains("chunking")
        || input_lower.contains("huge")
        || input_lower.contains("massive")
        || input_lower.contains("entire repo")
        || input_lower.contains("whole repo")
        || input_lower.contains("full repo")
        || input_lower.contains("whole project")
        || input_lower.contains("entire project")
        || input_lower.contains("full project")
        || explicit_rlm;

    if let Some(decision) = auto_rlm_paste_decision(input, explicit_rlm, has_hint) {
        return Some(decision);
    }

    if wants_largest_file && let Some((path, size)) = find_largest_file(&app.workspace) {
        return Some(AutoRlmDecision {
            source: AutoRlmSource::File(path),
            reason: format!("requested largest file ({} bytes)", size),
        });
    }

    let Some(candidate) = detect_requested_file(input, &app.workspace) else {
        if explicit_rlm_request && !already_rlm {
            return Some(AutoRlmDecision {
                source: AutoRlmSource::None,
                reason: "explicit RLM request".to_string(),
            });
        }
        return None;
    };
    if !app.trust_mode {
        let workspace_root = app
            .workspace
            .canonicalize()
            .unwrap_or_else(|_| app.workspace.clone());
        let candidate_canonical = candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.clone());
        if !candidate_canonical.starts_with(&workspace_root) {
            return None;
        }
    }
    let metadata = fs::metadata(&candidate).ok()?;
    if !metadata.is_file() {
        return None;
    }

    let size = metadata.len();
    let min_bytes = if has_hint {
        AUTO_RLM_HINT_FILE_BYTES
    } else {
        AUTO_RLM_MIN_FILE_BYTES
    };
    if size < min_bytes && !explicit_rlm {
        return None;
    }

    let reason = if explicit_rlm_request && !already_rlm {
        format!("explicit RLM file request ({} bytes)", size)
    } else if already_rlm {
        format!("RLM file request ({} bytes)", size)
    } else {
        format!("large file ({} bytes)", size)
    };

    Some(AutoRlmDecision {
        source: AutoRlmSource::File(candidate),
        reason,
    })
}

fn auto_rlm_paste_decision(
    input: &str,
    explicit_rlm: bool,
    has_hint: bool,
) -> Option<AutoRlmDecision> {
    let min_chars = if explicit_rlm || has_hint {
        AUTO_RLM_PASTE_HINT_CHARS
    } else {
        AUTO_RLM_PASTE_MIN_CHARS
    };

    if input.len() < min_chars {
        return None;
    }

    let (query, content) = split_paste_input(input);
    if content.len() < min_chars {
        return None;
    }

    Some(AutoRlmDecision {
        source: AutoRlmSource::Paste { content, query },
        reason: format!("pasted content ({} chars)", input.len()),
    })
}

fn split_paste_input(input: &str) -> (Option<String>, String) {
    let trimmed = input.trim();

    if let Some(idx) = trimmed.find("```").or_else(|| trimmed.find("~~~")) {
        let (prefix, rest) = trimmed.split_at(idx);
        let query = clean_query_prefix(prefix);
        if !query.is_empty() && query.len() <= AUTO_RLM_PASTE_QUERY_MAX_CHARS {
            return (Some(query.to_string()), rest.trim_start().to_string());
        }
    }

    if let Some(idx) = trimmed.find("\n\n") {
        let (prefix, rest) = trimmed.split_at(idx);
        let query = clean_query_prefix(prefix);
        if !query.is_empty() && query.len() <= AUTO_RLM_PASTE_QUERY_MAX_CHARS {
            return (Some(query.to_string()), rest.trim_start().to_string());
        }
    }

    if let Some((first, rest)) = trimmed.split_once('\n') {
        let query = clean_query_prefix(first);
        if !query.is_empty() && query.len() <= AUTO_RLM_PASTE_FIRST_LINE_MAX_CHARS {
            return (Some(query.to_string()), rest.trim_start().to_string());
        }
    }

    (None, trimmed.to_string())
}

fn clean_query_prefix(prefix: &str) -> &str {
    prefix.trim().trim_end_matches(':')
}

fn load_file_into_rlm(app: &mut App, path: &Path) -> Result<AutoRlmLoaded, String> {
    let mut session = app
        .rlm_session
        .lock()
        .map_err(|_| "Failed to access RLM session".to_string())?;
    let base_id = rlm::context_id_from_path(path);
    let context_id = rlm::unique_context_id(&session, &base_id);
    let (line_count, char_count) = session
        .load_file(&context_id, path)
        .map_err(|err| err.to_string())?;
    Ok(AutoRlmLoaded {
        context_id,
        line_count,
        char_count,
    })
}

fn load_paste_into_rlm(app: &mut App, content: String) -> Result<AutoRlmLoaded, String> {
    let mut session = app
        .rlm_session
        .lock()
        .map_err(|_| "Failed to access RLM session".to_string())?;
    let context_id = rlm::unique_context_id(&session, "paste");
    let line_count = content.lines().count();
    let char_count = content.len();
    session.load_context(&context_id, content, Some("pasted input".to_string()));
    Ok(AutoRlmLoaded {
        context_id,
        line_count,
        char_count,
    })
}

fn detect_requested_file(input: &str, workspace: &Path) -> Option<PathBuf> {
    if input.to_lowercase().contains("readme") {
        let readme = ["README.md", "README", "README.txt"];
        for name in readme {
            let candidate = workspace.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    for token in input.split_whitespace() {
        let token = trim_token(token);
        if token.is_empty() || token.contains("://") {
            continue;
        }
        if !looks_like_path_token(token) {
            continue;
        }
        if let Some(path) = resolve_candidate_path(token, workspace) {
            return Some(path);
        }
    }

    None
}

fn trim_token(token: &str) -> &str {
    token
        .trim_start_matches(['(', '[', '{', '"', '\'', '`'])
        .trim_end_matches([')', ']', '}', ',', ';', ':', '"', '\'', '`', '.'])
}

fn looks_like_path_token(token: &str) -> bool {
    let lower = token.to_lowercase();
    if lower == "readme" || lower == "readme.md" {
        return true;
    }
    if token.starts_with('@') || token.contains('/') || token.contains('\\') {
        return true;
    }
    matches!(
        token.rsplit('.').next(),
        Some(
            "md" | "txt"
                | "rs"
                | "toml"
                | "json"
                | "yaml"
                | "yml"
                | "py"
                | "js"
                | "ts"
                | "tsx"
                | "jsx"
                | "go"
                | "java"
                | "c"
                | "h"
                | "cpp"
                | "log"
        )
    )
}

fn resolve_candidate_path(token: &str, workspace: &Path) -> Option<PathBuf> {
    let candidate = if let Some(stripped) = token.strip_prefix('@') {
        workspace.join(stripped.trim_start_matches(['/', '\\']))
    } else if Path::new(token).is_absolute() {
        PathBuf::from(token)
    } else {
        workspace.join(token)
    };

    if candidate.is_file() {
        return Some(candidate);
    }
    None
}

fn find_largest_file(workspace: &Path) -> Option<(PathBuf, u64)> {
    let mut stack = vec![workspace.to_path_buf()];
    let mut scanned = 0;
    let mut largest: Option<(PathBuf, u64)> = None;

    while let Some(dir) = stack.pop() {
        if scanned >= AUTO_RLM_MAX_SCAN_ENTRIES {
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            scanned += 1;
            if scanned >= AUTO_RLM_MAX_SCAN_ENTRIES {
                break;
            }
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str())
                    && AUTO_RLM_EXCLUDED_DIRS.contains(&name)
                {
                    continue;
                }
                stack.push(path);
            } else if path.is_file() {
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                let size = metadata.len();
                match largest {
                    Some((_, current)) if size <= current => {}
                    _ => largest = Some((path, size)),
                }
            }
        }
    }

    largest
}

fn format_load_path(app: &App, path: &Path) -> String {
    if let Ok(stripped) = path.strip_prefix(&app.workspace) {
        return format!("@{}", stripped.display());
    }
    path.display().to_string()
}

fn looks_like_rlm_expr(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('/') {
        return true;
    }

    let token = trimmed
        .split(|c: char| c == '(' || c.is_whitespace())
        .next()
        .unwrap_or("");
    matches!(
        token,
        "len"
            | "line_count"
            | "lines"
            | "search"
            | "chunk"
            | "chunk_sections"
            | "chunk_lines"
            | "chunk_auto"
            | "vars"
            | "get"
            | "set"
            | "append"
            | "del"
            | "head"
            | "tail"
            | "peek"
    )
}

fn rlm_repl_should_route_to_chat(app: &App, input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.is_empty() || looks_like_rlm_expr(trimmed) {
        return false;
    }

    let Ok(session) = app.rlm_session.lock() else {
        return false;
    };
    session.contexts.is_empty()
}

fn render(f: &mut Frame, app: &mut App) {
    let size = f.area();

    // Show onboarding screen if needed
    if app.onboarding != OnboardingState::None {
        render_onboarding(f, size, app);
        return;
    }

    let footer_height = 1;
    let queued_preview = app.queued_message_previews(MAX_QUEUED_PREVIEW);
    let queued_lines = if queued_preview.is_empty() {
        0
    } else {
        queued_preview.len() + 1
    };
    let editing_lines = usize::from(app.queued_draft.is_some());
    let status_lines = usize::from(app.is_loading);
    let status_height =
        u16::try_from(status_lines + queued_lines + editing_lines).unwrap_or(u16::MAX);
    let prompt = prompt_for_mode(app.mode, app.rlm_repl_active);
    let composer_height = composer_height(
        &app.input,
        size.width,
        size.height.saturating_sub(footer_height + status_height),
        prompt,
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(status_height),
            Constraint::Length(composer_height),
            Constraint::Length(footer_height),
        ])
        .split(size);

    render_chat(f, chunks[0], app);
    if status_height > 0 {
        render_status_indicator(f, chunks[1], app, &queued_preview);
    }
    render_composer(f, chunks[2], app);
    render_footer(f, chunks[3], app);

    if app.show_help {
        render_help_popup(f, size, app.help_scroll);
    }

    // Render approval overlay if visible
    if app.approval_state.visible {
        render_approval_overlay(f, &app.approval_state);
    }
}

fn render_chat(f: &mut Frame, area: Rect, app: &mut App) {
    let mut content_area = area;
    let mut scrollbar_area = None;

    let show_scrollbar = !matches!(app.transcript_scroll, TranscriptScroll::ToBottom)
        && area.width > 1
        && area.height > 1;
    if show_scrollbar {
        content_area.width = content_area.width.saturating_sub(1);
        scrollbar_area = Some(Rect {
            x: content_area.x + content_area.width,
            y: content_area.y,
            width: 1,
            height: content_area.height,
        });
    }

    app.transcript_cache
        .ensure(&app.history, content_area.width.max(1), app.history_version);

    let total_lines = app.transcript_cache.total_lines();
    let visible_lines = content_area.height as usize;
    let line_meta = app.transcript_cache.line_meta();

    if app.pending_scroll_delta != 0 {
        app.transcript_scroll =
            app.transcript_scroll
                .scrolled_by(app.pending_scroll_delta, line_meta, visible_lines);
        app.pending_scroll_delta = 0;
    }

    let max_start = total_lines.saturating_sub(visible_lines);
    let (scroll_state, top) = app.transcript_scroll.resolve_top(line_meta, max_start);
    app.transcript_scroll = scroll_state;

    app.last_transcript_area = Some(content_area);
    app.last_scrollbar_area = scrollbar_area;
    app.last_transcript_top = top;
    app.last_transcript_visible = visible_lines;
    app.last_transcript_total = total_lines;
    app.last_transcript_padding_top = 0;

    let end = (top + visible_lines).min(total_lines);
    let mut visible = if total_lines == 0 {
        vec![Line::from("")]
    } else {
        app.transcript_cache.lines()[top..end].to_vec()
    };

    apply_selection(&mut visible, top, app);

    // Bottom-align the transcript when the user is "following" the chat and the
    // content doesn't fill the available viewport height.
    if matches!(app.transcript_scroll, TranscriptScroll::ToBottom) {
        app.last_transcript_padding_top = visible_lines.saturating_sub(visible.len());
        pad_lines_to_bottom(&mut visible, visible_lines);
    }

    let paragraph = Paragraph::new(visible);
    f.render_widget(paragraph, content_area);

    if let Some(scrollbar_area) = scrollbar_area {
        render_scrollbar(f, scrollbar_area, top, visible_lines, total_lines);
    }
}

fn pad_lines_to_bottom(lines: &mut Vec<Line<'static>>, height: usize) {
    if lines.len() >= height {
        return;
    }
    let padding = height.saturating_sub(lines.len());
    if padding == 0 {
        return;
    }

    let mut padded = Vec::with_capacity(height);
    padded.extend(std::iter::repeat_n(Line::from(""), padding));
    padded.append(lines);
    *lines = padded;
}

fn render_status_indicator(f: &mut Frame, area: Rect, app: &App, queued: &[String]) {
    let mut lines = Vec::new();

    if app.is_loading {
        let header = app.reasoning_header.clone();
        let elapsed = app.turn_started_at.map(format_elapsed);
        let spinner = minimax_squiggle(app.turn_started_at);
        let label = minimax_thinking_label(app.turn_started_at);
        let mut spans = vec![
            Span::styled(spinner, Style::default().fg(MINIMAX_CORAL).bold()),
            Span::raw(" "),
            Span::styled(label, Style::default().fg(Color::Yellow).bold()),
        ];
        if let Some(header) = header {
            spans.push(Span::raw(": "));
            spans.push(Span::styled(header, Style::default().fg(Color::Yellow)));
        }

        if let Some(elapsed) = elapsed {
            spans.push(Span::raw(" | "));
            spans.push(Span::styled(elapsed, Style::default().fg(Color::DarkGray)));
        }

        spans.push(Span::raw(" | "));
        spans.push(Span::styled(
            "Esc/Ctrl+C to interrupt",
            Style::default().fg(Color::DarkGray),
        ));

        lines.push(Line::from(spans));
    }

    if let Some(draft) = app.queued_draft.as_ref() {
        let available = area.width as usize;
        let prefix = "Editing queued:";
        let prefix_width = prefix.width() + 1;
        let max_len = available.saturating_sub(prefix_width).max(1);
        let preview = truncate_line_to_width(&draft.display, max_len);
        lines.push(Line::from(vec![
            Span::styled(prefix, Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled(preview, Style::default().fg(Color::Yellow)),
        ]));
    }

    if !queued.is_empty() {
        let available = area.width as usize;
        let queued_count = app.queued_message_count();
        let header = format!("Queued ({queued_count}) - /queue edit <n>");
        let header = truncate_line_to_width(&header, available.max(1));
        lines.push(Line::from(vec![Span::styled(
            header,
            Style::default().fg(Color::DarkGray),
        )]));

        for (idx, message) in queued.iter().enumerate() {
            let label = if message.starts_with('+') {
                message.to_string()
            } else {
                format!("{}. {message}", idx + 1)
            };
            let preview = truncate_line_to_width(&label, available.max(1));
            lines.push(Line::from(vec![Span::styled(
                preview,
                Style::default().fg(Color::Gray),
            )]));
        }
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn render_composer(f: &mut Frame, area: Rect, app: &mut App) {
    let prompt = prompt_for_mode(app.mode, app.rlm_repl_active);
    let prompt_width = prompt.width();
    let prompt_width_u16 = u16::try_from(prompt_width).unwrap_or(u16::MAX);
    let content_width = usize::from(area.width.saturating_sub(prompt_width_u16).max(1));
    let max_height = usize::from(area.height);

    let (visible_lines, cursor_row, cursor_col) =
        layout_input(&app.input, app.cursor_position, content_width, max_height);

    let background = Style::default().bg(Color::Rgb(24, 32, 24));
    let block = Block::default().style(background);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    if app.input.is_empty() {
        let placeholder = if app.mode == AppMode::Rlm {
            if app.rlm_repl_active {
                "Type an RLM expression or /repl to exit..."
            } else {
                "Ask a question or /repl to enter expression mode..."
            }
        } else {
            "Type a message or /help for commands..."
        };
        lines.push(Line::from(vec![
            Span::styled(prompt, Style::default().fg(Color::Green).bold()),
            Span::styled(placeholder, Style::default().fg(Color::DarkGray).italic()),
        ]));
    } else {
        for (idx, line) in visible_lines.iter().enumerate() {
            let prefix = if idx == 0 { prompt } else { "  " };
            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Green).bold()),
                Span::styled(line.clone(), Style::default().fg(Color::White)),
            ]));
        }
    }

    let paragraph = Paragraph::new(lines).style(background);
    f.render_widget(paragraph, area);

    let cursor_x = area
        .x
        .saturating_add(prompt_width_u16)
        .saturating_add(u16::try_from(cursor_col).unwrap_or(u16::MAX));
    let cursor_y = area
        .y
        .saturating_add(u16::try_from(cursor_row).unwrap_or(u16::MAX));
    if cursor_x < area.x + area.width && cursor_y < area.y + area.height {
        f.set_cursor_position((cursor_x, cursor_y));
    }
}

fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    let mut spans = vec![
        Span::styled(
            format!(" {} ", app.mode.label()),
            mode_badge_style(app.mode),
        ),
        Span::raw(" | "),
        Span::styled(context_indicator(app), Style::default().fg(Color::DarkGray)),
    ];

    if let Some((label, style)) = rlm_usage_badge(app) {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(label, style));
    }

    if let (Some(prompt), Some(completion)) = (app.last_prompt_tokens, app.last_completion_tokens) {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(
            format!("last tokens in/out: {prompt}/{completion}"),
            Style::default().fg(Color::DarkGray),
        ));
    }

    let can_scroll = app.last_transcript_total > app.last_transcript_visible;
    if can_scroll {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(
            "Alt+Up/Down scroll",
            Style::default().fg(Color::DarkGray),
        ));
    }

    if can_scroll && !matches!(app.transcript_scroll, TranscriptScroll::ToBottom) {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(
            "PgUp/PgDn/Home/End",
            Style::default().fg(Color::DarkGray),
        ));
        if app.last_transcript_total > 0 {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!(
                    "{}/{}",
                    app.last_transcript_top + 1,
                    app.last_transcript_total
                ),
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    if app.transcript_selection.is_active() {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(
            copy_selection_hint(),
            Style::default().fg(Color::DarkGray),
        ));
    }

    if let Some(ref msg) = app.status_message {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(msg, Style::default().fg(Color::Yellow)));
    }

    let footer = Paragraph::new(Line::from(spans));
    f.render_widget(footer, area);
}

fn rlm_usage_badge(app: &App) -> Option<(String, Style)> {
    let session = app.rlm_session.lock().ok()?;
    let usage = &session.usage;
    if usage.queries == 0 {
        return None;
    }

    let warn = usage.queries >= RLM_BUDGET_WARN_QUERIES
        || usage.input_tokens >= RLM_BUDGET_WARN_INPUT_TOKENS
        || usage.output_tokens >= RLM_BUDGET_WARN_OUTPUT_TOKENS;
    let hard = usage.queries >= RLM_BUDGET_HARD_QUERIES
        || usage.input_tokens >= RLM_BUDGET_HARD_INPUT_TOKENS
        || usage.output_tokens >= RLM_BUDGET_HARD_OUTPUT_TOKENS;

    let style = if hard {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if warn {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    Some((
        format!(
            "RLM q:{} in/out:{} /{}",
            usage.queries, usage.input_tokens, usage.output_tokens
        ),
        style,
    ))
}

fn mode_color(mode: AppMode) -> Color {
    match mode {
        AppMode::Normal => Color::Gray,
        AppMode::Agent => MINIMAX_RED,
        AppMode::Yolo => Color::Green,
        AppMode::Plan => MINIMAX_ORANGE,
        AppMode::Rlm => MINIMAX_CORAL,
    }
}

fn mode_badge_style(mode: AppMode) -> Style {
    Style::default()
        .fg(Color::White)
        .bg(mode_color(mode))
        .add_modifier(Modifier::BOLD)
}

fn prompt_for_mode(mode: AppMode, rlm_repl_active: bool) -> &'static str {
    match mode {
        AppMode::Normal => "> ",
        AppMode::Agent => "agent> ",
        AppMode::Yolo => "yolo> ",
        AppMode::Plan => "plan> ",
        AppMode::Rlm => {
            if rlm_repl_active {
                "rlm(repl)> "
            } else {
                "rlm> "
            }
        }
    }
}

fn composer_height(input: &str, width: u16, available_height: u16, prompt: &str) -> u16 {
    let prompt_width = prompt.width();
    let prompt_width_u16 = u16::try_from(prompt_width).unwrap_or(u16::MAX);
    let content_width = usize::from(width.saturating_sub(prompt_width_u16).max(1));
    let mut line_count = wrap_input_lines(input, content_width).len();
    if line_count == 0 {
        line_count = 1;
    }
    let max_height = usize::from(available_height.clamp(1, 8));
    line_count.clamp(1, max_height).try_into().unwrap_or(1)
}

fn layout_input(
    input: &str,
    cursor: usize,
    width: usize,
    max_height: usize,
) -> (Vec<String>, usize, usize) {
    let mut lines = wrap_input_lines(input, width);
    if lines.is_empty() {
        lines.push(String::new());
    }
    let (cursor_row, cursor_col) = cursor_row_col(input, cursor, width.max(1));

    let max_height = max_height.max(1);
    let mut start = 0usize;
    if cursor_row >= max_height {
        start = cursor_row + 1 - max_height;
    }
    if start + max_height > lines.len() {
        start = lines.len().saturating_sub(max_height);
    }
    let visible = lines
        .into_iter()
        .skip(start)
        .take(max_height)
        .collect::<Vec<_>>();
    let visible_cursor_row = cursor_row.saturating_sub(start);

    (
        visible,
        visible_cursor_row,
        cursor_col.min(width.saturating_sub(1)),
    )
}

fn cursor_row_col(input: &str, cursor: usize, width: usize) -> (usize, usize) {
    let mut row = 0usize;
    let mut col = 0usize;
    let mut idx = 0usize;

    for ch in input.chars() {
        if idx >= cursor {
            break;
        }
        idx += ch.len_utf8();

        if ch == '\n' {
            row += 1;
            col = 0;
            continue;
        }

        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(1);
        if col + ch_width > width {
            row += 1;
            col = 0;
        }
        col += ch_width;
        if col >= width {
            row += 1;
            col = 0;
        }
    }

    (row, col.min(width.saturating_sub(1)))
}

fn wrap_input_lines(input: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    if input.is_empty() {
        return lines;
    }

    for raw in input.split('\n') {
        let wrapped = wrap_text(raw, width);
        if wrapped.is_empty() {
            lines.push(String::new());
        } else {
            lines.extend(wrapped);
        }
    }

    if input.ends_with('\n') {
        lines.push(String::new());
    }

    lines
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for word in text.split_whitespace() {
        let word_width = UnicodeWidthStr::width(word);
        if current_width == 0 {
            current.push_str(word);
            current_width = word_width;
            continue;
        }

        if current_width + 1 + word_width <= width {
            current.push(' ');
            current.push_str(word);
            current_width += 1 + word_width;
        } else {
            lines.push(current);
            current = word.to_string();
            current_width = word_width;
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        vec![String::new()]
    } else {
        lines
    }
}

fn apply_selection(lines: &mut [Line<'static>], top: usize, app: &App) {
    let Some((start, end)) = app.transcript_selection.ordered_endpoints() else {
        return;
    };

    for (idx, line) in lines.iter_mut().enumerate() {
        let line_index = top + idx;
        if line_index < start.line_index || line_index > end.line_index {
            continue;
        }
        for span in &mut line.spans {
            span.style = span
                .style
                .patch(Style::default().bg(Color::Rgb(60, 60, 60)));
        }
    }
}

fn render_scrollbar(f: &mut Frame, area: Rect, top: usize, visible: usize, total: usize) {
    if total <= visible || area.height == 0 {
        return;
    }

    let height = usize::from(area.height);
    let max_start = total.saturating_sub(visible).max(1);
    let thumb_height = visible
        .saturating_mul(height)
        .div_ceil(total)
        .clamp(1, height);
    let track = height.saturating_sub(thumb_height).max(1);
    let thumb_start = (top.saturating_mul(track) + max_start / 2) / max_start;

    let mut lines = Vec::new();
    for row in 0..height {
        let ch = if row >= thumb_start && row < thumb_start + thumb_height {
            "#"
        } else {
            "|"
        };
        lines.push(Line::from(Span::styled(
            ch,
            Style::default().fg(Color::DarkGray),
        )));
    }

    let scrollbar = Paragraph::new(lines);
    f.render_widget(scrollbar, area);
}

fn context_indicator(app: &App) -> String {
    let used = if app.total_conversation_tokens > 0 {
        Some(i64::from(app.total_conversation_tokens))
    } else {
        estimated_context_tokens(app)
    };

    if let Some(max) = context_window_for_model(&app.model) {
        if let Some(used) = used {
            let max_i64 = i64::from(max);
            let remaining = (max_i64 - used).max(0);
            let percent = ((remaining.saturating_mul(100) + max_i64 / 2) / max_i64).clamp(0, 100);
            format!("{percent}% context left")
        } else {
            "100% context left".to_string()
        }
    } else if let Some(used) = used {
        format!("{used} used")
    } else {
        "100% context left".to_string()
    }
}

fn estimated_context_tokens(app: &App) -> Option<i64> {
    let mut total_chars = estimate_message_chars(&app.api_messages);

    match &app.system_prompt {
        Some(SystemPrompt::Text(text)) => total_chars = total_chars.saturating_add(text.len()),
        Some(SystemPrompt::Blocks(blocks)) => {
            for block in blocks {
                total_chars = total_chars.saturating_add(block.text.len());
            }
        }
        None => {}
    }

    let estimated_tokens = total_chars / 4;
    i64::try_from(estimated_tokens).ok()
}

fn format_elapsed(start: Instant) -> String {
    let elapsed = start.elapsed().as_secs();
    if elapsed >= 60 {
        format!("{}m{:02}s", elapsed / 60, elapsed % 60)
    } else {
        format!("{elapsed}s")
    }
}

fn minimax_squiggle(start: Option<Instant>) -> &'static str {
    const FRAMES: [&str; 8] = [
        "MM~", "MM~~", "MM~~~", "MM~~~~", "MM~~~", "MM~~", "MM~", "MM.",
    ];
    let elapsed_ms = start.map_or(0, |t| t.elapsed().as_millis());
    let idx = ((elapsed_ms / 220) as usize) % FRAMES.len();
    FRAMES[idx]
}

fn minimax_thinking_label(start: Option<Instant>) -> &'static str {
    const TAGLINES: [&str; 5] = [
        "Thinking",
        "Plotting",
        "Drafting",
        "You're absolutely right! ... maybe.",
        "Working",
    ];
    const INITIAL_MS: u128 = 2400;
    let elapsed_ms = start.map_or(0, |t| t.elapsed().as_millis());
    if elapsed_ms < INITIAL_MS {
        return "Working";
    }
    let idx = (((elapsed_ms - INITIAL_MS) / 2400) as usize) % TAGLINES.len();
    TAGLINES[idx]
}

fn truncate_line_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }
    if max_width <= 3 {
        return text.chars().take(max_width).collect();
    }

    let mut out = String::new();
    let mut width = 0usize;
    let limit = max_width.saturating_sub(3);
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + ch_width > limit {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out.push_str("...");
    out
}

fn handle_mouse_event(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            let update = app.mouse_scroll.on_scroll(ScrollDirection::Up);
            app.pending_scroll_delta += update.delta_lines;
        }
        MouseEventKind::ScrollDown => {
            let update = app.mouse_scroll.on_scroll(ScrollDirection::Down);
            app.pending_scroll_delta += update.delta_lines;
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if is_inside_scrollbar(app, mouse) {
                jump_scrollbar(app, mouse);
                return;
            }

            if let Some(point) = selection_point_from_mouse(app, mouse) {
                app.transcript_selection.anchor = Some(point);
                app.transcript_selection.head = Some(point);
                app.transcript_selection.dragging = true;

                if app.is_loading
                    && matches!(app.transcript_scroll, TranscriptScroll::ToBottom)
                    && let Some(anchor) = TranscriptScroll::anchor_for(
                        app.transcript_cache.line_meta(),
                        app.last_transcript_top,
                    )
                {
                    app.transcript_scroll = anchor;
                }
            } else if app.transcript_selection.is_active() {
                app.transcript_selection.clear();
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if is_inside_scrollbar(app, mouse) {
                jump_scrollbar(app, mouse);
                return;
            }

            if app.transcript_selection.dragging
                && let Some(point) = selection_point_from_mouse(app, mouse)
            {
                app.transcript_selection.head = Some(point);
            }
        }
        MouseEventKind::Up(MouseButton::Left) => {
            if app.transcript_selection.dragging {
                app.transcript_selection.dragging = false;
                if selection_has_content(app) {
                    copy_active_selection(app);
                }
            }
        }
        _ => {}
    }
}

fn selection_point_from_mouse(app: &App, mouse: MouseEvent) -> Option<TranscriptSelectionPoint> {
    selection_point_from_position(
        app.last_transcript_area?,
        mouse.column,
        mouse.row,
        app.last_transcript_top,
        app.last_transcript_total,
        app.last_transcript_padding_top,
    )
}

fn selection_point_from_position(
    area: Rect,
    column: u16,
    row: u16,
    transcript_top: usize,
    transcript_total: usize,
    padding_top: usize,
) -> Option<TranscriptSelectionPoint> {
    if column < area.x
        || column >= area.x + area.width
        || row < area.y
        || row >= area.y + area.height
    {
        return None;
    }

    if transcript_total == 0 {
        return None;
    }

    let row = row.saturating_sub(area.y) as usize;
    if row < padding_top {
        return None;
    }
    let row = row.saturating_sub(padding_top);

    let col = column.saturating_sub(area.x) as usize;
    let line_index = transcript_top
        .saturating_add(row)
        .min(transcript_total.saturating_sub(1));

    Some(TranscriptSelectionPoint {
        line_index,
        column: col,
    })
}

fn is_inside_scrollbar(app: &App, mouse: MouseEvent) -> bool {
    let Some(area) = app.last_scrollbar_area else {
        return false;
    };
    mouse.column >= area.x
        && mouse.column < area.x + area.width
        && mouse.row >= area.y
        && mouse.row < area.y + area.height
}

fn jump_scrollbar(app: &mut App, mouse: MouseEvent) {
    let Some(area) = app.last_scrollbar_area else {
        return;
    };
    if app.last_transcript_total <= app.last_transcript_visible {
        return;
    }

    let rel = usize::from(mouse.row.saturating_sub(area.y));
    let height = usize::from(area.height.max(1));
    let max_start = app
        .last_transcript_total
        .saturating_sub(app.last_transcript_visible)
        .max(1);
    let target = (rel.saturating_mul(max_start) + height / 2) / height;
    if let Some(anchor) = TranscriptScroll::anchor_for(app.transcript_cache.line_meta(), target) {
        app.transcript_scroll = anchor;
    }
}

fn selection_has_content(app: &App) -> bool {
    match app.transcript_selection.ordered_endpoints() {
        Some((start, end)) => start != end,
        None => false,
    }
}

fn copy_active_selection(app: &mut App) {
    if !app.transcript_selection.is_active() {
        return;
    }
    if let Some(text) = selection_to_text(app) {
        if app.clipboard.write_text(&text).is_ok() {
            app.status_message = Some("Selection copied".to_string());
        } else {
            app.status_message = Some("Copy failed".to_string());
        }
    }
}

fn selection_to_text(app: &App) -> Option<String> {
    let (start, end) = app.transcript_selection.ordered_endpoints()?;
    let lines = app.transcript_cache.lines();
    if lines.is_empty() {
        return None;
    }
    let end_index = end.line_index.min(lines.len().saturating_sub(1));
    let start_index = start.line_index.min(end_index);

    let mut out = String::new();
    #[allow(clippy::needless_range_loop)]
    for line_index in start_index..=end_index {
        let line_text = line_to_plain(&lines[line_index]);
        let slice = if start_index == end_index {
            slice_text(&line_text, start.column, end.column)
        } else if line_index == start_index {
            slice_text(&line_text, start.column, line_text.chars().count())
        } else if line_index == end_index {
            slice_text(&line_text, 0, end.column)
        } else {
            line_text
        };
        out.push_str(&slice);
        if line_index != end_index {
            out.push('\n');
        }
    }
    Some(out)
}

fn is_copy_shortcut(key: &KeyEvent) -> bool {
    let is_c = matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'));
    if !is_c {
        return false;
    }

    if key.modifiers.contains(KeyModifiers::SUPER) {
        return true;
    }

    key.modifiers.contains(KeyModifiers::CONTROL) && key.modifiers.contains(KeyModifiers::SHIFT)
}

fn copy_selection_hint() -> &'static str {
    "Release to copy selection"
}

fn should_scroll_with_arrows(_app: &App) -> bool {
    false
}

fn line_to_plain(line: &Line<'static>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
}

fn slice_text(text: &str, start: usize, end: usize) -> String {
    let mut out = String::new();
    let mut idx = 0usize;
    for ch in text.chars() {
        if idx >= start && idx < end {
            out.push(ch);
        }
        idx += 1;
        if idx >= end {
            break;
        }
    }
    out
}

fn render_help_popup(f: &mut Frame, area: Rect, scroll: usize) {
    let popup_width = 65.min(area.width - 4);
    let popup_height = 24.min(area.height - 4);

    let popup_area = Rect {
        x: (area.width - popup_width) / 2,
        y: (area.height - popup_height) / 2,
        width: popup_width,
        height: popup_height,
    };

    f.render_widget(Clear, popup_area);

    // Build all help lines
    let mut help_lines: Vec<Line> = vec![
        Line::from(vec![Span::styled(
            "MiniMax CLI Help",
            Style::default().fg(MINIMAX_RED).bold(),
        )]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Modes:",
            Style::default().fg(MINIMAX_CORAL).bold(),
        )]),
        Line::from("  Tab cycles modes: Normal → Plan → Agent → Yolo → RLM"),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Commands:",
            Style::default().fg(MINIMAX_CORAL).bold(),
        )]),
    ];

    // Add all commands
    for cmd in commands::COMMANDS.iter() {
        help_lines.push(Line::from(format!(
            "  /{:<12} - {}",
            cmd.name, cmd.description
        )));
    }

    help_lines.push(Line::from(""));
    help_lines.push(Line::from(vec![Span::styled(
        "Tools:",
        Style::default().fg(MINIMAX_CORAL).bold(),
    )]));
    help_lines.push(Line::from(
        "  web_search   - Search the web (DuckDuckGo; MCP optional)",
    ));
    help_lines.push(Line::from("  mcp_*        - Tools exposed by MCP servers"));
    help_lines.push(Line::from(""));
    help_lines.push(Line::from(vec![Span::styled(
        "Keys:",
        Style::default().fg(MINIMAX_CORAL).bold(),
    )]));
    help_lines.push(Line::from("  Enter        - Send message"));
    help_lines.push(Line::from("  Esc          - Cancel request"));
    help_lines.push(Line::from("  Tab          - Cycle modes"));
    help_lines.push(Line::from("  Ctrl+C       - Exit"));
    help_lines.push(Line::from("  Up/Down      - Scroll this help"));
    help_lines.push(Line::from(""));

    let total_lines = help_lines.len();
    let visible_lines = (popup_height as usize).saturating_sub(3); // account for border + footer
    let max_scroll = total_lines.saturating_sub(visible_lines);
    let scroll = scroll.min(max_scroll);

    // Show scroll indicator if needed
    let scroll_indicator = if total_lines > visible_lines {
        format!(" [{}/{} ↑↓] ", scroll + 1, max_scroll + 1)
    } else {
        String::new()
    };

    let help = Paragraph::new(help_lines)
        .block(
            Block::default()
                .title(Line::from(vec![Span::styled(
                    " Help ",
                    Style::default().fg(MINIMAX_RED).bold(),
                )]))
                .title_bottom(Line::from(vec![
                    Span::styled(" Esc to close ", Style::default().fg(Color::DarkGray)),
                    Span::styled(scroll_indicator, Style::default().fg(MINIMAX_CORAL)),
                ]))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(MINIMAX_CORAL)),
        )
        .scroll((scroll as u16, 0));

    f.render_widget(help, popup_area);
}

#[allow(clippy::too_many_lines)]
fn render_onboarding(f: &mut Frame, area: Rect, app: &App) {
    // Clear the entire screen with a dark background
    let block = Block::default().style(Style::default().bg(Color::Black));
    f.render_widget(block, area);

    // Center the content
    let content_width = 70.min(area.width.saturating_sub(4));
    let content_height = 24.min(area.height.saturating_sub(4));
    let content_area = Rect {
        x: (area.width - content_width) / 2,
        y: (area.height - content_height) / 2,
        width: content_width,
        height: content_height,
    };

    match app.onboarding {
        OnboardingState::Welcome => {
            let mut lines = vec![];

            // Logo
            for (i, line) in LOGO.lines().enumerate() {
                let color = match i % 3 {
                    0 => MINIMAX_RED,
                    1 => MINIMAX_CORAL,
                    _ => MINIMAX_ORANGE,
                };
                lines.push(Line::from(Span::styled(
                    line,
                    Style::default().fg(color).bold(),
                )));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Welcome to ", Style::default().fg(Color::White)),
                Span::styled("MiniMax CLI", Style::default().fg(MINIMAX_RED).bold()),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Unofficial CLI for MiniMax M2.1 API",
                Style::default().fg(Color::DarkGray).italic(),
            )));
            lines.push(Line::from(Span::styled(
                "Not affiliated with MiniMax Inc.",
                Style::default().fg(Color::DarkGray).italic(),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "To get started, you'll need a MiniMax API key.",
                Style::default().fg(Color::White),
            )));
            lines.push(Line::from(Span::styled(
                "Get yours at: https://platform.minimax.chat",
                Style::default().fg(MINIMAX_CORAL),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::DarkGray)),
                Span::styled("Enter", Style::default().fg(Color::White).bold()),
                Span::styled(
                    " to enter your API key",
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::DarkGray)),
                Span::styled("Ctrl+C", Style::default().fg(Color::White).bold()),
                Span::styled(" to exit", Style::default().fg(Color::DarkGray)),
            ]));

            let paragraph = Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(MINIMAX_RED)),
                )
                .centered();
            f.render_widget(paragraph, content_area);
        }
        OnboardingState::EnteringKey => {
            let mut lines = vec![
                Line::from(Span::styled(
                    "Enter Your API Key",
                    Style::default().fg(MINIMAX_RED).bold(),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Paste your MiniMax API key below:",
                    Style::default().fg(Color::White),
                )),
                Line::from(""),
            ];

            // API key input field (masked)
            let masked_key = if app.api_key_input.is_empty() {
                Span::styled(
                    "(paste your key here)",
                    Style::default().fg(Color::DarkGray).italic(),
                )
            } else {
                // Show first 8 chars, mask the rest
                let visible = app.api_key_input.chars().take(8).collect::<String>();
                let hidden = "*".repeat(app.api_key_input.len().saturating_sub(8));
                Span::styled(
                    format!("{visible}{hidden}"),
                    Style::default().fg(Color::Green),
                )
            };
            lines.push(Line::from(masked_key));
            lines.push(Line::from(""));
            lines.push(Line::from(""));

            // Status message
            if let Some(ref msg) = app.status_message {
                lines.push(Line::from(Span::styled(
                    msg,
                    Style::default().fg(Color::Yellow),
                )));
                lines.push(Line::from(""));
            }

            lines.push(Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::DarkGray)),
                Span::styled("Enter", Style::default().fg(Color::White).bold()),
                Span::styled(" to save", Style::default().fg(Color::DarkGray)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::DarkGray)),
                Span::styled("Esc", Style::default().fg(Color::White).bold()),
                Span::styled(" to go back", Style::default().fg(Color::DarkGray)),
            ]));

            let paragraph = Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(MINIMAX_CORAL)),
                )
                .centered();
            f.render_widget(paragraph, content_area);
        }
        OnboardingState::Success => {
            let lines = vec![
                Line::from(Span::styled(
                    "API Key Saved!",
                    Style::default().fg(Color::Green).bold(),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Your API key has been saved to:",
                    Style::default().fg(Color::White),
                )),
                Line::from(Span::styled(
                    "~/.minimax/config.toml",
                    Style::default().fg(MINIMAX_CORAL),
                )),
                Line::from(""),
                Line::from(""),
                Line::from(Span::styled(
                    "You're all set! Start chatting with MiniMax M2.1",
                    Style::default().fg(Color::White),
                )),
                Line::from(""),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Press ", Style::default().fg(Color::DarkGray)),
                    Span::styled("Enter", Style::default().fg(Color::White).bold()),
                    Span::styled(" to continue", Style::default().fg(Color::DarkGray)),
                ]),
            ];

            let paragraph = Paragraph::new(lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Green)),
                )
                .centered();
            f.render_widget(paragraph, content_area);
        }
        OnboardingState::None => {}
    }
}

fn extract_reasoning_header(text: &str) -> Option<String> {
    let start = text.find("**")?;
    let rest = &text[start + 2..];
    let end = rest.find("**")?;
    let header = rest[..end].trim().trim_end_matches(':');
    if header.is_empty() {
        None
    } else {
        Some(header.to_string())
    }
}

fn format_subagent_list(agents: &[SubAgentResult]) -> String {
    if agents.is_empty() {
        return "No sub-agents running.".to_string();
    }

    let mut lines = Vec::new();
    lines.push("Sub-agents:".to_string());
    lines.push("----------------------------------------".to_string());

    for agent in agents {
        let status = format_subagent_status(&agent.status);
        let mut line = format!(
            "  {} ({:?}) - {} | steps: {} | {}ms",
            agent.agent_id, agent.agent_type, status, agent.steps_taken, agent.duration_ms
        );
        if matches!(agent.status, SubAgentStatus::Completed)
            && let Some(result) = agent.result.as_ref()
        {
            let _ = write!(line, "\n    Result: {}", summarize_tool_output(result));
        }
        lines.push(line);
    }

    lines.join("\n")
}

fn format_subagent_status(status: &SubAgentStatus) -> String {
    match status {
        SubAgentStatus::Running => "running".to_string(),
        SubAgentStatus::Completed => "completed".to_string(),
        SubAgentStatus::Cancelled => "cancelled".to_string(),
        SubAgentStatus::Failed(err) => format!("failed: {}", summarize_tool_output(err)),
    }
}

#[allow(clippy::too_many_lines)]
fn handle_tool_call_started(app: &mut App, id: &str, name: &str, input: &serde_json::Value) {
    let id = id.to_string();
    if is_exploring_tool(name) {
        let label = exploring_label(name, input);
        let cell_index = if let Some(idx) = app.exploring_cell {
            idx
        } else {
            app.add_message(HistoryCell::Tool(ToolCell::Exploring(ExploringCell {
                entries: Vec::new(),
            })));
            let idx = app.history.len().saturating_sub(1);
            app.exploring_cell = Some(idx);
            idx
        };

        if let Some(HistoryCell::Tool(ToolCell::Exploring(cell))) = app.history.get_mut(cell_index)
        {
            let entry_index = cell.insert_entry(ExploringEntry {
                label,
                status: ToolStatus::Running,
            });
            app.mark_history_updated();
            app.exploring_entries
                .insert(id.clone(), (cell_index, entry_index));
        }
        app.tool_cells.insert(id, cell_index);
        return;
    }

    app.exploring_cell = None;

    if is_exec_tool(name) {
        let command = exec_command_from_input(input).unwrap_or_else(|| "<command>".to_string());
        let source = exec_source_from_input(input);
        let interaction = exec_interaction_summary(name, input);
        let mut is_wait = false;

        if let Some((summary, wait)) = interaction.as_ref() {
            is_wait = *wait;
            if is_wait
                && app
                    .last_exec_wait_command
                    .as_ref()
                    .is_some_and(|last| last == &command)
            {
                app.ignored_tool_calls.insert(id);
                return;
            }
            if is_wait {
                app.last_exec_wait_command = Some(command.clone());
            }

            app.add_message(HistoryCell::Tool(ToolCell::Exec(ExecCell {
                command,
                status: ToolStatus::Running,
                output: None,
                started_at: Some(Instant::now()),
                duration_ms: None,
                source,
                interaction: Some(summary.clone()),
            })));
            app.tool_cells
                .insert(id, app.history.len().saturating_sub(1));
            return;
        }

        if exec_is_background(input)
            && app
                .last_exec_wait_command
                .as_ref()
                .is_some_and(|last| last == &command)
        {
            app.ignored_tool_calls.insert(id);
            return;
        }
        if exec_is_background(input) && !is_wait {
            app.last_exec_wait_command = Some(command.clone());
        }

        app.add_message(HistoryCell::Tool(ToolCell::Exec(ExecCell {
            command,
            status: ToolStatus::Running,
            output: None,
            started_at: Some(Instant::now()),
            duration_ms: None,
            source,
            interaction: None,
        })));
        app.tool_cells
            .insert(id, app.history.len().saturating_sub(1));
        return;
    }

    if name == "update_plan" {
        let (explanation, steps) = parse_plan_input(input);
        app.add_message(HistoryCell::Tool(ToolCell::PlanUpdate(PlanUpdateCell {
            explanation,
            steps,
            status: ToolStatus::Running,
        })));
        app.tool_cells
            .insert(id, app.history.len().saturating_sub(1));
        return;
    }

    if name == "apply_patch" {
        let (path, summary) = parse_patch_summary(input);
        app.add_message(HistoryCell::Tool(ToolCell::PatchSummary(
            PatchSummaryCell {
                path,
                summary,
                status: ToolStatus::Running,
                error: None,
            },
        )));
        app.tool_cells
            .insert(id, app.history.len().saturating_sub(1));
        return;
    }

    if is_mcp_tool(name) {
        app.add_message(HistoryCell::Tool(ToolCell::Mcp(McpToolCell {
            tool: name.to_string(),
            status: ToolStatus::Running,
            content: None,
            is_image: false,
        })));
        app.tool_cells
            .insert(id, app.history.len().saturating_sub(1));
        return;
    }

    if is_view_image_tool(name) {
        if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
            let raw_path = PathBuf::from(path);
            let display_path = raw_path
                .strip_prefix(&app.workspace)
                .unwrap_or(&raw_path)
                .to_path_buf();
            app.add_message(HistoryCell::Tool(ToolCell::ViewImage(ViewImageCell {
                path: display_path,
            })));
            app.tool_cells
                .insert(id, app.history.len().saturating_sub(1));
        }
        return;
    }

    if is_web_search_tool(name) {
        let query = web_search_query(input);
        app.add_message(HistoryCell::Tool(ToolCell::WebSearch(WebSearchCell {
            query,
            status: ToolStatus::Running,
            summary: None,
        })));
        app.tool_cells
            .insert(id, app.history.len().saturating_sub(1));
        return;
    }

    let input_summary = summarize_tool_args(input);
    app.add_message(HistoryCell::Tool(ToolCell::Generic(GenericToolCell {
        name: name.to_string(),
        status: ToolStatus::Running,
        input_summary,
        output: None,
    })));
    app.tool_cells
        .insert(id, app.history.len().saturating_sub(1));
}

#[allow(clippy::too_many_lines)]
fn handle_tool_call_complete(
    app: &mut App,
    id: &str,
    _name: &str,
    result: &Result<ToolResult, ToolError>,
) {
    if app.ignored_tool_calls.remove(id) {
        return;
    }

    if let Some((cell_index, entry_index)) = app.exploring_entries.remove(id) {
        if let Some(HistoryCell::Tool(ToolCell::Exploring(cell))) = app.history.get_mut(cell_index)
            && let Some(entry) = cell.entries.get_mut(entry_index)
        {
            entry.status = match result.as_ref() {
                Ok(tool_result) if tool_result.success => ToolStatus::Success,
                Ok(_) | Err(_) => ToolStatus::Failed,
            };
            app.mark_history_updated();
        }
        return;
    }

    let Some(cell_index) = app.tool_cells.remove(id) else {
        return;
    };

    let status = match result.as_ref() {
        Ok(tool_result) => match tool_result.metadata.as_ref() {
            Some(meta)
                if meta
                    .get("status")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| s == "Running") =>
            {
                ToolStatus::Running
            }
            _ => {
                if tool_result.success {
                    ToolStatus::Success
                } else {
                    ToolStatus::Failed
                }
            }
        },
        Err(_) => ToolStatus::Failed,
    };

    if let Some(cell) = app.history.get_mut(cell_index) {
        match cell {
            HistoryCell::Tool(ToolCell::Exec(exec)) => {
                exec.status = status;
                if let Ok(tool_result) = result.as_ref() {
                    exec.duration_ms = tool_result
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("duration_ms"))
                        .and_then(serde_json::Value::as_u64);
                    if status != ToolStatus::Running && exec.interaction.is_none() {
                        exec.output = Some(tool_result.content.clone());
                    }
                } else if let Err(err) = result.as_ref()
                    && exec.interaction.is_none()
                {
                    exec.output = Some(err.to_string());
                }
                app.mark_history_updated();
            }
            HistoryCell::Tool(ToolCell::PlanUpdate(plan)) => {
                plan.status = status;
                app.mark_history_updated();
            }
            HistoryCell::Tool(ToolCell::PatchSummary(patch)) => {
                patch.status = status;
                match result.as_ref() {
                    Ok(tool_result) => {
                        if let Ok(json) =
                            serde_json::from_str::<serde_json::Value>(&tool_result.content)
                            && let Some(message) = json.get("message").and_then(|v| v.as_str())
                        {
                            patch.summary = message.to_string();
                        }
                    }
                    Err(err) => {
                        patch.error = Some(err.to_string());
                    }
                }
                app.mark_history_updated();
            }
            HistoryCell::Tool(ToolCell::Mcp(mcp)) => {
                match result.as_ref() {
                    Ok(tool_result) => {
                        let summary = summarize_mcp_output(&tool_result.content);
                        if summary.is_error == Some(true) {
                            mcp.status = ToolStatus::Failed;
                        } else {
                            mcp.status = status;
                        }
                        mcp.is_image = summary.is_image;
                        mcp.content = summary.content;
                    }
                    Err(err) => {
                        mcp.status = status;
                        mcp.content = Some(err.to_string());
                    }
                }
                app.mark_history_updated();
            }
            HistoryCell::Tool(ToolCell::WebSearch(search)) => {
                search.status = status;
                match result.as_ref() {
                    Ok(tool_result) => {
                        search.summary = Some(summarize_tool_output(&tool_result.content));
                    }
                    Err(err) => {
                        search.summary = Some(err.to_string());
                    }
                }
                app.mark_history_updated();
            }
            HistoryCell::Tool(ToolCell::Generic(generic)) => {
                generic.status = status;
                match result.as_ref() {
                    Ok(tool_result) => {
                        generic.output = Some(summarize_tool_output(&tool_result.content));
                    }
                    Err(err) => {
                        generic.output = Some(err.to_string());
                    }
                }
                app.mark_history_updated();
            }
            _ => {}
        }
    }
}

fn is_exploring_tool(name: &str) -> bool {
    matches!(name, "read_file" | "list_dir" | "grep_files" | "list_files")
}

fn is_exec_tool(name: &str) -> bool {
    matches!(
        name,
        "exec_shell" | "exec_shell_wait" | "exec_shell_interact" | "exec_wait" | "exec_interact"
    )
}

fn exploring_label(name: &str, input: &serde_json::Value) -> String {
    let fallback = format!("{name} tool");
    let obj = input.as_object();
    match name {
        "read_file" => obj
            .and_then(|o| o.get("path"))
            .and_then(|v| v.as_str())
            .map_or(fallback, |path| format!("Read {path}")),
        "list_dir" => obj
            .and_then(|o| o.get("path"))
            .and_then(|v| v.as_str())
            .map_or("List directory".to_string(), |path| format!("List {path}")),
        "grep_files" => {
            let pattern = obj
                .and_then(|o| o.get("pattern"))
                .and_then(|v| v.as_str())
                .unwrap_or("pattern");
            format!("Search {pattern}")
        }
        "list_files" => "List files".to_string(),
        _ => fallback,
    }
}

fn is_mcp_tool(name: &str) -> bool {
    name.starts_with("mcp_")
}

fn is_view_image_tool(name: &str) -> bool {
    matches!(name, "view_image" | "view_image_file" | "view_image_tool")
}

fn is_web_search_tool(name: &str) -> bool {
    matches!(name, "web_search" | "search_web" | "search") || name.ends_with("_web_search")
}

fn web_search_query(input: &serde_json::Value) -> String {
    input
        .get("query")
        .or_else(|| input.get("q"))
        .or_else(|| input.get("search"))
        .and_then(|v| v.as_str())
        .unwrap_or("Web search")
        .to_string()
}

fn parse_plan_input(input: &serde_json::Value) -> (Option<String>, Vec<PlanStep>) {
    let explanation = input
        .get("explanation")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string);
    let mut steps = Vec::new();
    if let Some(items) = input.get("plan").and_then(|v| v.as_array()) {
        for item in items {
            let step = item.get("step").and_then(|v| v.as_str()).unwrap_or("");
            let status = item
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("pending");
            if !step.is_empty() {
                steps.push(PlanStep {
                    step: step.to_string(),
                    status: status.to_string(),
                });
            }
        }
    }
    (explanation, steps)
}

fn parse_patch_summary(input: &serde_json::Value) -> (String, String) {
    let path = input
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("<file>")
        .to_string();
    let patch_text = input.get("patch").and_then(|v| v.as_str()).unwrap_or("");
    let (adds, removes) = count_patch_changes(patch_text);
    let summary = if adds == 0 && removes == 0 {
        "Patch applied".to_string()
    } else {
        format!("Changes: +{adds} / -{removes}")
    };
    (path, summary)
}

fn count_patch_changes(patch: &str) -> (usize, usize) {
    let mut adds = 0;
    let mut removes = 0;
    for line in patch.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            adds += 1;
        } else if line.starts_with('-') {
            removes += 1;
        }
    }
    (adds, removes)
}

fn exec_command_from_input(input: &serde_json::Value) -> Option<String> {
    input
        .get("command")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

fn exec_source_from_input(input: &serde_json::Value) -> ExecSource {
    match input.get("source").and_then(|v| v.as_str()) {
        Some(source) if source.eq_ignore_ascii_case("user") => ExecSource::User,
        _ => ExecSource::Assistant,
    }
}

fn exec_interaction_summary(name: &str, input: &serde_json::Value) -> Option<(String, bool)> {
    let command = exec_command_from_input(input).unwrap_or_else(|| "<command>".to_string());
    let command_display = format!("\"{command}\"");
    let interaction_input = input
        .get("input")
        .or_else(|| input.get("stdin"))
        .or_else(|| input.get("data"))
        .and_then(|v| v.as_str());

    let is_wait_tool = matches!(name, "exec_shell_wait" | "exec_wait");
    let is_interact_tool = matches!(name, "exec_shell_interact" | "exec_interact");

    if is_interact_tool || interaction_input.is_some() {
        let preview = interaction_input.map(summarize_interaction_input);
        let summary = if let Some(preview) = preview {
            format!("Interacted with {command_display}, sent {preview}")
        } else {
            format!("Interacted with {command_display}")
        };
        return Some((summary, false));
    }

    if is_wait_tool || input.get("wait").and_then(serde_json::Value::as_bool) == Some(true) {
        return Some((format!("Waited for {command_display}"), true));
    }

    None
}

fn summarize_interaction_input(input: &str) -> String {
    let mut single_line = input.replace('\r', "");
    single_line = single_line.replace('\n', "\\n");
    single_line = single_line.replace('\"', "'");
    let max_len = 80;
    if single_line.chars().count() <= max_len {
        return format!("\"{single_line}\"");
    }
    let mut out = String::new();
    for ch in single_line.chars().take(max_len.saturating_sub(3)) {
        out.push(ch);
    }
    out.push_str("...");
    format!("\"{out}\"")
}

fn exec_is_background(input: &serde_json::Value) -> bool {
    input
        .get("background")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::tui::app::TuiOptions;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn pad_lines_to_bottom_noop_when_already_filled() {
        let mut lines = vec![Line::from("one"), Line::from("two")];
        pad_lines_to_bottom(&mut lines, 2);
        assert_eq!(lines, vec![Line::from("one"), Line::from("two")]);
    }

    #[test]
    fn pad_lines_to_bottom_prepends_empty_lines() {
        let mut lines = vec![Line::from("one"), Line::from("two")];
        pad_lines_to_bottom(&mut lines, 5);

        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0], Line::from(""));
        assert_eq!(lines[1], Line::from(""));
        assert_eq!(lines[2], Line::from(""));
        assert_eq!(lines[3], Line::from("one"));
        assert_eq!(lines[4], Line::from("two"));
    }

    #[test]
    fn pad_lines_to_bottom_noop_when_height_is_zero() {
        let mut lines = vec![Line::from("one")];
        pad_lines_to_bottom(&mut lines, 0);
        assert_eq!(lines, vec![Line::from("one")]);
    }

    #[test]
    fn selection_point_from_position_ignores_top_padding() {
        let area = Rect {
            x: 10,
            y: 20,
            width: 30,
            height: 5,
        };

        // Content is bottom-aligned: 2 transcript lines in a 5-row viewport.
        let padding_top = 3;
        let transcript_top = 0;
        let transcript_total = 2;

        // Click in padding area -> no selection
        assert!(
            selection_point_from_position(
                area,
                area.x + 1,
                area.y,
                transcript_top,
                transcript_total,
                padding_top,
            )
            .is_none()
        );

        // First transcript line is at row `padding_top`
        let p0 = selection_point_from_position(
            area,
            area.x + 2,
            area.y + u16::try_from(padding_top).unwrap(),
            transcript_top,
            transcript_total,
            padding_top,
        )
        .expect("point");
        assert_eq!(p0.line_index, 0);
        assert_eq!(p0.column, 2);

        // Second transcript line is one row below
        let p1 = selection_point_from_position(
            area,
            area.x,
            area.y + u16::try_from(padding_top + 1).unwrap(),
            transcript_top,
            transcript_total,
            padding_top,
        )
        .expect("point");
        assert_eq!(p1.line_index, 1);
        assert_eq!(p1.column, 0);
    }

    fn make_test_app_with_workspace(workspace: PathBuf) -> App {
        let options = TuiOptions {
            model: "test-model".to_string(),
            workspace,
            allow_shell: false,
            max_subagents: 1,
            skills_dir: PathBuf::from("."),
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
    fn looks_like_rlm_expr_detects_known_functions() {
        assert!(looks_like_rlm_expr("lines(1, 10)"));
        assert!(looks_like_rlm_expr("search(\"foo\")"));
        assert!(looks_like_rlm_expr("vars()"));
        assert!(!looks_like_rlm_expr("read the README"));
    }

    #[test]
    fn rlm_repl_routes_to_chat_when_no_context_loaded() {
        let app = make_test_app_with_workspace(PathBuf::from("."));
        assert!(rlm_repl_should_route_to_chat(
            &app,
            "Please read the README"
        ));
        assert!(!rlm_repl_should_route_to_chat(&app, "lines(1, 5)"));
    }

    #[test]
    fn rlm_repl_stays_in_repl_when_context_exists() {
        let app = make_test_app_with_workspace(PathBuf::from("."));
        {
            let mut session = app.rlm_session.lock().expect("lock session");
            session.load_context("ctx", "hello".to_string(), None);
        }
        assert!(!rlm_repl_should_route_to_chat(
            &app,
            "Please read the README"
        ));
    }

    #[test]
    fn auto_rlm_detects_large_file() {
        let tmp = tempdir().expect("tempdir");
        let big = tmp.path().join("big.txt");
        let content = vec![b'a'; (AUTO_RLM_MIN_FILE_BYTES + 1) as usize];
        fs::write(&big, content).expect("write");

        let app = make_test_app_with_workspace(tmp.path().to_path_buf());
        let decision = auto_rlm_decision(&app, "analyze big.txt", false).expect("decision");
        assert!(matches!(decision.source, AutoRlmSource::File(path) if path == big));
    }

    #[test]
    fn auto_rlm_uses_largest_file_hint() {
        let tmp = tempdir().expect("tempdir");
        let small = tmp.path().join("small.txt");
        let big = tmp.path().join("bigger.txt");
        fs::write(&small, b"tiny").expect("write");
        fs::write(&big, b"this is larger").expect("write");

        let app = make_test_app_with_workspace(tmp.path().to_path_buf());
        let decision =
            auto_rlm_decision(&app, "analyze the largest file", false).expect("decision");
        assert!(matches!(decision.source, AutoRlmSource::File(path) if path == big));
    }

    #[test]
    fn auto_rlm_triggers_on_explicit_request() {
        let tmp = tempdir().expect("tempdir");
        let app = make_test_app_with_workspace(tmp.path().to_path_buf());
        let decision = auto_rlm_decision(&app, "use rlm mode", false).expect("decision");
        assert!(matches!(decision.source, AutoRlmSource::None));
    }

    #[test]
    fn auto_rlm_triggers_on_large_paste() {
        let tmp = tempdir().expect("tempdir");
        let app = make_test_app_with_workspace(tmp.path().to_path_buf());
        let content = "a".repeat(AUTO_RLM_PASTE_MIN_CHARS + 5);
        let input = format!("Summarize this\n\n{content}");
        let decision = auto_rlm_decision(&app, &input, false).expect("decision");
        match decision.source {
            AutoRlmSource::Paste { content, query } => {
                assert!(content.len() >= AUTO_RLM_PASTE_MIN_CHARS);
                assert_eq!(query.as_deref(), Some("Summarize this"));
            }
            _ => panic!("expected paste decision"),
        }
    }
}
