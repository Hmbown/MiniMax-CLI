//! API request/response models for `MiniMax` endpoints.

use serde::{Deserialize, Serialize};

// === Core Message Types ===

/// Request payload for sending a message to the API.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MessageRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
}

/// System prompt representation (plain text or structured blocks).
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum SystemPrompt {
    Text(String),
    Blocks(Vec<SystemBlock>),
}

/// A structured system prompt block.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// A chat message with role and content blocks.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: Vec<ContentBlock>,
}

/// A single content block inside a message.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

/// Cache control metadata for tool definitions and blocks.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub cache_type: String,
}

/// Tool definition exposed to the model.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// Response payload for a message request.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MessageResponse {
    pub id: String,
    pub r#type: String,
    pub role: String,
    pub content: Vec<ContentBlock>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    pub usage: Usage,
}

/// Token usage metadata for a response.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// Map known models to their approximate context window sizes.
#[must_use]
pub fn context_window_for_model(model: &str) -> Option<u32> {
    let lower = model.to_lowercase();
    // MiniMax M2.5 series (2M context)
    if lower.contains("minimax-m2.5") || lower.contains("minimax-2.5") || lower.contains("m2.5") {
        return Some(2_000_000);
    }
    // MiniMax M2.1 (1M context)
    if lower.contains("minimax-m2.1") || lower.contains("m2.1") {
        return Some(1_000_000);
    }
    // MiniMax M2 (efficient agentic model - assume similar to M2.1)
    if lower.contains("minimax-m2") && !lower.contains("2.1") && !lower.contains("2.5") {
        return Some(1_000_000);
    }
    // MiniMax Text-01 (256K context)
    if lower.contains("minimax-text-01") {
        return Some(256_000);
    }
    // MiniMax Coding-01 (128K context)
    if lower.contains("minimax-coding-01") {
        return Some(128_000);
    }
    // Claude models
    if lower.contains("claude") {
        return Some(200_000);
    }
    // Gemini models
    if lower.contains("gemini") {
        return Some(2_000_000);
    }
    None
}

// === Streaming Structures ===

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
/// Streaming event types for SSE responses.
pub enum StreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: MessageResponse },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: u32,
        content_block: ContentBlockStart,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: u32, delta: Delta },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: u32 },
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: MessageDelta,
        usage: Option<Usage>,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
/// Content block types used in streaming starts.
pub enum ContentBlockStart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value, // usually empty or partial
    },
}

// Variant names match the compatibility API schema, suppressing style warning
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
/// Delta events emitted during streaming responses.
pub enum Delta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { thinking: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
/// Delta payload for message-level updates.
pub struct MessageDelta {
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::context_window_for_model;

    #[test]
    fn maps_minimax_m2_5_and_aliases() {
        assert_eq!(context_window_for_model("MiniMax-M2.5"), Some(2_000_000));
        assert_eq!(context_window_for_model("MiniMax-2.5"), Some(2_000_000));
        assert_eq!(context_window_for_model("m2.5"), Some(2_000_000));
        assert_eq!(
            context_window_for_model("MiniMax-M2.5-lightning"),
            Some(2_000_000)
        );
    }

    #[test]
    fn maps_minimax_m2_and_m2_1() {
        assert_eq!(context_window_for_model("MiniMax-M2.1"), Some(1_000_000));
        assert_eq!(context_window_for_model("MiniMax-M2"), Some(1_000_000));
        assert_eq!(context_window_for_model("m2.1"), Some(1_000_000));
    }
}
