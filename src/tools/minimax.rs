//! `MiniMax` API tools: TTS, image generation, video generation, music generation, file operations
//! plus voice management and video template generation.
//!
//! These tools provide access to `MiniMax` APIs for multimodal content creation.

use async_trait::async_trait;
use base64::Engine;
use serde_json::{Value, json};

use crate::client::MiniMaxClient;
use crate::config::Config;
use crate::modules::{audio, files, image, music, video};
use crate::tools::spec::{
    ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec, optional_str, required_str,
};
use crate::utils::pretty_json;

// === Helpers ===

fn optional_json_string(input: &Value, field: &str) -> Option<String> {
    let value = input.get(field)?;
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).ok(),
        _ => Some(value.to_string()),
    }
}

fn optional_string_vec(input: &Value, field: &str) -> Vec<String> {
    input
        .get(field)
        .and_then(|v| v.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(std::string::ToString::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn extract_chat_text(response: &Value) -> Option<String> {
    let choices = response.get("choices")?.as_array()?;
    let choice = choices.first()?;
    if let Some(message) = choice.get("message")
        && let Some(content) = message.get("content")
    {
        if let Some(text) = content.as_str() {
            return Some(text.to_string());
        }
        if let Some(items) = content.as_array() {
            let mut parts = Vec::new();
            for item in items {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    parts.push(text);
                }
            }
            if !parts.is_empty() {
                return Some(parts.join("\n"));
            }
        }
    }
    if let Some(text) = choice.get("text").and_then(|v| v.as_str()) {
        return Some(text.to_string());
    }
    None
}

// === Audio TTS Tool ===

/// Tool for text-to-speech conversion using `MiniMax` TTS.
pub struct TtsTool;

#[async_trait]
impl ToolSpec for TtsTool {
    fn name(&self) -> &'static str {
        "tts"
    }

    fn description(&self) -> &'static str {
        "Convert text to speech using MiniMax TTS. Returns the path to the saved audio file."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to convert to speech"
                },
                "model": {
                    "type": "string",
                    "description": "TTS model name (default: speech-02-hd). Options: speech-02-hd, speech-02-turbo, speech-01-hd, speech-01-turbo, speech-2.6",
                    "default": "speech-02-hd"
                },
                "voice_id": {
                    "type": "string",
                    "description": "Voice ID to use (optional)"
                },
                "output_format": {
                    "type": "string",
                    "description": "Output format value to request from MiniMax (see MiniMax docs)"
                },
                "stream": {
                    "type": "boolean",
                    "description": "Stream audio response if true",
                    "default": false
                },
                "voice_setting_json": {
                    "type": ["string", "object"],
                    "description": "JSON for voice_setting overrides"
                },
                "audio_setting_json": {
                    "type": ["string", "object"],
                    "description": "JSON for audio_setting overrides"
                },
                "pronunciation_dict_json": {
                    "type": ["string", "object"],
                    "description": "JSON pronunciation dictionary"
                },
                "timber_weights_json": {
                    "type": ["string", "object"],
                    "description": "JSON timbre weights"
                },
                "language_boost_json": {
                    "type": ["string", "object"],
                    "description": "JSON language boost settings"
                },
                "voice_modify_json": {
                    "type": ["string", "object"],
                    "description": "JSON voice modify settings"
                },
                "subtitle_enable": {
                    "type": "boolean",
                    "description": "Enable subtitle generation"
                }
            },
            "required": ["text"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network, ToolCapability::WritesFiles]
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let text = required_str(&input, "text")?;
        let model = optional_str(&input, "model")
            .unwrap_or("speech-02-hd")
            .to_string();
        let voice_id = optional_str(&input, "voice_id").map(std::string::ToString::to_string);
        let output_format = optional_str(&input, "output_format")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let stream = input
            .get("stream")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let voice_setting_json = optional_json_string(&input, "voice_setting_json");
        let audio_setting_json = optional_json_string(&input, "audio_setting_json");
        let pronunciation_dict_json = optional_json_string(&input, "pronunciation_dict_json");
        let timber_weights_json = optional_json_string(&input, "timber_weights_json");
        let language_boost_json = optional_json_string(&input, "language_boost_json");
        let voice_modify_json = optional_json_string(&input, "voice_modify_json");
        let subtitle_enable = input
            .get("subtitle_enable")
            .and_then(serde_json::Value::as_bool);

        let client = create_minimax_client()?;
        let output_dir = context.workspace.clone();

        let options = audio::T2aOptions {
            model,
            text: text.to_string(),
            stream,
            output_format,
            voice_id,
            voice_setting_json,
            audio_setting_json,
            pronunciation_dict_json,
            timber_weights_json,
            language_boost_json,
            voice_modify_json,
            subtitle_enable,
            output_dir,
        };

        match audio::t2a(&client, options).await {
            Ok(path) => Ok(ToolResult::success(format!(
                "TTS audio saved to {}",
                path.display()
            ))),
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to generate TTS audio: {e}"
            ))),
        }
    }
}

// === Async TTS Tools ===

/// Tool for creating async text-to-speech tasks using `MiniMax`.
pub struct TtsAsyncCreateTool;

