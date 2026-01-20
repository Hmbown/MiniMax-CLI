//! Audio API wrappers for `MiniMax` (TTS, async TTS, voice operations).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::Engine;
use colored::Colorize;
use serde_json::{Value, json};

use crate::client::MiniMaxClient;
use crate::modules::files::{FileUploadOptions, retrieve_file, upload};
use crate::palette;
use crate::utils::{
    extension_from_url, output_path, pretty_json, timestamped_filename, write_bytes,
};

// === Types ===

const DEFAULT_TTS_VOICE_ID: &str = "English_expressive_narrator";

/// Options for synchronous text-to-audio generation.
pub struct T2aOptions {
    pub model: String,
    pub text: String,
    pub stream: bool,
    pub output_format: Option<String>,
    pub voice_id: Option<String>,
    pub voice_setting_json: Option<String>,
    pub audio_setting_json: Option<String>,
    pub pronunciation_dict_json: Option<String>,
    pub timber_weights_json: Option<String>,
    pub language_boost_json: Option<String>,
    pub voice_modify_json: Option<String>,
    pub subtitle_enable: Option<bool>,
    pub output_dir: PathBuf,
}

/// Options for creating an async TTS job.
pub struct T2aAsyncCreateOptions {
    pub model: String,
    pub text: Option<String>,
    pub text_file_id: Option<String>,
    pub voice_id: Option<String>,
    pub voice_setting_json: Option<String>,
    pub audio_setting_json: Option<String>,
    pub pronunciation_dict_json: Option<String>,
    pub language_boost_json: Option<String>,
    pub voice_modify_json: Option<String>,
}

/// Options for querying an async TTS job.
pub struct T2aAsyncQueryOptions {
    pub task_id: String,
}

/// Options for cloning a voice from audio samples.
pub struct VoiceCloneOptions {
    pub clone_audio: PathBuf,
    pub prompt_audio: Option<PathBuf>,
    pub voice_id: Option<String>,
    pub clone_prompt_text: Option<String>,
    pub text: Option<String>,
    pub model: Option<String>,
    pub language_boost_json: Option<String>,
    pub need_noise_reduction: Option<bool>,
    pub need_volume_normalization: Option<bool>,
}

/// Options for listing voices.
pub struct VoiceListOptions {
    pub voice_type: Option<String>,
}

/// Options for deleting a voice.
pub struct VoiceDeleteOptions {
    pub voice_type: Option<String>,
    pub voice_id: String,
}

/// Options for designing a synthetic voice.
pub struct VoiceDesignOptions {
    pub prompt: String,
    pub preview_text: String,
    pub voice_id: Option<String>,
}

// === API Calls ===

pub async fn t2a(client: &MiniMaxClient, options: T2aOptions) -> Result<PathBuf> {
    let mut body = json!({
        "model": options.model,
        "text": options.text,
        "stream": options.stream,
    });

    if let Some(output_format) = options.output_format.clone() {
        body["output_format"] = json!(output_format);
    }

    if let Some(voice_id) = options.voice_id {
        body["voice_setting"] = json!({ "voice_id": voice_id });
    }

    merge_json_field(&mut body, "voice_setting", options.voice_setting_json)?;
    merge_json_field(&mut body, "audio_setting", options.audio_setting_json)?;
    merge_json_field(
        &mut body,
        "pronunciation_dict",
        options.pronunciation_dict_json,
    )?;
    merge_json_field(&mut body, "timber_weights", options.timber_weights_json)?;
    merge_json_field(&mut body, "language_boost", options.language_boost_json)?;
    merge_json_field(&mut body, "voice_modify", options.voice_modify_json)?;

    if body.get("voice_setting").is_none() {
        body["voice_setting"] = json!({ "voice_id": DEFAULT_TTS_VOICE_ID });
    }

    if let Some(subtitle_enable) = options.subtitle_enable {
        body["subtitle_enable"] = json!(subtitle_enable);
    }

    if options.stream {
        let response = client.post_json_raw("/v1/t2a_v2", &body).await?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();

        let bytes = response.bytes().await?;

        if let Some(value) = parse_json_bytes(&content_type, &bytes) {
            return handle_audio_response(client, &value, &options.output_dir).await;
        }

        let extension = options
            .output_format
            .or_else(|| {
                extension_from_content_type(&content_type).map(std::string::ToString::to_string)
            })
            .or_else(|| infer_audio_extension(&bytes).map(std::string::ToString::to_string))
            .unwrap_or_else(|| "bin".to_string());
        let filename = timestamped_filename("speech", &extension);
        let path = output_path(&options.output_dir, &filename);
        write_bytes(&path, &bytes)?;
        let (r, g, b) = palette::MINIMAX_GREEN_RGB;
        println!("{} {}", "Saved".truecolor(r, g, b).bold(), path.display());
        Ok(path)
    } else {
        let response: Value = client.post_json("/v1/t2a_v2", &body).await?;
        handle_audio_response(client, &response, &options.output_dir).await
    }
}

