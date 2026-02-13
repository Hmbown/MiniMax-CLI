//! Context compaction for long conversations.
//!
//! This module provides automatic context management for extended conversations:
//! - Token estimation using character-based approximation
//! - Automatic compaction when conversations exceed thresholds
//! - Summary-based context reduction preserving key information
//!
//! Integration: Call `maybe_compact()` before each API request to check if
//! compaction is needed. If so, it will summarize older messages and return
//! a compacted message history.

use crate::logging;
use anyhow::Result;
use std::fmt::Write;

use crate::client::MiniMaxTextClient;
use crate::models::{
    CacheControl, ContentBlock, Message, MessageRequest, SystemBlock, SystemPrompt, Tool,
};

/// Configuration for conversation compaction behavior.
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    pub enabled: bool,
    pub token_threshold: usize,
    pub message_threshold: usize,
    pub model: Option<String>,
    pub cache_summary: bool,
    pub compact_prompt: Option<String>,
    /// Recompute token threshold from active model context window each turn.
    pub derive_token_threshold_from_model: bool,
    /// Keep this many recent messages unsummarized
    pub keep_recent: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,          // Enable by default for better UX
            token_threshold: 80000, // 80K tokens ~ 320K chars
            message_threshold: 30,  // After 30 messages
            model: None,
            cache_summary: true,
            compact_prompt: None,
            derive_token_threshold_from_model: false,
            keep_recent: 6, // Keep last 6 messages as-is
        }
    }
}

pub fn estimate_tokens(messages: &[Message]) -> usize {
    // Better estimate: varies by content type
    // - English text: ~4 chars per token
    // - Code: ~3.5 chars per token
    // - JSON/tool data: ~2.5 chars per token
    messages
        .iter()
        .map(|m| {
            m.content
                .iter()
                .map(|c| match c {
                    ContentBlock::Text { text, .. } => {
                        // Estimate: check if it looks like code (many brackets/parens)
                        let is_code = text
                            .chars()
                            .filter(|&c| {
                                c == '{' || c == '}' || c == '[' || c == ']' || c == '(' || c == ')'
                            })
                            .count()
                            > text.len() / 20;
                        if is_code {
                            text.len() / 3
                        } else {
                            text.len() / 4
                        }
                    }
                    ContentBlock::Thinking { thinking } => thinking.len() / 4,
                    ContentBlock::ToolUse { input, .. } => {
                        let json = serde_json::to_string(input).unwrap_or_default();
                        // JSON is more token-dense
                        json.len() / 2
                    }
                    ContentBlock::ToolResult { content, .. } => content.len() / 4,
                })
                .sum::<usize>()
        })
        .sum()
}

/// Estimate tokens for system prompt content
pub fn estimate_system_tokens(system: &Option<SystemPrompt>) -> usize {
    match system {
        Some(SystemPrompt::Text(text)) => text.len() / 4,
        Some(SystemPrompt::Blocks(blocks)) => blocks.iter().map(|b| b.text.len() / 4).sum(),
        None => 0,
    }
}

/// Estimate tokens for tools array
pub fn estimate_tools_tokens(tools: &Option<Vec<Tool>>) -> usize {
    match tools {
        Some(tools) => tools
            .iter()
            .map(|t| {
                // Tool name, description, and input schema
                t.name.len() / 4
                    + t.description.len() / 4
                    + serde_json::to_string(&t.input_schema)
                        .map(|s| s.len() / 4)
                        .unwrap_or(0)
            })
            .sum(),
        None => 0,
    }
}

/// Total estimated tokens for a request (messages + system + tools)
pub fn estimate_request_tokens(
    messages: &[Message],
    system: &Option<SystemPrompt>,
    tools: &Option<Vec<Tool>>,
) -> usize {
    estimate_tokens(messages) + estimate_system_tokens(system) + estimate_tools_tokens(tools)
}

#[allow(dead_code)]
pub fn should_compact(messages: &[Message], config: &CompactionConfig) -> bool {
    if !config.enabled {
        return false;
    }

    let token_estimate = estimate_tokens(messages);
    let message_count = messages.len();

    token_estimate > config.token_threshold || message_count > config.message_threshold
}

pub async fn compact_messages(
    client: &MiniMaxTextClient,
    messages: &[Message],
    config: &CompactionConfig,
    active_model: &str,
) -> Result<(Vec<Message>, Option<SystemPrompt>)> {
    if messages.is_empty() {
        return Ok((Vec::new(), None));
    }

    // Keep the last few messages as-is (use config setting)
    let keep_recent = config.keep_recent;
    let (to_summarize, recent) = if messages.len() <= keep_recent {
        return Ok((messages.to_vec(), None));
    } else {
        let split_point = messages.len() - keep_recent;
        (&messages[..split_point], &messages[split_point..])
    };

    // Create a summary of older messages
    let summary_model = config.model.as_deref().unwrap_or(active_model);
    let summary = create_summary(
        client,
        to_summarize,
        summary_model,
        config.compact_prompt.as_deref(),
    )
    .await?;

    // Build new message list with summary as system block
    let summary_block = SystemBlock {
        block_type: "text".to_string(),
        text: format!(
            "## Conversation Summary\n\nThe following is a summary of the earlier conversation:\n\n{summary}\n\n---\nRecent messages follow:"
        ),
        cache_control: if config.cache_summary {
            Some(CacheControl {
                cache_type: "ephemeral".to_string(),
            })
        } else {
            None
        },
    };

    Ok((
        recent.to_vec(),
        Some(SystemPrompt::Blocks(vec![summary_block])),
    ))
}

