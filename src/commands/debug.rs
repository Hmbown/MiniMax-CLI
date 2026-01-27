//! Debug commands: tokens, cost, system, context, undo, retry, debug

use super::CommandResult;
use crate::models::{SystemPrompt, context_window_for_model};
use crate::pricing;
use crate::tui::app::{App, AppAction, AppMode};
use crate::tui::history::HistoryCell;
use crate::utils::estimate_message_chars;

/// Show token usage for session
pub fn tokens(app: &mut App) -> CommandResult {
    let message_count = app.api_messages.len();
    let chat_count = app.history.len();

    CommandResult::message(format!(
        "Token Usage:\n\
         ─────────────────────────────\n\
         Total tokens:     {}\n\
         Session cost:     ${:.4}\n\
         API messages:     {}\n\
         Chat messages:    {}\n\
         Model:            {}",
        app.total_tokens, app.session_cost, message_count, chat_count, app.model,
    ))
}

/// Show session cost breakdown
pub fn cost(app: &mut App) -> CommandResult {
    CommandResult::message(format!(
        "Session Cost:\n\
         ─────────────────────────────\n\
         Total spent:      ${:.4}\n\n\
         MiniMax API Pricing:\n\
         ─────────────────────────────\n\
         Image generation: ${:.4}/image\n\
         Audio TTS (HD):   ${:.5}/char\n\
         Video (768P 6s):  ${:.2}\n\
         Video (1080P 6s): ${:.2}\n\
         Music (per 5min): ${:.2}\n\
         Voice cloning:    ${:.2}/voice\n\n\
         Cost is tracked when paid tools are executed.",
        app.session_cost,
        pricing::prices::IMAGE_PER_UNIT,
        pricing::prices::AUDIO_HD_PER_CHAR,
        pricing::prices::VIDEO_768P_6S,
        pricing::prices::VIDEO_1080P_6S,
        pricing::prices::MUSIC_PER_5MIN,
        pricing::prices::VOICE_CLONE,
    ))
}

/// Show current system prompt
pub fn system_prompt(app: &mut App) -> CommandResult {
    let prompt_text = match &app.system_prompt {
        Some(SystemPrompt::Text(text)) => text.clone(),
        Some(SystemPrompt::Blocks(blocks)) => blocks
            .iter()
            .map(|b| b.text.clone())
            .collect::<Vec<_>>()
            .join("\n\n---\n\n"),
        None => "(no system prompt)".to_string(),
    };

    // Truncate if too long
    let display = if prompt_text.len() > 500 {
        // Find a valid UTF-8 char boundary at or before byte 500
        let truncate_at = prompt_text
            .char_indices()
            .take_while(|(i, _)| *i <= 500)
            .last()
            .map_or(0, |(i, _)| i);
        format!(
            "{}...\n\n(truncated, {} chars total)",
            &prompt_text[..truncate_at],
            prompt_text.len()
        )
    } else {
        prompt_text
    };

    CommandResult::message(format!(
        "System Prompt ({} mode):\n─────────────────────────────\n{}",
        app.mode.label(),
        display
    ))
}

/// Show context window usage
pub fn context(app: &mut App) -> CommandResult {
    let mut total_chars = estimate_message_chars(&app.api_messages);

    // System prompt
    if let Some(SystemPrompt::Text(text)) = &app.system_prompt {
        total_chars += text.len();
    } else if let Some(SystemPrompt::Blocks(blocks)) = &app.system_prompt {
        for block in blocks {
            total_chars += block.text.len();
        }
    }

    // Rough token estimate (4 chars per token on average)
    let estimated_tokens = total_chars / 4;

    let context_size = context_window_for_model(&app.model).unwrap_or(128_000);
    let estimated_tokens_u32 = u32::try_from(estimated_tokens).unwrap_or(u32::MAX);
    let usage_pct = (f64::from(estimated_tokens_u32) / f64::from(context_size) * 100.0).min(100.0);

    CommandResult::message(format!(
        "Context Usage:\n\
         ─────────────────────────────\n\
         Characters:       {}\n\
         Estimated tokens: ~{}\n\
         Context window:   {}\n\
         Usage:            {:.1}%\n\n\
         Messages:         {}\n\
         API messages:     {}",
        total_chars,
        estimated_tokens,
        context_size,
        usage_pct,
        app.history.len(),
        app.api_messages.len(),
    ))
}

/// Remove last message pair (user + assistant)
pub fn undo(app: &mut App) -> CommandResult {
    // Remove from display history (up to the last user message)
    let mut removed_count = 0;
    while !app.history.is_empty() {
        let last_is_user = matches!(app.history.last(), Some(HistoryCell::User { .. }));
        app.history.pop();
        removed_count += 1;
        if last_is_user {
            break;
        }
    }

    // Remove from API messages
    while let Some(last) = app.api_messages.last() {
        if last.role == "user" {
            app.api_messages.pop();
            break;
        }
        app.api_messages.pop();
    }

    if removed_count > 0 {
        app.mark_history_updated();
        CommandResult::message(format!("Removed {removed_count} message(s)"))
    } else {
        CommandResult::message("Nothing to undo")
    }
}