pub async fn t2a_async_create(
    client: &MiniMaxClient,
    options: T2aAsyncCreateOptions,
) -> Result<Value> {
    let mut body = json!({ "model": options.model });
    if let Some(text) = options.text {
        body["text"] = json!(text);
    }
    if let Some(text_file_id) = options.text_file_id {
        body["text_file_id"] = json!(text_file_id);
    }
    if let Some(voice_id) = options.voice_id {
        body["voice_setting"] = json!({ "voice_id": voice_id });
    }

    merge_json_field(&mut body, "voice_setting", options.voice_setting_json)?;
    merge_json_field(&mut body, "audio_setting", options.audio_setting_json)?;
    merge_json_field(
        &mut body,
        "pronunciation_dict",
        options.pronunciation_dict_json,
    )?;
    merge_json_field(&mut body, "language_boost", options.language_boost_json)?;
    merge_json_field(&mut body, "voice_modify", options.voice_modify_json)?;

    let response: Value = client.post_json("/v1/t2a_async_v2", &body).await?;
    Ok(response)
}

pub async fn t2a_async_query(
    client: &MiniMaxClient,
    options: T2aAsyncQueryOptions,
) -> Result<Value> {
    let response: Value = client
        .get_json(
            "/v1/query/t2a_async_query_v2",
            Some(&[("task_id", options.task_id.as_str())]),
        )
        .await?;
    Ok(response)
}

pub async fn voice_clone(client: &MiniMaxClient, options: VoiceCloneOptions) -> Result<()> {
    let upload_response = upload(
        client,
        FileUploadOptions {
            path: options.clone_audio,
            purpose: "voice_clone".to_string(),
        },
    )
    .await?;

    let clone_file_id = extract_file_id(&upload_response)
        .context("Could not find file_id in voice clone upload response.")?;

    let prompt_audio_id = if let Some(prompt_audio) = options.prompt_audio {
        let prompt_response = upload(
            client,
            FileUploadOptions {
                path: prompt_audio,
                purpose: "prompt_audio".to_string(),
            },
        )
        .await?;
        Some(
            extract_file_id(&prompt_response)
                .context("Could not find file_id in prompt audio upload response.")?,
        )
    } else {
        None
    };

    let mut clone_prompt = serde_json::Map::new();
    if let Some(prompt_audio_id) = prompt_audio_id {
        clone_prompt.insert("prompt_audio".to_string(), json!(prompt_audio_id));
    }
    if let Some(text) = options.clone_prompt_text {
        clone_prompt.insert("text".to_string(), json!(text));
    }

    let mut body = json!({
        "file_id": clone_file_id,
    });
    body["clone_prompt"] = Value::Object(clone_prompt);

    if let Some(voice_id) = options.voice_id {
        body["voice_id"] = json!(voice_id);
    }
    if let Some(text) = options.text {
        body["text"] = json!(text);
    }
    if let Some(model) = options.model {
        body["model"] = json!(model);
    }
    if let Some(language_boost) = options.language_boost_json {
        let parsed: Value = serde_json::from_str(&language_boost)
            .context("Failed to parse language_boost_json: expected JSON.")?;
        body["language_boost"] = parsed;
    }
    if let Some(need_noise_reduction) = options.need_noise_reduction {
        body["need_noise_reduction"] = json!(need_noise_reduction);
    }
    if let Some(need_volume_normalization) = options.need_volume_normalization {
        body["need_volume_normalization"] = json!(need_volume_normalization);
    }

    let response: Value = client.post_json("/v1/voice_clone", &body).await?;
    println!("{}", pretty_json(&response));
    Ok(())
}

