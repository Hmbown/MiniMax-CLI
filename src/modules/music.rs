//! Music generation API wrappers for `MiniMax`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::Engine;
use colored::Colorize;
use serde_json::{Value, json};

use crate::client::MiniMaxClient;
use crate::modules::files::retrieve_file;
use crate::palette;
use crate::utils::{
    extension_from_url, output_path, pretty_json, timestamped_filename, write_bytes,
};

// === Types ===

/// Options for music generation requests.
pub struct MusicGenerateOptions {
    pub model: String,
    pub prompt: String,
    pub lyrics: Option<String>,
    pub stream: bool,
    pub output_format: Option<String>,
    pub audio_setting_json: Option<String>,
    pub output_dir: PathBuf,
}

// === API Calls ===

pub async fn generate(client: &MiniMaxClient, options: MusicGenerateOptions) -> Result<PathBuf> {
    let prompt_for_fallback = options.prompt.clone();
    let has_lyrics = options.lyrics.is_some();
    let mut body = json!({
        "model": options.model,
        "prompt": options.prompt,
    });

    if options.stream {
        body["stream"] = json!(true);
    }

    if let Some(lyrics) = options.lyrics {
        body["lyrics"] = json!(lyrics);
    }
    if let Some(output_format) = options.output_format.clone() {
        body["output_format"] = json!(output_format);
    }
    if let Some(audio_setting) = options.audio_setting_json {
        let parsed: Value = serde_json::from_str(&audio_setting)
            .context("Failed to parse audio_setting_json: expected JSON.")?;
        body["audio_setting"] = parsed;
    }

    if options.stream {
        let response = client.post_json_raw("/v1/music_generation", &body).await?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = response.bytes().await?;

        if let Some(value) = parse_json_bytes(&content_type, &bytes) {
            if !has_lyrics && is_lyrics_too_short(&value) {
                let mut retry_body = body.clone();
                retry_body["lyrics"] = json!(fallback_lyrics(&prompt_for_fallback));
                let retry_response: Value = client
                    .post_json("/v1/music_generation", &retry_body)
                    .await?;
                return handle_music_response(client, &retry_response, &options.output_dir).await;
            }
            return handle_music_response(client, &value, &options.output_dir).await;
        }

        let extension = options
            .output_format
            .or_else(|| {
                extension_from_content_type(&content_type).map(std::string::ToString::to_string)
            })
            .or_else(|| infer_audio_extension(&bytes).map(std::string::ToString::to_string))
            .unwrap_or_else(|| "bin".to_string());
        let filename = timestamped_filename("music", &extension);
        let path = output_path(&options.output_dir, &filename);
        write_bytes(&path, &bytes)?;
        let (r, g, b) = palette::MINIMAX_GREEN_RGB;
        println!("{} {}", "Saved".truecolor(r, g, b).bold(), path.display());
        Ok(path)
    } else {
        let response: Value = client.post_json("/v1/music_generation", &body).await?;
        match handle_music_response(client, &response, &options.output_dir).await {
            Ok(path) => Ok(path),
            Err(e) => {
                if !has_lyrics && is_lyrics_too_short(&response) {
                    let mut retry_body = body.clone();
                    retry_body["lyrics"] = json!(fallback_lyrics(&prompt_for_fallback));
                    let retry_response: Value = client
                        .post_json("/v1/music_generation", &retry_body)
                        .await?;
                    handle_music_response(client, &retry_response, &options.output_dir).await
                } else {
                    Err(e)
                }
            }
        }
    }
}