async fn create_summary(
    client: &MiniMaxTextClient,
    messages: &[Message],
    model: &str,
    compact_prompt: Option<&str>,
) -> Result<String> {
    // Format messages for summarization
    let mut conversation_text = String::new();
    for msg in messages {
        let role = if msg.role == "user" {
            "User"
        } else {
            "Assistant"
        };
        for block in &msg.content {
            match block {
                ContentBlock::Text { text, .. } => {
                    let _ = write!(conversation_text, "{role}: {text}\n\n");
                }
                ContentBlock::ToolUse { name, .. } => {
                    let _ = write!(conversation_text, "{role}: [Used tool: {name}]\n\n");
                }
                ContentBlock::ToolResult { content, .. } => {
                    let _ = write!(
                        conversation_text,
                        "Tool result: {}\n\n",
                        &content[..500.min(content.len())]
                    );
                }
                ContentBlock::Thinking { .. } => {
                    // Skip thinking blocks in summary
                }
            }
        }
    }

    const DEFAULT_COMPACT_PROMPT: &str = "Summarize the following conversation in a concise but comprehensive way. \
         Preserve key information, decisions made, and any important context. \
         Keep it under 500 words.";
    let prompt = compact_prompt
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .unwrap_or(DEFAULT_COMPACT_PROMPT);

    let request = MessageRequest {
        model: model.to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: format!("{prompt}\n\n---\n\n{conversation_text}"),
                cache_control: None,
            }],
        }],
        max_tokens: 1024,
        system: Some(SystemPrompt::Text(
            "You are a helpful assistant that creates concise conversation summaries.".to_string(),
        )),
        tools: None,
        tool_choice: None,
        metadata: None,
        thinking: None,
        stream: Some(false),
        temperature: Some(0.3),
        top_p: None,
    };

    let response = client.create_message(request).await?;

    // Extract text from response
    let summary = response
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(summary)
}

pub fn merge_system_prompts(
    original: Option<&SystemPrompt>,
    summary: Option<SystemPrompt>,
) -> Option<SystemPrompt> {
    match (original, summary) {
        (None, None) => None,
        (Some(orig), None) => Some(orig.clone()),
        (None, Some(sum)) => Some(sum),
        (Some(SystemPrompt::Text(orig_text)), Some(SystemPrompt::Blocks(mut sum_blocks))) => {
            // Prepend original system prompt
            sum_blocks.insert(
                0,
                SystemBlock {
                    block_type: "text".to_string(),
                    text: orig_text.clone(),
                    cache_control: None,
                },
            );
            Some(SystemPrompt::Blocks(sum_blocks))
        }
        (Some(SystemPrompt::Blocks(orig_blocks)), Some(SystemPrompt::Blocks(mut sum_blocks))) => {
            // Prepend original blocks
            for (i, block) in orig_blocks.iter().enumerate() {
                sum_blocks.insert(i, block.clone());
            }
            Some(SystemPrompt::Blocks(sum_blocks))
        }
        (Some(orig), Some(SystemPrompt::Text(sum_text))) => {
            let mut blocks = match orig {
                SystemPrompt::Text(t) => vec![SystemBlock {
                    block_type: "text".to_string(),
                    text: t.clone(),
                    cache_control: None,
                }],
                SystemPrompt::Blocks(b) => b.clone(),
            };
            blocks.push(SystemBlock {
                block_type: "text".to_string(),
                text: sum_text,
                cache_control: None,
            });
            Some(SystemPrompt::Blocks(blocks))
        }
    }
}

/// Integration helper: Check if compaction is needed and apply it.
///
/// Returns a tuple of (compacted_messages, new_system_prompt, was_compacted)
/// If compaction is not needed, returns (messages.clone(), system.clone(), false)
///
/// # Arguments
/// * `client` - MiniMaxTextClient for making summary API calls
/// * `messages` - Current conversation messages
/// * `system` - Current system prompt
/// * `tools` - Current tools (for token estimation)
/// * `config` - Compaction configuration
pub async fn maybe_compact(
    client: &MiniMaxTextClient,
    messages: &[Message],
    system: &Option<SystemPrompt>,
    tools: &Option<Vec<Tool>>,
    config: &CompactionConfig,
    active_model: &str,
) -> Result<(Vec<Message>, Option<SystemPrompt>, bool)> {
    if !config.enabled {
        return Ok((messages.to_vec(), system.clone(), false));
    }

    // Estimate total request tokens
    let total_tokens = estimate_request_tokens(messages, system, tools);
    let message_count = messages.len();

    let should_compact =
        total_tokens > config.token_threshold || message_count > config.message_threshold;

    if !should_compact {
        return Ok((messages.to_vec(), system.clone(), false));
    }

    // Log compaction attempt
    logging::info(format!(
        "Context compaction triggered: {} tokens (threshold: {}), {} messages (threshold: {})",
        total_tokens, config.token_threshold, message_count, config.message_threshold
    ));

    // Perform compaction
    let (compacted_messages, summary_prompt) =
        compact_messages(client, messages, config, active_model).await?;

    // Merge with original system prompt
    let merged_system = merge_system_prompts(system.as_ref(), summary_prompt);

    // Count compacted messages
    let original_count = messages.len();
    let new_count = compacted_messages.len();

    logging::info(format!(
        "Compaction complete: {} messages -> {} messages (kept {} recent)",
        original_count, new_count, config.keep_recent
    ));

    Ok((compacted_messages, merged_system, true))
}