#[async_trait]
impl ToolSpec for TtsAsyncCreateTool {
    fn name(&self) -> &'static str {
        "tts_async_create"
    }

    fn description(&self) -> &'static str {
        "Create an async text-to-speech task using MiniMax. Returns task metadata."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "model": {
                    "type": "string",
                    "description": "TTS model name (default: speech-02-hd). Options: speech-02-hd, speech-02-turbo, speech-01-hd, speech-01-turbo, speech-2.6",
                    "default": "speech-02-hd"
                },
                "text": {
                    "type": "string",
                    "description": "Text to convert to speech"
                },
                "text_file_id": {
                    "type": "string",
                    "description": "File ID containing text to convert"
                },
                "voice_id": {
                    "type": "string",
                    "description": "Voice ID to use (optional)"
                },
                "voice_setting_json": {
                    "type": ["string", "object"],
                    "description": "JSON for voice_setting overrides"
                },
                "audio_setting_json": {
                    "type": ["string", "object"],
                    "description": "JSON for audio_setting overrides"
                },
                "pronunciation_dict_json": {
                    "type": ["string", "object"],
                    "description": "JSON pronunciation dictionary"
                },
                "language_boost_json": {
                    "type": ["string", "object"],
                    "description": "JSON language boost settings"
                },
                "voice_modify_json": {
                    "type": ["string", "object"],
                    "description": "JSON voice modify settings"
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network]
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let model = optional_str(&input, "model")
            .unwrap_or("speech-02-hd")
            .to_string();
        let text = optional_str(&input, "text").map(std::string::ToString::to_string);
        let text_file_id =
            optional_str(&input, "text_file_id").map(std::string::ToString::to_string);
        if text.is_none() && text_file_id.is_none() {
            return Err(ToolError::invalid_input(
                "Provide either text or text_file_id for async TTS.",
            ));
        }
        let voice_id = optional_str(&input, "voice_id").map(std::string::ToString::to_string);
        let voice_setting_json = optional_json_string(&input, "voice_setting_json");
        let audio_setting_json = optional_json_string(&input, "audio_setting_json");
        let pronunciation_dict_json = optional_json_string(&input, "pronunciation_dict_json");
        let language_boost_json = optional_json_string(&input, "language_boost_json");
        let voice_modify_json = optional_json_string(&input, "voice_modify_json");

        let client = create_minimax_client()?;
        let options = audio::T2aAsyncCreateOptions {
            model,
            text,
            text_file_id,
            voice_id,
            voice_setting_json,
            audio_setting_json,
            pronunciation_dict_json,
            language_boost_json,
            voice_modify_json,
        };

        match audio::t2a_async_create(&client, options).await {
            Ok(response) => Ok(ToolResult::success(pretty_json(&response))),
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to create async TTS task: {e}"
            ))),
        }
    }
}

/// Tool for querying async TTS tasks using `MiniMax`.
pub struct TtsAsyncQueryTool;

#[async_trait]
impl ToolSpec for TtsAsyncQueryTool {
    fn name(&self) -> &'static str {
        "tts_async_query"
    }

    fn description(&self) -> &'static str {
        "Query an async text-to-speech task by task ID."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Task ID returned from async TTS creation"
                }
            },
            "required": ["task_id"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network, ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let task_id = required_str(&input, "task_id")?;
        let client = create_minimax_client()?;
        let options = audio::T2aAsyncQueryOptions {
            task_id: task_id.to_string(),
        };

        match audio::t2a_async_query(&client, options).await {
            Ok(response) => Ok(ToolResult::success(pretty_json(&response))),
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to query async TTS task: {e}"
            ))),
        }
    }
}

// === Image Understanding Tool ===

/// Tool for understanding images using `MiniMax` chat completions.
pub struct AnalyzeImageTool;

#[async_trait]
impl ToolSpec for AnalyzeImageTool {
    fn name(&self) -> &'static str {
        "analyze_image"
    }

    fn description(&self) -> &'static str {
        "Analyze an image and answer a prompt using MiniMax image understanding."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the image file (relative to workspace)"
                },
                "prompt": {
                    "type": "string",
                    "description": "Question or instruction for the image (default: Describe this image)"
                },
                "model": {
                    "type": "string",
                    "description": "Model name for image understanding (default: MiniMax-Text-01)",
                    "default": "MiniMax-Text-01"
                },
                "max_tokens": {
                    "type": "integer",
                    "description": "Maximum tokens in the response",
                    "default": 512
                }
            },
            "required": ["path"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network, ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let path_str = required_str(&input, "path")?;
        let prompt = optional_str(&input, "prompt")
            .unwrap_or("Describe this image.")
            .to_string();
        let model = optional_str(&input, "model")
            .unwrap_or("MiniMax-Text-01")
            .to_string();
        let max_tokens = input
            .get("max_tokens")
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(512);

        let path = context.resolve_path(path_str)?;
        let bytes = std::fs::read(&path).map_err(|e| {
            ToolError::execution_failed(format!("Failed to read image {}: {}", path.display(), e))
        })?;
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);

        let request_content = format!("{prompt}\n\n[Image base64:{encoded}]");
        let request = json!({
            "model": model,
            "messages": [{ "role": "user", "content": request_content }],
            "stream": false,
            "max_tokens": max_tokens,
        });

        let client = create_minimax_client()?;
        match client.post_json("/v1/chat/completions", &request).await {
            Ok(response) => {
                if let Some(text) = extract_chat_text(&response) {
                    Ok(ToolResult::success(text))
                } else {
                    Ok(ToolResult::success(pretty_json(&response)))
                }
            }
            Err(e) => Ok(ToolResult::error(format!("Failed to analyze image: {e}"))),
        }
    }
}

