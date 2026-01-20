//! Video generation API wrappers for `MiniMax`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use base64::Engine;
use colored::Colorize;
use serde_json::{Value, json};
use tokio::time::{Duration, sleep};

use crate::client::MiniMaxClient;
use crate::modules::files::retrieve_file;
use crate::palette;
use crate::ui::spinner;
use crate::utils::{
    extension_from_url, output_path, pretty_json, timestamped_filename, write_bytes,
};

// === Types ===

/// Result of a video generation request.
pub struct VideoGenerateResult {
    pub task_id: Option<String>,
    pub response_path: Option<PathBuf>,
    pub video_path: Option<PathBuf>,
}

/// Options for creating a video generation request.
pub struct VideoGenerateOptions {
    pub model: String,
    pub prompt: String,
    pub first_frame: Option<String>,
    pub last_frame: Option<String>,
    pub subject_reference: Vec<String>,
    pub subject_reference_json: Option<String>,
    pub duration: Option<u32>,
    pub resolution: Option<String>,
    pub callback_url: Option<String>,
    pub prompt_optimizer: Option<bool>,
    pub fast_pretreatment: Option<bool>,
    pub wait: bool,
    pub output_dir: PathBuf,
}

/// Options for querying a video generation task.
pub struct VideoQueryOptions {
    pub task_id: String,
}

/// Options for creating a templated video request.
pub struct VideoAgentCreateOptions {
    pub template_id: String,
    pub text_inputs: Option<Value>,
    pub media_inputs: Option<Value>,
    pub callback_url: Option<String>,
}

// === API Calls ===

pub async fn generate(
    client: &MiniMaxClient,
    options: VideoGenerateOptions,
) -> Result<VideoGenerateResult> {
    let mut body = json!({
        "model": options.model,
        "prompt": options.prompt,
    });

    if let Some(first_frame) = options.first_frame {
        body["first_frame_image"] = json!(resolve_media_input(&first_frame)?);
    }
    if let Some(last_frame) = options.last_frame {
        body["last_frame_image"] = json!(resolve_media_input(&last_frame)?);
    }
    if let Some(duration) = options.duration {
        body["duration"] = json!(duration);
    }
    if let Some(resolution) = options.resolution {
        body["resolution"] = json!(resolution);
    }
    if let Some(callback_url) = options.callback_url {
        body["callback_url"] = json!(callback_url);
    }
    if let Some(prompt_optimizer) = options.prompt_optimizer {
        body["prompt_optimizer"] = json!(prompt_optimizer);
    }
    if let Some(fast_pretreatment) = options.fast_pretreatment {
        body["fast_pretreatment"] = json!(fast_pretreatment);
    }

    if let Some(raw_json) = options.subject_reference_json {
        let parsed: Value = serde_json::from_str(&raw_json)
            .context("Failed to parse subject_reference_json: expected JSON.")?;
        body["subject_reference"] = parsed;
    } else if !options.subject_reference.is_empty() {
        let values: Vec<String> = options
            .subject_reference
            .iter()
            .map(|value| resolve_media_input(value))
            .collect::<Result<Vec<_>>>()?;
        body["subject_reference"] = json!(values);
    }

    let response: Value = client.post_json("/v1/video_generation", &body).await?;
    ensure_success(&response)?;
    println!("{}", pretty_json(&response));

    let task_id = extract_task_id(&response);

    if let Some(path) = download_video_if_available(client, &response, &options.output_dir).await? {
        let (r, g, b) = palette::MINIMAX_GREEN_RGB;
        println!("{} {}", "Saved".truecolor(r, g, b).bold(), path.display());
        return Ok(VideoGenerateResult {
            task_id,
            response_path: None,
            video_path: Some(path),
        });
    }

    if !options.wait {
        let response_path = save_response_json(&options.output_dir, &response)?;
        return Ok(VideoGenerateResult {
            task_id,
            response_path: Some(response_path),
            video_path: None,
        });
    }

    let Some(task_id) = task_id else {
        let response_path = save_response_json(&options.output_dir, &response)?;
        anyhow::bail!(
            "Video generation did not return task_id or downloadable URL. Response saved to {}. Response: {}",
            response_path.display(),
            pretty_json(&response)
        );
    };

    let spinner = spinner("Generating video...");
    let final_response = wait_for_video(client, &task_id).await?;
    spinner.finish_and_clear();
    ensure_success(&final_response)?;
    println!("{}", pretty_json(&final_response));

    if let Some(status) = extract_status(&final_response) {
        let status_upper = status.to_uppercase();
        if status_upper.contains("FAIL") || status_upper.contains("ERROR") {
            let response_path = save_response_json(&options.output_dir, &final_response)?;
            anyhow::bail!(
                "Video generation failed: status={status}. Response saved to {}. Response: {}",
                response_path.display(),
                pretty_json(&final_response)
            );
        }
    }

    let video_path =
        download_video_if_available(client, &final_response, &options.output_dir).await?;

    if video_path.is_none() {
        let response_path = save_response_json(&options.output_dir, &final_response)?;
        anyhow::bail!(
            "Video generation completed but no downloadable URL returned. Response saved to {}. Response: {}",
            response_path.display(),
            pretty_json(&final_response)
        );
    }

    if let Some(path) = &video_path {
        let (r, g, b) = palette::MINIMAX_GREEN_RGB;
        println!("{} {}", "Saved".truecolor(r, g, b).bold(), path.display());
    }

    Ok(VideoGenerateResult {
        task_id: Some(task_id),
        response_path: None,
        video_path,
    })
}