pub async fn voice_list(client: &MiniMaxClient, options: VoiceListOptions) -> Result<Value> {
    let voice_type = options.voice_type.unwrap_or_else(|| "all".to_string());
    let body = json!({ "voice_type": voice_type });
    let response: Value = client.post_json("/v1/get_voice", &body).await?;
    Ok(response)
}

pub async fn voice_delete(client: &MiniMaxClient, options: VoiceDeleteOptions) -> Result<Value> {
    let mut body = json!({ "voice_id": options.voice_id });
    if let Some(voice_type) = options.voice_type {
        body["voice_type"] = json!(voice_type);
    }
    let response: Value = client.post_json("/v1/delete_voice", &body).await?;
    Ok(response)
}

pub async fn voice_design(client: &MiniMaxClient, options: VoiceDesignOptions) -> Result<Value> {
    let mut body = json!({
        "prompt": options.prompt,
        "preview_text": options.preview_text,
    });
    if let Some(voice_id) = options.voice_id {
        body["voice_id"] = json!(voice_id);
    }
    let response: Value = client.post_json("/v1/voice_design", &body).await?;
    Ok(response)
}

async fn handle_audio_response(
    client: &MiniMaxClient,
    response: &Value,
    output_dir: &Path,
) -> Result<PathBuf> {
    ensure_success(response)?;

    if let Some(url) = extract_audio_url(response) {
        let bytes = client.get_bytes(&url).await?;
        let extension = extension_from_url(&url)
            .or_else(|| infer_audio_extension(&bytes).map(std::string::ToString::to_string))
            .unwrap_or_else(|| "bin".to_string());
        let filename = timestamped_filename("speech", &extension);
        let path = output_path(output_dir, &filename);
        write_bytes(&path, &bytes)?;
        let (r, g, b) = palette::MINIMAX_GREEN_RGB;
        println!("{} {}", "Saved".truecolor(r, g, b).bold(), path.display());
        return Ok(path);
    }

    if let Some(file_id) = extract_file_id(response) {
        let url_opt = retrieve_file(client, &file_id, Some("audio")).await?;
        if let Some(url) = url_opt {
            let bytes = client.get_bytes(&url).await?;
            let extension = extension_from_url(&url)
                .or_else(|| infer_audio_extension(&bytes).map(std::string::ToString::to_string))
                .unwrap_or_else(|| "bin".to_string());
            let filename = timestamped_filename("speech", &extension);
            let path = output_path(output_dir, &filename);
            write_bytes(&path, &bytes)?;
            let (r, g, b) = palette::MINIMAX_GREEN_RGB;
            println!("{} {}", "Saved".truecolor(r, g, b).bold(), path.display());
            return Ok(path);
        }
    }

    if let Some(b64) = extract_audio_base64(response) {
        let bytes = decode_hex_or_base64(b64.trim())
            .context("Failed to decode audio payload: expected hex or base64 data.")?;
        let extension = infer_audio_extension(&bytes)
            .map(std::string::ToString::to_string)
            .unwrap_or_else(|| "bin".to_string());
        let filename = timestamped_filename("speech", &extension);
        let path = output_path(output_dir, &filename);
        write_bytes(&path, &bytes)?;
        let (r, g, b) = palette::MINIMAX_GREEN_RGB;
        println!("{} {}", "Saved".truecolor(r, g, b).bold(), path.display());
        return Ok(path);
    }

    anyhow::bail!(
        "Failed to generate audio: no audio payload found in response. Response: {}",
        pretty_json(response)
    )
}

