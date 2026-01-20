//! File upload and retrieval helpers for `MiniMax` APIs.

use std::path::PathBuf;

use anyhow::{Context, Result};
use colored::Colorize;
use futures_util::TryStreamExt;
use serde_json::{Value, json};
use tokio_util::io::ReaderStream;

use crate::client::MiniMaxClient;
use crate::palette;
use crate::ui::progress_bar;
use crate::utils::{output_path, pretty_json, write_bytes};

// === Types ===

/// Options for uploading a file.
pub struct FileUploadOptions {
    pub path: PathBuf,
    pub purpose: String,
}

/// Options for listing files.
pub struct FileListOptions {
    pub purpose: String,
}

/// Options for deleting a file.
pub struct FileDeleteOptions {
    pub file_id: String,
    pub purpose: Option<String>,
}

/// Options for retrieving a file record.
pub struct FileRetrieveOptions {
    pub file_id: String,
    pub purpose: Option<String>,
}

/// Options for downloading a file's content.
pub struct FileRetrieveContentOptions {
    pub file_id: String,
    pub output: Option<PathBuf>,
    pub output_dir: PathBuf,
}

// === API Calls ===

pub async fn upload(client: &MiniMaxClient, options: FileUploadOptions) -> Result<Value> {
    let file = tokio::fs::File::open(&options.path)
        .await
        .with_context(|| format!("Failed to open {}", options.path.display()))?;
    let metadata = file.metadata().await?;
    let total = metadata.len();
    let filename = options
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("upload.bin")
        .to_string();

    let progress = progress_bar(total, "Uploading file...");
    let progress_clone = progress.clone();
    let stream = ReaderStream::new(file).map_ok(move |chunk| {
        progress_clone.inc(chunk.len() as u64);
        chunk
    });
    let body = reqwest::Body::wrap_stream(stream);
    let part = reqwest::multipart::Part::stream_with_length(body, total).file_name(filename);
    let form = reqwest::multipart::Form::new()
        .text("purpose", options.purpose)
        .part("file", part);

    let response: Value = client.post_multipart("/v1/files/upload", form).await?;
    progress.finish_and_clear();
    println!("{}", pretty_json(&response));
    Ok(response)
}

pub async fn list(client: &MiniMaxClient, options: FileListOptions) -> Result<Value> {
    let response: Value = client
        .get_json(
            "/v1/files/list",
            Some(&[("purpose", options.purpose.as_str())]),
        )
        .await?;
    Ok(response)
}

pub async fn delete(client: &MiniMaxClient, options: FileDeleteOptions) -> Result<Value> {
    let mut body = json!({ "file_id": options.file_id });
    if let Some(purpose) = options.purpose {
        body["purpose"] = json!(purpose);
    }
    let response: Value = client.post_json("/v1/files/delete", &body).await?;
    Ok(response)
}

pub async fn retrieve(
    client: &MiniMaxClient,
    options: FileRetrieveOptions,
) -> Result<Option<String>> {
    let url = retrieve_file(client, &options.file_id, options.purpose.as_deref()).await?;
    Ok(url)
}

pub async fn retrieve_content(
    client: &MiniMaxClient,
    options: FileRetrieveContentOptions,
) -> Result<PathBuf> {
    let (bytes, content_type) = client
        .get_bytes_with_query(
            "/v1/files/retrieve_content",
            &[("file_id", options.file_id.as_str())],
        )
        .await?;

    let extension = content_type
        .as_deref()
        .and_then(extension_from_content_type)
        .unwrap_or("bin");

    let filename = format!("file_{}.{}", options.file_id, extension);
    let path = options
        .output
        .unwrap_or_else(|| output_path(&options.output_dir, &filename));
    write_bytes(&path, &bytes)?;
    Ok(path)
}