pub async fn query(client: &MiniMaxClient, options: VideoQueryOptions) -> Result<()> {
    let response: Value = client
        .get_json(
            "/v1/query/video_generation",
            Some(&[("task_id", options.task_id.as_str())]),
        )
        .await?;
    println!("{}", pretty_json(&response));
    Ok(())
}

pub async fn agent_create(
    client: &MiniMaxClient,
    options: VideoAgentCreateOptions,
) -> Result<Value> {
    let mut body = json!({ "template_id": options.template_id });

    if let Some(text_inputs) = options.text_inputs {
        body["text_inputs"] = text_inputs;
    }
    if let Some(media_inputs) = options.media_inputs {
        body["media_inputs"] = media_inputs;
    }
    if let Some(callback_url) = options.callback_url {
        body["callback_url"] = json!(callback_url);
    }

    let response: Value = client
        .post_json("/v1/video_template_generation", &body)
        .await?;
    Ok(response)
}

pub async fn agent_query(client: &MiniMaxClient, task_id: &str) -> Result<Value> {
    let response: Value = client
        .get_json(
            "/v1/query/video_template_generation",
            Some(&[("task_id", task_id)]),
        )
        .await?;
    Ok(response)
}

async fn wait_for_video(client: &MiniMaxClient, task_id: &str) -> Result<Value> {
    loop {
        let response: Value = client
            .get_json("/v1/query/video_generation", Some(&[("task_id", task_id)]))
            .await?;

        if let Some(status) = extract_status(&response) {
            let status_upper = status.to_uppercase();
            if status_upper.contains("SUCCESS")
                || status_upper.contains("FINISH")
                || status_upper.contains("DONE")
            {
                return Ok(response);
            }
            if status_upper.contains("FAIL") || status_upper.contains("ERROR") {
                return Ok(response);
            }
        }

        sleep(Duration::from_secs(3)).await;
    }
}