fn extract_audio_url(response: &Value) -> Option<String> {
    if let Some(url) = response
        .get("audio_url")
        .or_else(|| response.get("url"))
        .and_then(|value| value.as_str())
    {
        return Some(url.to_string());
    }
    if let Some(data) = response.get("data")
        && let Some(url) = data
            .get("audio_url")
            .or_else(|| data.get("url"))
            .and_then(|value| value.as_str())
    {
        return Some(url.to_string());
    }
    None
}

fn extract_file_id(response: &Value) -> Option<String> {
    response
        .get("file_id")
        .and_then(|value| value.as_str())
        .map(std::string::ToString::to_string)
        .or_else(|| {
            response
                .get("data")
                .and_then(|value| value.get("file_id"))
                .and_then(|value| value.as_str())
                .map(std::string::ToString::to_string)
        })
}

fn extract_audio_base64(response: &Value) -> Option<String> {
    if let Some(b64) = response
        .get("audio")
        .or_else(|| response.get("audio_base64"))
        .and_then(|value| value.as_str())
    {
        return Some(b64.to_string());
    }

    if let Some(data) = response.get("data") {
        if let Some(b64) = data
            .get("audio")
            .or_else(|| data.get("audio_base64"))
            .and_then(|value| value.as_str())
        {
            return Some(b64.to_string());
        }
        if let Some(items) = data.as_array() {
            for item in items {
                if let Some(b64) = item
                    .get("audio")
                    .or_else(|| item.get("audio_base64"))
                    .and_then(|value| value.as_str())
                {
                    return Some(b64.to_string());
                }
            }
        }
    }

    None
}

fn ensure_success(response: &Value) -> Result<()> {
    let status_code = response
        .get("base_resp")
        .and_then(|base| base.get("status_code"))
        .and_then(serde_json::Value::as_i64);
    let status_msg = response
        .get("base_resp")
        .and_then(|base| base.get("status_msg"))
        .and_then(|value| value.as_str())
        .unwrap_or("unknown");

    if let Some(status_code) = status_code
        && status_code != 0
    {
        anyhow::bail!(
            "MiniMax t2a_v2 error: {status_code} {status_msg}. Response: {}",
            pretty_json(response)
        );
    }

    Ok(())
}

fn parse_json_bytes(content_type: &str, bytes: &[u8]) -> Option<Value> {
    let looks_json = content_type
        .to_ascii_lowercase()
        .contains("application/json")
        || bytes
            .iter()
            .copied()
            .find(|b| !b.is_ascii_whitespace())
            .is_some_and(|b| b == b'{' || b == b'[');
    if !looks_json {
        return None;
    }
    serde_json::from_slice(bytes).ok()
}

fn decode_hex_or_base64(payload: &str) -> Result<Vec<u8>> {
    if looks_like_hex(payload) {
        return decode_hex(payload).context("Invalid hex payload.");
    }

    base64::engine::general_purpose::STANDARD
        .decode(payload)
        .context("Invalid base64 payload.")
}

fn looks_like_hex(payload: &str) -> bool {
    !payload.is_empty()
        && payload.len().is_multiple_of(2)
        && payload.bytes().all(|b| b.is_ascii_hexdigit())
}

