//! Coding API tools for code generation and completion.
//!
//! These tools use the MiniMax Coding API for enhanced code generation capabilities.

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::client::MiniMaxCodingClient;
use crate::config::Config;
use crate::models::{ContentBlock, Message, MessageRequest};
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_str, required_str,
};

/// Tool for code generation using the MiniMax Coding API.
pub struct CodingCompleteTool {
    coding_client: MiniMaxCodingClient,
}

impl CodingCompleteTool {
    /// Create a new CodingCompleteTool.
    pub fn new(coding_client: MiniMaxCodingClient) -> Self {
        Self { coding_client }
    }

    /// Create from config.
    pub fn from_config(config: &Config) -> Result<Self, anyhow::Error> {
        let coding_client = MiniMaxCodingClient::new(config)?;
        Ok(Self::new(coding_client))
    }
}

#[async_trait]
impl ToolSpec for CodingCompleteTool {
    fn name(&self) -> &'static str {
        "coding_complete"
    }

    fn description(&self) -> &'static str {
        "Generate code using MiniMax Coding API. Optimized for code generation with better context handling."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Code generation prompt or partial code to complete"
                },
                "language": {
                    "type": "string",
                    "description": "Programming language (e.g., rust, python, typescript, javascript, go, java)"
                },
                "max_tokens": {
                    "type": "integer",
                    "description": "Maximum tokens to generate (default: 4096)",
                    "default": 4096
                },
                "temperature": {
                    "type": "number",
                    "description": "Temperature for generation (0.0-1.0, default: 0.3)",
                    "default": 0.3
                }
            },
            "required": ["prompt"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let prompt = required_str(&input, "prompt")?;
        let language = optional_str(&input, "language");
        let max_tokens = input
            .get("max_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(4096);
        let temperature = input
            .get("temperature")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or(0.3);

        // Build the prompt with language context if provided
        let full_prompt = if let Some(lang) = language {
            format!(
                "You are an expert {} programmer. Write clean, idiomatic code.\n\n### Task\n{}\n\n### Requirements\n- Use {} best practices\n- Include comments for complex logic\n- Handle edge cases\n- Write production-quality code\n\n```{}",
                lang, prompt, lang, lang
            )
        } else {
            format!(
                "You are an expert programmer. Write clean, production-quality code.\n\n### Task\n{}\n\n### Requirements\n- Include comments for complex logic\n- Handle edge cases\n```",
                prompt
            )
        };

        let request = MessageRequest {
            model: self.coding_client.default_model().to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: full_prompt,
                    cache_control: None,
                }],
            }],
            max_tokens,
            system: None,
            tools: None,
            tool_choice: None,
            metadata: None,
            thinking: None,
            stream: Some(false),
            temperature: Some(temperature),
            top_p: Some(0.95),
        };

        let response = self
            .coding_client
            .create_message(request)
            .await
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;

        // Extract code from response
        let text = extract_code_from_response(&response, language.as_deref());

        Ok(ToolResult::success(text))
    }
}

/// Tool for code review using the MiniMax Coding API.
pub struct CodingReviewTool {
    coding_client: MiniMaxCodingClient,
}

impl CodingReviewTool {
    /// Create a new CodingReviewTool.
    pub fn new(coding_client: MiniMaxCodingClient) -> Self {
        Self { coding_client }
    }

    /// Create from config.
    pub fn from_config(config: &Config) -> Result<Self, anyhow::Error> {
        let coding_client = MiniMaxCodingClient::new(config)?;
        Ok(Self::new(coding_client))
    }
}

#[async_trait]
impl ToolSpec for CodingReviewTool {
    fn name(&self) -> &'static str {
        "coding_review"
    }

    fn description(&self) -> &'static str {
        "Review code using MiniMax Coding API. Identifies bugs, style issues, and improvements."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "code": {
                    "type": "string",
                    "description": "Code to review"
                },
                "language": {
                    "type": "string",
                    "description": "Programming language"
                },
                "focus": {
                    "type": "string",
                    "description": "Review focus: bugs, style, security, performance, or all (default: all)"
                }
            },
            "required": ["code"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let code = required_str(&input, "code")?;
        let language = optional_str(&input, "language");
        let focus = optional_str(&input, "focus").unwrap_or("all");

        let full_prompt = format!(
            "You are an expert code reviewer. Review the following code.\n\n\
            ### Code\n```{}\n{}\n```\n\n\
            ### Review Focus: {}\n\n\
            Please provide:\n\
            1. Summary of issues found (by severity: critical, major, minor)\n\
            2. Specific line numbers and suggestions\n\
            3. Overall code quality score (0-10)\n\
            4. Recommended improvements",
            language.as_deref().unwrap_or("text"),
            code,
            focus
        );

        let request = MessageRequest {
            model: self.coding_client.default_model().to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: full_prompt,
                    cache_control: None,
                }],
            }],
            max_tokens: 4096,
            system: None,
            tools: None,
            tool_choice: None,
            metadata: None,
            thinking: None,
            stream: Some(false),
            temperature: Some(0.3),
            top_p: Some(0.9),
        };

        let response = self
            .coding_client
            .create_message(request)
            .await
            .map_err(|e| ToolError::execution_failed(e.to_string()))?;

        let text = extract_text_from_response(&response);

        Ok(ToolResult::success(text))
    }
}

/// Extract code from API response, handling markdown code blocks.
fn extract_code_from_response(
    response: &crate::models::MessageResponse,
    language: Option<&str>,
) -> String {
    let text = extract_text_from_response(response);

    // Try to extract code from markdown code blocks
    let code_block_pattern = if let Some(lang) = language {
        format!(r"```{}\s*\n([\s\S]*?)\n```", lang)
    } else {
        r"```[\w]*\s*\n([\s\S]*?)\n```".to_string()
    };

    if let Ok(regex) = regex::Regex::new(&code_block_pattern) {
        if let Some(captures) = regex.captures(&text) {
            if let Some(code_match) = captures.get(1) {
                return code_match.as_str().trim().to_string();
            }
        }
    }

    // Fallback: try generic code block
    if let Ok(regex) = regex::Regex::new(r"```\s*\n([\s\S]*?)\n```") {
        if let Some(captures) = regex.captures(&text) {
            if let Some(code_match) = captures.get(1) {
                return code_match.as_str().trim().to_string();
            }
        }
    }

    // Return as-is if no code block found
    text
}

/// Extract text content from API response.
fn extract_text_from_response(response: &crate::models::MessageResponse) -> String {
    response
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text, .. } => Some(text.clone()),
            ContentBlock::Thinking { .. } => None,
            ContentBlock::ToolUse { .. } => None,
            ContentBlock::ToolResult { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