// === Image Generation Tool ===

/// Tool for generating images using `MiniMax`.
pub struct GenerateImageTool;

fn normalize_image_model(value: Option<&str>) -> String {
    let raw = value.unwrap_or("image-01").trim();
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    if cleaned.is_empty() {
        "image-01".to_string()
    } else {
        cleaned
    }
}

fn normalize_aspect_ratio(value: Option<&str>) -> Option<String> {
    let raw = value?.trim();
    if raw.is_empty() {
        return None;
    }
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == ':')
        .collect();
    if cleaned.contains(':') {
        Some(cleaned)
    } else {
        None
    }
}

fn normalize_response_format(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "b64_json" | "b64" | "base64" => "b64_json".to_string(),
        _ => "url".to_string(),
    }
}

fn normalize_video_resolution(value: Option<&str>) -> Result<Option<String>, ToolError> {
    let Some(raw) = value else {
        return Ok(Some("768P".to_string()));
    };

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Some("768P".to_string()));
    }

    let upper = trimmed.to_ascii_uppercase();
    let normalized = match upper.as_str() {
        "512P" | "512" => "512P",
        "768P" | "768" => "768P",
        "1080P" | "1080" => "1080P",
        "720P" | "720" => "768P", // common expectation; MiniMax video uses 768P instead
        _ => {
            return Err(ToolError::invalid_input(
                "Invalid resolution. Supported: 512P, 768P, 1080P (720p maps to 768P).",
            ));
        }
    };

    Ok(Some(normalized.to_string()))
}

fn normalize_video_duration(value: Option<u32>) -> Result<Option<u32>, ToolError> {
    let Some(value) = value else {
        return Ok(Some(6));
    };

    match value {
        6 | 10 => Ok(Some(value)),
        5 => Ok(Some(6)), // legacy default -> supported default
        _ => Err(ToolError::invalid_input(
            "Invalid duration. Supported: 6 or 10 seconds.",
        )),
    }
}

#[async_trait]
impl ToolSpec for GenerateImageTool {
    fn name(&self) -> &'static str {
        "generate_image"
    }

    fn description(&self) -> &'static str {
        "Generate images from text prompts using MiniMax. Returns the path(s) to saved images."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Text prompt describing the image to generate"
                },
                "model": {
                    "type": "string",
                    "description": "Image model name (default: image-01)",
                    "default": "image-01"
                },
                "aspect_ratio": {
                    "type": "string",
                    "description": "Aspect ratio (e.g., 1:1, 16:9, 9:16)"
                },
                "response_format": {
                    "type": "string",
                    "description": "Response format (url or b64_json)"
                },
                "style": {
                    "type": "string",
                    "description": "Style preset (e.g., realistic, anime, 3d-render)"
                },
                "negative_prompt": {
                    "type": "string",
                    "description": "Optional negative prompt to steer away from"
                },
                "width": {
                    "type": "integer",
                    "description": "Custom width in pixels (may override aspect ratio)"
                },
                "height": {
                    "type": "integer",
                    "description": "Custom height in pixels (may override aspect ratio)"
                },
                "seed": {
                    "type": "integer",
                    "description": "Seed for deterministic generation"
                },
                "n": {
                    "type": "integer",
                    "description": "Number of images to generate"
                },
                "prompt_optimizer": {
                    "type": "boolean",
                    "description": "Let MiniMax optimize the prompt"
                },
                "subject_reference": {
                    "type": "array",
                    "description": "Subject reference URLs or IDs",
                    "items": { "type": "string" }
                }
            },
            "required": ["prompt"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network, ToolCapability::WritesFiles]
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let prompt = required_str(&input, "prompt")?;
        let model = normalize_image_model(optional_str(&input, "model"));
        let aspect_ratio = normalize_aspect_ratio(optional_str(&input, "aspect_ratio"));
        let response_format =
            optional_str(&input, "response_format").map(normalize_response_format);
        let style = optional_str(&input, "style").map(std::string::ToString::to_string);
        let negative_prompt =
            optional_str(&input, "negative_prompt").map(std::string::ToString::to_string);
        let width = input
            .get("width")
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok());
        let height = input
            .get("height")
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok());
        let seed = input
            .get("seed")
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok());
        let n = input
            .get("n")
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok());
        let prompt_optimizer = input
            .get("prompt_optimizer")
            .and_then(serde_json::Value::as_bool);
        let subject_reference = optional_string_vec(&input, "subject_reference");

        let client = create_minimax_client()?;
        let output_dir = context.workspace.clone();

        let options = image::ImageGenerateOptions {
            model,
            prompt: prompt.to_string(),
            negative_prompt,
            aspect_ratio,
            width,
            height,
            style,
            response_format,
            seed,
            n,
            prompt_optimizer,
            subject_reference,
            output_dir,
        };

        match image::generate(&client, options).await {
            Ok(paths) => {
                let message = if paths.len() == 1 {
                    format!("Image saved to {}", paths[0].display())
                } else {
                    let joined = paths
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("Images saved to {joined}")
                };
                Ok(ToolResult::success(message))
            }
            Err(e) => Ok(ToolResult::error(format!("Failed to generate image: {e}"))),
        }
    }
}