/// Retry last request - remove last exchange and re-send the user's message
pub fn retry(app: &mut App) -> CommandResult {
    let last_user_input = app.history.iter().rev().find_map(|cell| match cell {
        HistoryCell::User { content } => Some(content.clone()),
        _ => None,
    });

    match last_user_input {
        Some(input) => {
            undo(app);
            let display_input = if input.len() > 50 {
                let truncate_at = input
                    .char_indices()
                    .take_while(|(i, _)| *i <= 50)
                    .last()
                    .map_or(0, |(i, _)| i);
                format!("{}...", &input[..truncate_at])
            } else {
                input.clone()
            };
            CommandResult::with_message_and_action(
                format!("Retrying: {display_input}"),
                AppAction::SendMessage(input),
            )
        }
        None => CommandResult::error("No previous request to retry"),
    }
}

/// Show comprehensive debug information
pub fn debug_info(app: &mut App) -> CommandResult {
    let mut output = String::new();

    // Session info
    output.push_str("Debug Information\n");
    output.push_str("═══════════════════════════════════════\n\n");

    // Session
    output.push_str("Session:\n");
    output.push_str(&format!(
        "  Session ID:     {}\n",
        app.current_session_id.as_deref().unwrap_or("(none)")
    ));
    output.push_str(&format!("  Messages:       {}\n", app.api_messages.len()));
    output.push_str(&format!("  History cells:  {}\n", app.history.len()));
    output.push_str(&format!("  Total tokens:   {}\n", app.total_tokens));
    output.push_str(&format!("  Session cost:   ${:.4}\n", app.session_cost));
    output.push_str(&format!("  Mode:           {:?}\n", app.mode));
    output.push_str(&format!("  Model:          {}\n\n", app.model));

    // Context
    let context_size = context_window_for_model(&app.model).unwrap_or(128_000);
    let total_chars = estimate_message_chars(&app.api_messages);
    let estimated_tokens = total_chars / 4;
    let usage_pct = (estimated_tokens as f64 / context_size as f64 * 100.0).min(100.0);

    output.push_str("Context:\n");
    output.push_str(&format!("  Context window: {}\n", context_size));
    output.push_str(&format!("  Characters:     {}\n", total_chars));
    output.push_str(&format!("  Est. tokens:    ~{}\n", estimated_tokens));
    output.push_str(&format!("  Usage:          {:.1}%\n\n", usage_pct));

    // Settings
    output.push_str("Settings:\n");
    output.push_str(&format!("  Auto-compact:   {}\n", app.auto_compact));
    output.push_str(&format!("  Show thinking:  {}\n", app.show_thinking));
    output.push_str(&format!("  Show tools:     {}\n", app.show_tool_details));
    output.push_str(&format!("  Shell allowed:  {}\n", app.allow_shell));
    output.push_str(&format!("  Trust mode:     {}\n", app.trust_mode));
    output.push_str(&format!("  Approval mode:  {:?}\n", app.approval_mode));
    output.push_str(&format!("  Max subagents:  {}\n\n", app.max_subagents));

    // Workspace
    output.push_str("Workspace:\n");
    output.push_str(&format!("  Path:           {}\n", app.workspace.display()));
    output.push_str(&format!("  Skills dir:     {}\n", app.skills_dir.display()));
    output.push_str(&format!(
        "  Project doc:    {}\n\n",
        if app.project_doc.is_some() {
            "loaded"
        } else {
            "none"
        }
    ));

    // Recent activity
    if !app.recent_files.is_empty() {
        output.push_str("Recent files:\n");
        for (i, file) in app.recent_files.iter().take(5).enumerate() {
            output.push_str(&format!("  {}. {}\n", i + 1, file.display()));
        }
        output.push('\n');
    }

    // RLM status (if in RLM mode)
    if app.mode == AppMode::Rlm
        && let Ok(session) = app.rlm_session.lock()
    {
        output.push_str("RLM Session:\n");
        output.push_str(&format!("  Contexts:       {}\n", session.context_count()));
        output.push_str(&format!(
            "  Total lines:    {}\n\n",
            session.total_line_count()
        ));
    }

    // Queued messages
    if !app.queued_messages.is_empty() {
        output.push_str(&format!(
            "Queued messages:  {}\n\n",
            app.queued_messages.len()
        ));
    }

    // System prompt preview
    output.push_str("System prompt:\n");
    let prompt_preview = match &app.system_prompt {
        Some(SystemPrompt::Text(text)) => {
            if text.len() > 200 {
                format!("{}... ({} chars)", &text[..200], text.len())
            } else {
                text.clone()
            }
        }
        Some(SystemPrompt::Blocks(blocks)) => {
            format!("{} blocks", blocks.len())
        }
        None => "(none)".to_string(),
    };
    output.push_str(&format!("  {}\n", prompt_preview));

    CommandResult::message(output)
}