fn is_lyrics_too_short(response: &Value) -> bool {
    let status_msg = response
        .get("base_resp")
        .and_then(|base| base.get("status_msg"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    let normalized = status_msg.to_ascii_lowercase();
    normalized.contains("lyrics") && normalized.contains("too short")
}

fn fallback_lyrics(prompt: &str) -> String {
    format!(
        "Instrumental track. No vocals.\n\nPrompt:\n{prompt}\n\nGuidance:\n- No singing or spoken word\n- Use the prompt as style guidance\n\n{prompt}\n{prompt}\n"
    )
}

async fn handle_music_response(
    client: &MiniMaxClient,
    response: &Value,
    output_dir: &Path,
) -> Result<PathBuf> {
    ensure_success(response)?;

    if let Some(url) = extract_music_url(response) {
        let bytes = client.get_bytes(&url).await?;
        let extension = extension_from_url(&url)
            .or_else(|| infer_audio_extension(&bytes).map(std::string::ToString::to_string))
            .unwrap_or_else(|| "bin".to_string());
        let filename = timestamped_filename("music", &extension);
        let path = output_path(output_dir, &filename);
        write_bytes(&path, &bytes)?;
        let (r, g, b) = palette::MINIMAX_GREEN_RGB;
        println!("{} {}", "Saved".truecolor(r, g, b).bold(), path.display());
        return Ok(path);
    }

    if let Some(file_id) = extract_file_id(response)
        && let Some(url) = retrieve_file(client, &file_id, Some("audio")).await?
    {
        let bytes = client.get_bytes(&url).await?;
        let extension = extension_from_url(&url)
            .or_else(|| infer_audio_extension(&bytes).map(std::string::ToString::to_string))
            .unwrap_or_else(|| "bin".to_string());
        let filename = timestamped_filename("music", &extension);
        let path = output_path(output_dir, &filename);
        write_bytes(&path, &bytes)?;
        let (r, g, b) = palette::MINIMAX_GREEN_RGB;
        println!("{} {}", "Saved".truecolor(r, g, b).bold(), path.display());
        return Ok(path);
    }

    if let Some(b64) = extract_music_base64(response) {
        let bytes = decode_hex_or_base64(b64.trim())
            .context("Failed to decode music payload: expected hex or base64 data.")?;
        let extension = infer_audio_extension(&bytes)
            .map(std::string::ToString::to_string)
            .unwrap_or_else(|| "bin".to_string());
        let filename = timestamped_filename("music", &extension);
        let path = output_path(output_dir, &filename);
        write_bytes(&path, &bytes)?;
        let (r, g, b) = palette::MINIMAX_GREEN_RGB;
        println!("{} {}", "Saved".truecolor(r, g, b).bold(), path.display());
        return Ok(path);
    }

    anyhow::bail!(
        "Failed to generate music: no audio payload found in response. Response: {}",
        pretty_json(response)
    )
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
            "MiniMax music_generation error: {status_code} {status_msg}. Response: {}",
            pretty_json(response)
        );
    }

    Ok(())
}

fn extract_music_url(response: &Value) -> Option<String> {
    if let Some(url) = response
        .get("audio_url")
        .or_else(|| response.get("url"))
        .and_then(|value| value.as_str())
    {
        return Some(url.to_string());
    }

    if let Some(data) = response.get("data") {
        if let Some(url) = data
            .get("audio_url")
            .or_else(|| data.get("url"))
            .and_then(|value| value.as_str())
        {
            return Some(url.to_string());
        }
        if let Some(items) = data.as_array() {
            for item in items {
                if let Some(url) = item
                    .get("audio_url")
                    .or_else(|| item.get("url"))
                    .and_then(|value| value.as_str())
                {
                    return Some(url.to_string());
                }
            }
        }
    }

    None
}