// === Video Generation Tool ===

/// Tool for generating videos using `MiniMax`.
pub struct GenerateVideoTool;

#[async_trait]
impl ToolSpec for GenerateVideoTool {
    fn name(&self) -> &'static str {
        "generate_video"
    }

    fn description(&self) -> &'static str {
        "Generate a video from a text prompt using MiniMax. Saves the downloaded video file to the workspace (set wait=false for async)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Text prompt describing the video to generate"
                },
                "model": {
                    "type": "string",
                    "description": "Video model name (default: MiniMax-Hailuo-02). Options: MiniMax-Hailuo-02, MiniMax-Hailuo-2.3, MiniMax-Hailuo-2.3-Fast, video-01, video-01-live",
                    "default": "MiniMax-Hailuo-02"
                },
                "duration": {
                    "type": "integer",
                    "description": "Video duration in seconds (supported: 6 or 10)",
                    "default": 6
                },
                "resolution": {
                    "type": "string",
                    "description": "Video resolution (supported: 512P, 768P, 1080P; 720p maps to 768P)",
                    "default": "768P"
                },
                "first_frame": {
                    "type": "string",
                    "description": "Optional first frame image path or URL"
                },
                "last_frame": {
                    "type": "string",
                    "description": "Optional last frame image path or URL"
                },
                "subject_reference": {
                    "type": "array",
                    "description": "Subject reference images (paths or URLs)",
                    "items": { "type": "string" }
                },
                "subject_reference_json": {
                    "type": "string",
                    "description": "Raw JSON string for subject reference (advanced)"
                },
                "callback_url": {
                    "type": "string",
                    "description": "Webhook callback URL for completion"
                },
                "prompt_optimizer": {
                    "type": "boolean",
                    "description": "Let MiniMax optimize the prompt"
                },
                "fast_pretreatment": {
                    "type": "boolean",
                    "description": "Enable fast pretreatment"
                },
                "wait": {
                    "type": "boolean",
                    "description": "Wait for the video and download when ready",
                    "default": true
                }
            },
            "required": ["prompt"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network, ToolCapability::WritesFiles]
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let prompt = required_str(&input, "prompt")?;
        let model = optional_str(&input, "model")
            .unwrap_or("MiniMax-Hailuo-02")
            .to_string();
        let duration = input
            .get("duration")
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok());
        let duration = normalize_video_duration(duration)?;
        let resolution = normalize_video_resolution(optional_str(&input, "resolution"))?;
        let first_frame = optional_str(&input, "first_frame").map(std::string::ToString::to_string);
        let last_frame = optional_str(&input, "last_frame").map(std::string::ToString::to_string);
        let subject_reference = optional_string_vec(&input, "subject_reference");
        let subject_reference_json = input.get("subject_reference_json").and_then(|value| {
            if let Some(raw) = value.as_str() {
                Some(raw.to_string())
            } else {
                serde_json::to_string(value).ok()
            }
        });
        let callback_url =
            optional_str(&input, "callback_url").map(std::string::ToString::to_string);
        let prompt_optimizer = input
            .get("prompt_optimizer")
            .and_then(serde_json::Value::as_bool);
        let fast_pretreatment = input
            .get("fast_pretreatment")
            .and_then(serde_json::Value::as_bool);
        let wait = input
            .get("wait")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);

        let client = create_minimax_client()?;
        let output_dir = context.workspace.clone();

        let options = video::VideoGenerateOptions {
            model,
            prompt: prompt.to_string(),
            first_frame,
            last_frame,
            subject_reference,
            subject_reference_json,
            duration,
            resolution,
            callback_url,
            prompt_optimizer,
            fast_pretreatment,
            wait,
            output_dir,
        };

        match video::generate(&client, options).await {
            Ok(result) => {
                if let Some(path) = result.video_path {
                    Ok(ToolResult::success(format!(
                        "Video saved to {}",
                        path.display()
                    )))
                } else if let Some(task_id) = result.task_id {
                    if let Some(path) = result.response_path {
                        Ok(ToolResult::success(format!(
                            "Video task submitted: {task_id}. Response saved to {}",
                            path.display()
                        )))
                    } else {
                        Ok(ToolResult::success(format!(
                            "Video task submitted: {task_id}"
                        )))
                    }
                } else if let Some(path) = result.response_path {
                    Ok(ToolResult::success(format!(
                        "Video request submitted. Response saved to {}",
                        path.display()
                    )))
                } else {
                    Ok(ToolResult::success("Video request submitted"))
                }
            }
            Err(e) => Ok(ToolResult::error(format!("Failed to generate video: {e}"))),
        }
    }
}

// === Video Template Tools ===

/// Tool for starting a video template generation task.
pub struct VideoTemplateCreateTool;