fn decode_hex(payload: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(payload.len() / 2);
    let mut iter = payload.bytes();
    while let (Some(high), Some(low)) = (iter.next(), iter.next()) {
        let high = hex_value(high).context("Invalid hex digit.")?;
        let low = hex_value(low).context("Invalid hex digit.")?;
        out.push((high << 4) | low);
    }
    Ok(out)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn infer_audio_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return Some("wav");
    }
    if bytes.starts_with(b"ID3")
        || bytes.starts_with(&[0xFF, 0xFB])
        || bytes.starts_with(&[0xFF, 0xF3])
        || bytes.starts_with(&[0xFF, 0xF2])
    {
        return Some("mp3");
    }
    if bytes.starts_with(b"fLaC") {
        return Some("flac");
    }
    if bytes.starts_with(b"OggS") {
        return Some("ogg");
    }
    if bytes.starts_with(b"MThd") {
        return Some("mid");
    }
    None
}

fn extension_from_content_type(content_type: &str) -> Option<&'static str> {
    let normalized = content_type.to_ascii_lowercase();
    if normalized.contains("audio/mpeg") {
        return Some("mp3");
    }
    if normalized.contains("audio/wav") || normalized.contains("audio/wave") {
        return Some("wav");
    }
    if normalized.contains("audio/mp4") || normalized.contains("audio/x-m4a") {
        return Some("m4a");
    }
    if normalized.contains("audio/flac") {
        return Some("flac");
    }
    if normalized.contains("audio/ogg") {
        return Some("ogg");
    }
    None
}

fn merge_json_field(target: &mut Value, field: &str, raw_json: Option<String>) -> Result<()> {
    if let Some(raw_json) = raw_json {
        let parsed: Value = serde_json::from_str(&raw_json)
            .with_context(|| format!("Failed to parse {field}: expected JSON."))?;
        match target.get_mut(field) {
            Some(existing) => merge_json(existing, parsed),
            None => {
                target[field] = parsed;
            }
        }
    }
    Ok(())
}

fn merge_json(existing: &mut Value, incoming: Value) {
    if let (Some(existing_obj), Some(incoming_obj)) =
        (existing.as_object_mut(), incoming.as_object())
    {
        for (key, value) in incoming_obj {
            existing_obj.insert(key.clone(), value.clone());
        }
    } else {
        *existing = incoming;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use base64::Engine;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client_for_base_url(base_url: String) -> MiniMaxClient {
        let config = Config {
            api_key: Some("test".to_string()),
            base_url: Some(base_url),
            ..Config::default()
        };
        MiniMaxClient::new(&config).expect("create client")
    }

    #[tokio::test]
    async fn t2a_saves_audio_to_output_dir() {
        let server = MockServer::start().await;
        let wav_bytes = b"RIFF\x00\x00\x00\x00WAVE".to_vec();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&wav_bytes);

        Mock::given(method("POST"))
            .and(path("/v1/t2a_v2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "base_resp": { "status_code": 0, "status_msg": "success" },
                "audio_base64": b64
            })))
            .mount(&server)
            .await;

        let client = client_for_base_url(server.uri());
        let dir = tempfile::tempdir().expect("tempdir");
        let options = T2aOptions {
            model: "speech-02-hd".to_string(),
            text: "hello".to_string(),
            stream: false,
            output_format: None,
            voice_id: None,
            voice_setting_json: None,
            audio_setting_json: None,
            pronunciation_dict_json: None,
            timber_weights_json: None,
            language_boost_json: None,
            voice_modify_json: None,
            subtitle_enable: None,
            output_dir: dir.path().to_path_buf(),
        };

        let path = t2a(&client, options).await.expect("t2a");
        assert!(path.starts_with(dir.path()));
        assert!(path.exists());
        assert_eq!(std::fs::read(&path).expect("read audio"), wav_bytes);
    }

    #[test]
    fn decode_hex_or_base64_decodes_hex_payload() {
        let bytes = decode_hex_or_base64("494433040000").expect("decode hex");
        assert!(bytes.starts_with(b"ID3"));
    }
}