async fn download_video_if_available(
    client: &MiniMaxClient,
    response: &Value,
    output_dir: &Path,
) -> Result<Option<PathBuf>> {
    if let Some(url) = extract_video_url(response) {
        let bytes = client.get_bytes(&url).await?;
        let extension = extension_from_url(&url).unwrap_or_else(|| "mp4".to_string());
        let filename = timestamped_filename("video", &extension);
        let path = output_path(output_dir, &filename);
        write_bytes(&path, &bytes)?;
        return Ok(Some(path));
    }

    if let Some(file_id) = extract_file_id(response)
        && let Some(url) = retrieve_file(client, &file_id, Some("video")).await?
    {
        let bytes = client.get_bytes(&url).await?;
        let extension = extension_from_url(&url).unwrap_or_else(|| "mp4".to_string());
        let filename = timestamped_filename("video", &extension);
        let path = output_path(output_dir, &filename);
        write_bytes(&path, &bytes)?;
        return Ok(Some(path));
    }

    Ok(None)
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
            "MiniMax video_generation error: {status_code} {status_msg}. Response: {}",
            pretty_json(response)
        );
    }

    Ok(())
}

fn save_response_json(output_dir: &Path, response: &Value) -> Result<PathBuf> {
    let filename = timestamped_filename("video_response", "json");
    let path = output_path(output_dir, &filename);
    write_bytes(&path, pretty_json(response).as_bytes())?;
    Ok(path)
}

fn extract_task_id(response: &Value) -> Option<String> {
    response
        .get("task_id")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
        .or_else(|| {
            response
                .get("data")
                .and_then(|v| v.get("task_id"))
                .and_then(|v| v.as_str())
                .map(std::string::ToString::to_string)
        })
}

fn extract_status(response: &Value) -> Option<String> {
    response
        .get("status")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
        .or_else(|| {
            response
                .get("data")
                .and_then(|v| v.get("status"))
                .and_then(|v| v.as_str())
                .map(std::string::ToString::to_string)
        })
}

fn extract_video_url(response: &Value) -> Option<String> {
    if let Some(url) = response
        .get("video_url")
        .or_else(|| response.get("url"))
        .and_then(|value| value.as_str())
    {
        return Some(url.to_string());
    }

    if let Some(data) = response.get("data")
        && let Some(url) = data
            .get("video_url")
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

fn resolve_media_input(value: &str) -> Result<String> {
    let path = Path::new(value);
    if !path.exists() {
        return Ok(value.to_string());
    }

    let bytes = std::fs::read(path)
        .with_context(|| format!("Failed to read media file: {}", path.display()))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    let mime = match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    };
    Ok(format!("data:{mime};base64,{encoded}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use wiremock::matchers::{method, path, query_param};
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
    async fn generate_waits_and_downloads_video_to_output_dir() {
        let server = MockServer::start().await;
        let task_id = "task-123";
        let video_bytes = b"MP4DATA".to_vec();
        let video_url = format!("{}/video.mp4", server.uri());

        Mock::given(method("POST"))
            .and(path("/v1/video_generation"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "base_resp": { "status_code": 0, "status_msg": "success" },
                "task_id": task_id
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/v1/query/video_generation"))
            .and(query_param("task_id", task_id))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "base_resp": { "status_code": 0, "status_msg": "success" },
                "status": "SUCCESS",
                "video_url": video_url
            })))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/video.mp4"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(video_bytes.clone()))
            .mount(&server)
            .await;

        let client = client_for_base_url(server.uri());
        let dir = tempfile::tempdir().expect("tempdir");
        let options = VideoGenerateOptions {
            model: "video-01".to_string(),
            prompt: "test".to_string(),
            first_frame: None,
            last_frame: None,
            subject_reference: Vec::new(),
            subject_reference_json: None,
            duration: None,
            resolution: None,
            callback_url: None,
            prompt_optimizer: None,
            fast_pretreatment: None,
            wait: true,
            output_dir: dir.path().to_path_buf(),
        };

        let result = generate(&client, options).await.expect("generate video");
        assert_eq!(result.task_id.as_deref(), Some(task_id));
        assert!(result.response_path.is_none());

        let video_path = result.video_path.expect("video path");
        assert!(video_path.starts_with(dir.path()));
        assert!(video_path.exists());
        assert_eq!(std::fs::read(&video_path).expect("read video"), video_bytes);
    }
}