#[async_trait]
impl ToolSpec for VideoTemplateCreateTool {
    fn name(&self) -> &'static str {
        "generate_video_template"
    }

    fn description(&self) -> &'static str {
        "Generate a video from a template using MiniMax. Returns the task response."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "template_id": {
                    "type": "string",
                    "description": "Template ID for the video generation"
                },
                "text_inputs": {
                    "description": "Text inputs for template slots",
                    "type": ["object", "array"]
                },
                "media_inputs": {
                    "description": "Media inputs for template slots",
                    "type": ["object", "array"]
                },
                "callback_url": {
                    "type": "string",
                    "description": "Webhook callback URL for completion"
                }
            },
            "required": ["template_id"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network]
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let template_id = required_str(&input, "template_id")?;
        let text_inputs = input.get("text_inputs").cloned();
        let media_inputs = input.get("media_inputs").cloned();
        let callback_url =
            optional_str(&input, "callback_url").map(std::string::ToString::to_string);

        let client = create_minimax_client()?;
        let options = video::VideoAgentCreateOptions {
            template_id: template_id.to_string(),
            text_inputs,
            media_inputs,
            callback_url,
        };

        match video::agent_create(&client, options).await {
            Ok(response) => Ok(ToolResult::success(pretty_json(&response))),
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to create video template: {e}"
            ))),
        }
    }
}

/// Tool for querying a video template generation task.
pub struct VideoTemplateQueryTool;

#[async_trait]
impl ToolSpec for VideoTemplateQueryTool {
    fn name(&self) -> &'static str {
        "query_video_template"
    }

    fn description(&self) -> &'static str {
        "Query the status of a video template generation task by task ID."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Task ID from template generation"
                }
            },
            "required": ["task_id"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network, ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let task_id = required_str(&input, "task_id")?;
        let client = create_minimax_client()?;

        match video::agent_query(&client, task_id).await {
            Ok(response) => Ok(ToolResult::success(pretty_json(&response))),
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to query video template: {e}"
            ))),
        }
    }
}

// === Video Query Tool ===

/// Tool for querying video generation status.
pub struct QueryVideoTool;

#[async_trait]
impl ToolSpec for QueryVideoTool {
    fn name(&self) -> &'static str {
        "query_video"
    }

    fn description(&self) -> &'static str {
        "Query the status of a video generation task by task ID."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The task ID from a previous video generation"
                }
            },
            "required": ["task_id"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network, ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let task_id = required_str(&input, "task_id")?;

        let client = create_minimax_client()?;
        let options = video::VideoQueryOptions {
            task_id: task_id.to_string(),
        };

        match video::query(&client, options).await {
            Ok(()) => Ok(ToolResult::success(format!(
                "Video task {task_id} queried successfully"
            ))),
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to query video task: {e}"
            ))),
        }
    }
}

// === Music Generation Tool ===

/// Tool for generating music using `MiniMax`.
pub struct GenerateMusicTool;

#[async_trait]
impl ToolSpec for GenerateMusicTool {
    fn name(&self) -> &'static str {
        "generate_music"
    }

    fn description(&self) -> &'static str {
        "Generate music from text prompts using MiniMax. Returns the path to the saved audio file."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Text prompt describing the music to generate"
                },
                "model": {
                    "type": "string",
                    "description": "Music model name (default: music-1.5). Options: music-2.5, music-2.0, music-1.5",
                    "default": "music-1.5"
                },
                "lyrics": {
                    "type": "string",
                    "description": "Optional lyrics to include in the music"
                },
                "output_format": {
                    "type": "string",
                    "description": "Output format value to request from MiniMax (see MiniMax docs)"
                },
                "stream": {
                    "type": "boolean",
                    "description": "Stream music response if true",
                    "default": false
                },
                "audio_setting_json": {
                    "type": ["string", "object"],
                    "description": "JSON for audio_setting overrides"
                }
            },
            "required": ["prompt"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network, ToolCapability::WritesFiles]
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let prompt = required_str(&input, "prompt")?;
        let model = optional_str(&input, "model")
            .unwrap_or("music-1.5")
            .to_string();
        let lyrics = optional_str(&input, "lyrics").map(std::string::ToString::to_string);
        let output_format = optional_str(&input, "output_format")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let stream = input
            .get("stream")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let audio_setting_json = optional_json_string(&input, "audio_setting_json");

        let client = create_minimax_client()?;
        let output_dir = context.workspace.clone();

        let options = music::MusicGenerateOptions {
            model,
            prompt: prompt.to_string(),
            lyrics,
            stream,
            output_format,
            audio_setting_json,
            output_dir,
        };

        match music::generate(&client, options).await {
            Ok(path) => Ok(ToolResult::success(format!(
                "Music saved to {}",
                path.display()
            ))),
            Err(e) => Ok(ToolResult::error(format!("Failed to generate music: {e}"))),
        }
    }
}

// === File Upload Tool ===

/// Tool for uploading files to `MiniMax`.
pub struct UploadFileTool;

#[async_trait]
impl ToolSpec for UploadFileTool {
    fn name(&self) -> &'static str {
        "upload_file"
    }

    fn description(&self) -> &'static str {
        "Upload a file to MiniMax for use in other operations (e.g., voice clone). Returns the file ID."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to upload (relative to workspace)"
                },
                "purpose": {
                    "type": "string",
                    "description": "Purpose of the file upload (e.g., voice_clone, audio)",
                    "default": "audio"
                }
            },
            "required": ["path"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network, ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let path_str = required_str(&input, "path")?;
        let purpose = optional_str(&input, "purpose")
            .unwrap_or("audio")
            .to_string();

        let path = context.resolve_path(path_str)?;

        let client = create_minimax_client()?;
        let options = files::FileUploadOptions { path, purpose };

        match files::upload(&client, options).await {
            Ok(response) => {
                let response_json = serde_json::to_string_pretty(&response)
                    .unwrap_or_else(|_| "Upload successful".to_string());
                Ok(ToolResult::success(response_json))
            }
            Err(e) => Ok(ToolResult::error(format!("Failed to upload file: {e}"))),
        }
    }
}