pub async fn retrieve_file(
    client: &MiniMaxClient,
    file_id: &str,
    purpose: Option<&str>,
) -> Result<Option<String>> {
    let mut query: Vec<(&str, &str)> = vec![("file_id", file_id)];
    if let Some(purpose) = purpose {
        query.push(("purpose", purpose));
    }

    // MiniMax expects GET with query parameters:
    // https://platform.minimax.io/docs/api-reference/file-management-retrieve
    let response: Value = client.get_json("/v1/files/retrieve", Some(&query)).await?;

    if let Some(status_code) = response
        .get("base_resp")
        .and_then(|base| base.get("status_code"))
        .and_then(serde_json::Value::as_i64)
        && status_code != 0
    {
        let status_msg = response
            .get("base_resp")
            .and_then(|base| base.get("status_msg"))
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        anyhow::bail!(
            "MiniMax files/retrieve error: {status_code} {status_msg}. Response: {}",
            pretty_json(&response)
        );
    }

    if let Some(url) = response
        .get("download_url")
        .or_else(|| response.get("file_url"))
        .or_else(|| response.get("url"))
        .and_then(|value| value.as_str())
    {
        return Ok(Some(url.to_string()));
    }

    if let Some(file) = response.get("file")
        && let Some(url) = file
            .get("download_url")
            .or_else(|| file.get("file_url"))
            .or_else(|| file.get("url"))
            .and_then(|value| value.as_str())
    {
        return Ok(Some(url.to_string()));
    }

    if let Some(data) = response.get("data") {
        if let Some(url) = data
            .get("download_url")
            .or_else(|| data.get("file_url"))
            .or_else(|| data.get("url"))
            .and_then(|value| value.as_str())
        {
            return Ok(Some(url.to_string()));
        }
        if let Some(file) = data.get("file")
            && let Some(url) = file
                .get("download_url")
                .or_else(|| file.get("file_url"))
                .or_else(|| file.get("url"))
                .and_then(|value| value.as_str())
        {
            return Ok(Some(url.to_string()));
        }
    }

    println!(
        "{}",
        {
            let (r, g, b) = palette::MINIMAX_ORANGE_RGB;
            "Failed to retrieve file: no download URL returned.".truecolor(r, g, b)
        }
    );
    println!("{}", pretty_json(&response));
    Ok(None)
}

fn extension_from_content_type(content_type: &str) -> Option<&'static str> {
    let normalized = content_type.to_lowercase();
    if normalized.contains("pdf") {
        return Some("pdf");
    }
    if normalized.contains("msword") {
        return Some("doc");
    }
    if normalized.contains("officedocument.wordprocessingml") {
        return Some("docx");
    }
    if normalized.contains("json") {
        return Some("jsonl");
    }
    if normalized.contains("text/plain") {
        return Some("txt");
    }
    if normalized.contains("audio/mpeg") {
        return Some("mp3");
    }
    if normalized.contains("audio/wav") || normalized.contains("audio/wave") {
        return Some("wav");
    }
    if normalized.contains("audio/mp4") || normalized.contains("audio/x-m4a") {
        return Some("m4a");
    }
    None
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
    async fn retrieve_file_parses_download_url_from_file_object() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/files/retrieve"))
            .and(query_param("file_id", "123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "base_resp": { "status_code": 0, "status_msg": "success" },
                "file": { "download_url": "https://example.com/file.mp4" }
            })))
            .mount(&server)
            .await;

        let client = client_for_base_url(server.uri());
        let url = retrieve_file(&client, "123", None)
            .await
            .expect("retrieve file");
        assert_eq!(url.as_deref(), Some("https://example.com/file.mp4"));
    }

    #[tokio::test]
    async fn retrieve_file_bails_on_base_resp_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/files/retrieve"))
            .and(query_param("file_id", "123"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "base_resp": { "status_code": 2013, "status_msg": "invalid params" }
            })))
            .mount(&server)
            .await;

        let client = client_for_base_url(server.uri());
        let result = retrieve_file(&client, "123", None).await;
        assert!(result.is_err());
    }
}