fn extract_music_base64(response: &Value) -> Option<String> {
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

fn extract_file_id(response: &Value) -> Option<String> {
    if let Some(file_id) = response
        .get("file_id")
        .and_then(|value| value.as_str())
        .map(std::string::ToString::to_string)
    {
        return Some(file_id);
    }

    if let Some(data) = response.get("data") {
        if let Some(file_id) = data
            .get("file_id")
            .and_then(|value| value.as_str())
            .map(std::string::ToString::to_string)
        {
            return Some(file_id);
        }

        if let Some(items) = data.as_array() {
            for item in items {
                if let Some(file_id) = item
                    .get("file_id")
                    .and_then(|value| value.as_str())
                    .map(std::string::ToString::to_string)
                {
                    return Some(file_id);
                }
            }
        }
    }

    None
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use base64::Engine;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn dummy_client() -> MiniMaxClient {
        let config = Config {
            api_key: Some("test".to_string()),
            base_url: Some("https://api.minimax.io".to_string()),
            ..Config::default()
        };
        MiniMaxClient::new(&config).expect("create dummy client")
    }

    fn client_for_base_url(base_url: String) -> MiniMaxClient {
        let config = Config {
            api_key: Some("test".to_string()),
            base_url: Some(base_url),
            ..Config::default()
        };
        MiniMaxClient::new(&config).expect("create client")
    }

    #[test]
    fn parse_json_bytes_detects_error_payload() {
        let bytes = br#"{"base_resp":{"status_code":2013,"status_msg":"invalid params"}}"#;
        let parsed = parse_json_bytes("application/octet-stream", bytes);
        assert!(parsed.is_some());
    }

    #[test]
    fn infer_audio_extension_detects_wav() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&[0u8; 4]);
        bytes.extend_from_slice(b"WAVE");
        assert_eq!(infer_audio_extension(&bytes), Some("wav"));
    }

    #[test]
    fn infer_audio_extension_detects_mp3() {
        assert_eq!(infer_audio_extension(b"ID3\x04\x00\x00"), Some("mp3"));
    }

    #[test]
    fn decode_hex_or_base64_decodes_hex_payload() {
        let bytes = decode_hex_or_base64("494433040000").expect("decode hex");
        assert!(bytes.starts_with(b"ID3"));
    }

    #[tokio::test]
    async fn generate_saves_music_to_output_dir() {
        let server = MockServer::start().await;
        let wav_bytes = b"RIFF\x00\x00\x00\x00WAVE".to_vec();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&wav_bytes);

        Mock::given(method("POST"))
            .and(path("/v1/music_generation"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "base_resp": { "status_code": 0, "status_msg": "success" },
                "data": { "audio_base64": b64 }
            })))
            .mount(&server)
            .await;

        let client = client_for_base_url(server.uri());
        let dir = tempfile::tempdir().expect("tempdir");
        let options = MusicGenerateOptions {
            model: "music-1.5".to_string(),
            prompt: "test".to_string(),
            lyrics: None,
            stream: false,
            output_format: None,
            audio_setting_json: None,
            output_dir: dir.path().to_path_buf(),
        };

        let path = generate(&client, options).await.expect("generate music");
        assert!(path.starts_with(dir.path()));
        assert!(path.exists());
        assert_eq!(std::fs::read(&path).expect("read music"), wav_bytes);
    }

    #[tokio::test]
    async fn handle_music_response_bails_on_base_resp_error() {
        let client = dummy_client();
        let response = json!({
            "data": null,
            "base_resp": { "status_code": 2013, "status_msg": "invalid output_format" }
        });
        let dir = tempfile::tempdir().expect("tempdir");
        let result = handle_music_response(&client, &response, dir.path()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn handle_music_response_bails_on_missing_payload() {
        let client = dummy_client();
        let response = json!({
            "data": null,
            "base_resp": { "status_code": 0, "status_msg": "success" }
        });
        let dir = tempfile::tempdir().expect("tempdir");
        let result = handle_music_response(&client, &response, dir.path()).await;
        assert!(result.is_err());
    }

    #[test]
    fn extract_music_url_reads_nested_data() {
        let response = json!({
            "base_resp": { "status_code": 0, "status_msg": "success" },
            "data": { "audio_url": "https://example.com/audio.mp3" }
        });
        assert_eq!(
            extract_music_url(&response),
            Some("https://example.com/audio.mp3".to_string())
        );
    }

    #[test]
    fn extract_music_base64_reads_nested_data() {
        let response = json!({
            "base_resp": { "status_code": 0, "status_msg": "success" },
            "data": { "audio_base64": "SGVsbG8=" }
        });
        assert_eq!(
            extract_music_base64(&response),
            Some("SGVsbG8=".to_string())
        );
    }
}