// === File List Tool ===

/// Tool for listing uploaded files on `MiniMax`.
pub struct ListFilesTool;

#[async_trait]
impl ToolSpec for ListFilesTool {
    fn name(&self) -> &'static str {
        "list_files"
    }

    fn description(&self) -> &'static str {
        "List files uploaded to MiniMax by purpose (e.g., audio, voice_clone)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "purpose": {
                    "type": "string",
                    "description": "Purpose filter (e.g., audio, voice_clone)",
                    "default": "audio"
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network, ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let purpose = optional_str(&input, "purpose")
            .unwrap_or("audio")
            .to_string();

        let client = create_minimax_client()?;
        let options = files::FileListOptions { purpose };

        match files::list(&client, options).await {
            Ok(response) => Ok(ToolResult::success(pretty_json(&response))),
            Err(e) => Ok(ToolResult::error(format!("Failed to list files: {e}"))),
        }
    }
}

// === File Retrieve/Download/Delete Tools ===

/// Tool for retrieving a file URL from `MiniMax`.
pub struct RetrieveFileTool;

#[async_trait]
impl ToolSpec for RetrieveFileTool {
    fn name(&self) -> &'static str {
        "retrieve_file"
    }

    fn description(&self) -> &'static str {
        "Retrieve a file URL from MiniMax by file ID."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_id": {
                    "type": "string",
                    "description": "File ID to retrieve"
                },
                "purpose": {
                    "type": "string",
                    "description": "Optional purpose (e.g., audio, voice_clone)"
                }
            },
            "required": ["file_id"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network, ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let file_id = required_str(&input, "file_id")?;
        let purpose = optional_str(&input, "purpose").map(std::string::ToString::to_string);

        let client = create_minimax_client()?;
        let options = files::FileRetrieveOptions {
            file_id: file_id.to_string(),
            purpose,
        };

        match files::retrieve(&client, options).await {
            Ok(Some(url)) => Ok(ToolResult::success(url)),
            Ok(None) => Ok(ToolResult::error(
                "Failed to retrieve file: no file URL returned.",
            )),
            Err(e) => Ok(ToolResult::error(format!("Failed to retrieve file: {e}"))),
        }
    }
}

/// Tool for downloading a file from `MiniMax`.
pub struct DownloadFileTool;

#[async_trait]
impl ToolSpec for DownloadFileTool {
    fn name(&self) -> &'static str {
        "download_file"
    }

    fn description(&self) -> &'static str {
        "Download a file by file ID from MiniMax to the workspace."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_id": {
                    "type": "string",
                    "description": "File ID to download"
                },
                "output": {
                    "type": "string",
                    "description": "Optional output path (relative to workspace)"
                }
            },
            "required": ["file_id"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network, ToolCapability::WritesFiles]
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let file_id = required_str(&input, "file_id")?;
        let output = match optional_str(&input, "output") {
            Some(path) => Some(context.resolve_path(path)?),
            None => None,
        };

        let client = create_minimax_client()?;
        let options = files::FileRetrieveContentOptions {
            file_id: file_id.to_string(),
            output,
            output_dir: context.workspace.clone(),
        };

        match files::retrieve_content(&client, options).await {
            Ok(path) => Ok(ToolResult::success(format!(
                "File downloaded to {}",
                path.display()
            ))),
            Err(e) => Ok(ToolResult::error(format!("Failed to download file: {e}"))),
        }
    }
}

/// Tool for deleting a file from `MiniMax`.
pub struct DeleteFileTool;

#[async_trait]
impl ToolSpec for DeleteFileTool {
    fn name(&self) -> &'static str {
        "delete_file"
    }

    fn description(&self) -> &'static str {
        "Delete a file from MiniMax by file ID."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_id": {
                    "type": "string",
                    "description": "File ID to delete"
                },
                "purpose": {
                    "type": "string",
                    "description": "Optional purpose (e.g., audio, voice_clone)"
                }
            },
            "required": ["file_id"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network]
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let file_id = required_str(&input, "file_id")?;
        let purpose = optional_str(&input, "purpose").map(std::string::ToString::to_string);

        let client = create_minimax_client()?;
        let options = files::FileDeleteOptions {
            file_id: file_id.to_string(),
            purpose,
        };

        match files::delete(&client, options).await {
            Ok(response) => Ok(ToolResult::success(pretty_json(&response))),
            Err(e) => Ok(ToolResult::error(format!("Failed to delete file: {e}"))),
        }
    }
}

// === Voice Clone Tool ===

/// Tool for cloning voices using `MiniMax`.
pub struct VoiceCloneTool;

#[async_trait]
impl ToolSpec for VoiceCloneTool {
    fn name(&self) -> &'static str {
        "voice_clone"
    }

    fn description(&self) -> &'static str {
        "Clone a voice from an audio file. Uploads the audio and creates a voice ID."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "clone_audio": {
                    "type": "string",
                    "description": "Path to the audio file for cloning (relative to workspace)"
                },
                "prompt_audio": {
                    "type": "string",
                    "description": "Optional prompt audio file for cloning (relative to workspace)"
                },
                "clone_prompt_text": {
                    "type": "string",
                    "description": "Optional prompt text paired with prompt audio"
                },
                "voice_id": {
                    "type": "string",
                    "description": "Optional voice ID to update"
                },
                "text": {
                    "type": "string",
                    "description": "Optional test text to verify the cloned voice"
                },
                "model": {
                    "type": "string",
                    "description": "TTS model to use with the cloned voice (default: speech-2.6). Options: speech-2.6, speech-02-hd, speech-02-turbo, speech-01-hd, speech-01-turbo",
                    "default": "speech-2.6"
                },
                "language_boost_json": {
                    "type": ["string", "object"],
                    "description": "JSON language boost settings"
                },
                "need_noise_reduction": {
                    "type": "boolean",
                    "description": "Enable noise reduction"
                },
                "need_volume_normalization": {
                    "type": "boolean",
                    "description": "Enable volume normalization"
                }
            },
            "required": ["clone_audio"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network, ToolCapability::WritesFiles]
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let clone_audio_str = required_str(&input, "clone_audio")?;
        let prompt_audio = match optional_str(&input, "prompt_audio") {
            Some(path) => Some(context.resolve_path(path)?),
            None => None,
        };
        let clone_prompt_text =
            optional_str(&input, "clone_prompt_text").map(std::string::ToString::to_string);
        let voice_id = optional_str(&input, "voice_id").map(std::string::ToString::to_string);
        let text = optional_str(&input, "text").map(std::string::ToString::to_string);
        let model = optional_str(&input, "model")
            .unwrap_or("speech-2.6")
            .to_string();
        let language_boost_json = optional_json_string(&input, "language_boost_json");
        let need_noise_reduction = input
            .get("need_noise_reduction")
            .and_then(serde_json::Value::as_bool);
        let need_volume_normalization = input
            .get("need_volume_normalization")
            .and_then(serde_json::Value::as_bool);

        let clone_audio = context.resolve_path(clone_audio_str)?;

        let client = create_minimax_client()?;

        let options = audio::VoiceCloneOptions {
            clone_audio,
            prompt_audio,
            voice_id,
            clone_prompt_text,
            text,
            model: Some(model),
            language_boost_json,
            need_noise_reduction,
            need_volume_normalization,
        };

        match audio::voice_clone(&client, options).await {
            Ok(()) => Ok(ToolResult::success("Voice cloned successfully")),
            Err(e) => Ok(ToolResult::error(format!("Failed to clone voice: {e}"))),
        }
    }
}

// === Voice Management Tools ===

/// Tool for listing available voices.
pub struct VoiceListTool;

#[async_trait]
impl ToolSpec for VoiceListTool {
    fn name(&self) -> &'static str {
        "voice_list"
    }

    fn description(&self) -> &'static str {
        "List available voices from MiniMax."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "voice_type": {
                    "type": "string",
                    "description": "Optional voice type filter"
                }
            }
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network, ToolCapability::ReadOnly]
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let voice_type = optional_str(&input, "voice_type").map(std::string::ToString::to_string);
        let client = create_minimax_client()?;
        let options = audio::VoiceListOptions { voice_type };

        match audio::voice_list(&client, options).await {
            Ok(response) => Ok(ToolResult::success(pretty_json(&response))),
            Err(e) => Ok(ToolResult::error(format!("Failed to list voices: {e}"))),
        }
    }
}

/// Tool for deleting a voice.
pub struct VoiceDeleteTool;

#[async_trait]
impl ToolSpec for VoiceDeleteTool {
    fn name(&self) -> &'static str {
        "voice_delete"
    }

    fn description(&self) -> &'static str {
        "Delete a voice by voice_id."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "voice_id": {
                    "type": "string",
                    "description": "Voice ID to delete"
                },
                "voice_type": {
                    "type": "string",
                    "description": "Optional voice type"
                }
            },
            "required": ["voice_id"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network]
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let voice_id = required_str(&input, "voice_id")?;
        let voice_type = optional_str(&input, "voice_type").map(std::string::ToString::to_string);
        let client = create_minimax_client()?;
        let options = audio::VoiceDeleteOptions {
            voice_type,
            voice_id: voice_id.to_string(),
        };

        match audio::voice_delete(&client, options).await {
            Ok(response) => Ok(ToolResult::success(pretty_json(&response))),
            Err(e) => Ok(ToolResult::error(format!("Failed to delete voice: {e}"))),
        }
    }
}

/// Tool for designing a voice from a prompt.
pub struct VoiceDesignTool;

#[async_trait]
impl ToolSpec for VoiceDesignTool {
    fn name(&self) -> &'static str {
        "voice_design"
    }

    fn description(&self) -> &'static str {
        "Create or refine a voice design using a prompt and preview text."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Design prompt for the voice"
                },
                "preview_text": {
                    "type": "string",
                    "description": "Preview text to synthesize"
                },
                "voice_id": {
                    "type": "string",
                    "description": "Optional voice ID to update"
                }
            },
            "required": ["prompt", "preview_text"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::Network]
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let prompt = required_str(&input, "prompt")?;
        let preview_text = required_str(&input, "preview_text")?;
        let voice_id = optional_str(&input, "voice_id").map(std::string::ToString::to_string);
        let client = create_minimax_client()?;
        let options = audio::VoiceDesignOptions {
            prompt: prompt.to_string(),
            preview_text: preview_text.to_string(),
            voice_id,
        };

        match audio::voice_design(&client, options).await {
            Ok(response) => Ok(ToolResult::success(pretty_json(&response))),
            Err(e) => Ok(ToolResult::error(format!("Failed to design voice: {e}"))),
        }
    }
}

// === Helper function to create MiniMaxClient ===

/// Create a `MiniMaxClient` from the default config.
fn create_minimax_client() -> Result<MiniMaxClient, ToolError> {
    Config::load(None, None)
        .map_err(|e| ToolError::execution_failed(format!("Failed to load config: {e}")))
        .and_then(|config| {
            MiniMaxClient::new(&config).map_err(|e| {
                ToolError::execution_failed(format!("Failed to create MiniMax client: {e}"))
            })
        })
}

// === Unit Tests ===

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_tts_tool_properties() {
        let tool = TtsTool;
        assert_eq!(tool.name(), "tts");
        assert!(!tool.is_read_only());
    }

    #[test]
    fn test_tts_async_create_tool_properties() {
        let tool = TtsAsyncCreateTool;
        assert_eq!(tool.name(), "tts_async_create");
        assert!(!tool.is_read_only());
    }

    #[test]
    fn test_tts_async_query_tool_properties() {
        let tool = TtsAsyncQueryTool;
        assert_eq!(tool.name(), "tts_async_query");
        assert!(tool.is_read_only());
    }

    #[test]
    fn test_analyze_image_tool_properties() {
        let tool = AnalyzeImageTool;
        assert_eq!(tool.name(), "analyze_image");
        assert!(tool.is_read_only());
    }

    #[test]
    fn test_generate_image_tool_properties() {
        let tool = GenerateImageTool;
        assert_eq!(tool.name(), "generate_image");
        assert!(!tool.is_read_only());
    }

    #[test]
    fn test_generate_video_tool_properties() {
        let tool = GenerateVideoTool;
        assert_eq!(tool.name(), "generate_video");
        assert!(!tool.is_read_only());
    }

    #[test]
    fn test_query_video_tool_properties() {
        let tool = QueryVideoTool;
        assert_eq!(tool.name(), "query_video");
        assert!(tool.is_read_only());
    }

    #[test]
    fn test_generate_music_tool_properties() {
        let tool = GenerateMusicTool;
        assert_eq!(tool.name(), "generate_music");
        assert!(!tool.is_read_only());
    }

    #[test]
    fn test_upload_file_tool_properties() {
        let tool = UploadFileTool;
        assert_eq!(tool.name(), "upload_file");
        assert!(tool.is_read_only());
    }

    #[test]
    fn test_list_files_tool_properties() {
        let tool = ListFilesTool;
        assert_eq!(tool.name(), "list_files");
        assert!(tool.is_read_only());
    }

    #[test]
    fn test_retrieve_file_tool_properties() {
        let tool = RetrieveFileTool;
        assert_eq!(tool.name(), "retrieve_file");
        assert!(tool.is_read_only());
    }

    #[test]
    fn test_download_file_tool_properties() {
        let tool = DownloadFileTool;
        assert_eq!(tool.name(), "download_file");
        assert!(!tool.is_read_only());
    }

    #[test]
    fn test_delete_file_tool_properties() {
        let tool = DeleteFileTool;
        assert_eq!(tool.name(), "delete_file");
        assert!(!tool.is_read_only());
    }

    #[test]
    fn test_voice_clone_tool_properties() {
        let tool = VoiceCloneTool;
        assert_eq!(tool.name(), "voice_clone");
        assert!(!tool.is_read_only());
    }

    #[test]
    fn test_voice_list_tool_properties() {
        let tool = VoiceListTool;
        assert_eq!(tool.name(), "voice_list");
        assert!(tool.is_read_only());
    }

    #[test]
    fn test_voice_delete_tool_properties() {
        let tool = VoiceDeleteTool;
        assert_eq!(tool.name(), "voice_delete");
        assert!(!tool.is_read_only());
    }

    #[test]
    fn test_voice_design_tool_properties() {
        let tool = VoiceDesignTool;
        assert_eq!(tool.name(), "voice_design");
        assert!(!tool.is_read_only());
    }

    #[test]
    fn test_generate_video_template_tool_properties() {
        let tool = VideoTemplateCreateTool;
        assert_eq!(tool.name(), "generate_video_template");
        assert!(!tool.is_read_only());
    }

    #[test]
    fn test_query_video_template_tool_properties() {
        let tool = VideoTemplateQueryTool;
        assert_eq!(tool.name(), "query_video_template");
        assert!(tool.is_read_only());
    }

    #[test]
    fn test_tts_tool_schema() {
        let tool = TtsTool;
        let schema = tool.input_schema();
        assert!(schema.is_object());
        assert!(schema.get("type").is_some());
        assert!(schema.get("properties").is_some());
    }

    #[test]
    fn test_generate_image_tool_schema() {
        let tool = GenerateImageTool;
        let schema = tool.input_schema();
        assert!(schema.is_object());
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("prompt")));
    }
}
