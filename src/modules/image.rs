//! Image generation API wrappers for `MiniMax`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use base64::Engine;
use colored::Colorize;
use serde_json::{Value, json};

use crate::client::MiniMaxClient;
use crate::palette;
use crate::utils::{
    extension_from_url, output_path, pretty_json, timestamped_filename, write_bytes,
};

// === Types ===

/// Options for image generation requests.
pub struct ImageGenerateOptions {
    pub model: String,
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub aspect_ratio: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub style: Option<String>,
    pub response_format: Option<String>,
    pub seed: Option<u32>,
    pub n: Option<u32>,
    pub prompt_optimizer: Option<bool>,
    pub subject_reference: Vec<String>,
    pub output_dir: PathBuf,
}

// === Request Helpers ===

fn build_image_request(options: &ImageGenerateOptions, include_optional: bool) -> Value {
    let mut body = json!({
        "model": options.model,
        "prompt": options.prompt,
    });

    if !include_optional {
        return body;
    }

    if let Some(negative_prompt) = options.negative_prompt.clone() {
        body["negative_prompt"] = json!(negative_prompt);
    }
    if let Some(aspect_ratio) = options.aspect_ratio.clone() {
        body["aspect_ratio"] = json!(aspect_ratio);
    }
    if let Some(width) = options.width {
        body["width"] = json!(width);
    }
    if let Some(height) = options.height {
        body["height"] = json!(height);
    }
    if let Some(style) = options.style.clone() {
        body["style"] = json!(style);
    }
    if let Some(response_format) = options.response_format.clone() {
        body["response_format"] = json!(response_format);
    }
    if let Some(seed) = options.seed {
        body["seed"] = json!(seed);
    }
    if let Some(n) = options.n {
        body["n"] = json!(n);
    }
    if let Some(prompt_optimizer) = options.prompt_optimizer {
        body["prompt_optimizer"] = json!(prompt_optimizer);
    }
    if !options.subject_reference.is_empty() {
        body["subject_reference"] = json!(options.subject_reference);
    }

    body
}

fn has_optional_fields(options: &ImageGenerateOptions) -> bool {
    options.negative_prompt.is_some()
        || options.aspect_ratio.is_some()
        || options.width.is_some()
        || options.height.is_some()
        || options.style.is_some()
        || options.response_format.is_some()
        || options.seed.is_some()
        || options.n.is_some()
        || options.prompt_optimizer.is_some()
        || !options.subject_reference.is_empty()
}

fn is_invalid_params(response: &Value) -> bool {
    let status_code = response
        .get("base_resp")
        .and_then(|base| base.get("status_code"))
        .and_then(serde_json::Value::as_i64);
    let status_msg = response
        .get("base_resp")
        .and_then(|base| base.get("status_msg"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    status_code == Some(2013) || status_msg.to_lowercase().contains("invalid")
}

pub async fn generate(
    client: &MiniMaxClient,
    options: ImageGenerateOptions,
) -> Result<Vec<PathBuf>> {
    let mut response: Value = client
        .post_json("/v1/image_generation", &build_image_request(&options, true))
        .await?;
    let mut payloads = extract_image_payloads(&response);

    if payloads.is_empty() && is_invalid_params(&response) && has_optional_fields(&options) {
        let retry_response: Value = client
            .post_json(
                "/v1/image_generation",
                &build_image_request(&options, false),
            )
            .await?;
        let retry_payloads = extract_image_payloads(&retry_response);
        response = retry_response;
        if !retry_payloads.is_empty() {
            payloads = retry_payloads;
        }
    }

    if payloads.is_empty() {
        let status_code = response
            .get("base_resp")
            .and_then(|base| base.get("status_code"))
            .and_then(serde_json::Value::as_i64);
        let status_msg = response
            .get("base_resp")
            .and_then(|base| base.get("status_msg"))
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let message = status_code.map_or_else(
            || "Failed to generate image: no payloads found in response.".to_string(),
            |code| format!("Failed to generate image: {code} {status_msg}"),
        );
        anyhow::bail!("{} Response: {}", message, pretty_json(&response));
    }

    let mut saved_paths = Vec::new();
    let total = payloads.len();
    for (index, payload) in payloads.into_iter().enumerate() {
        let bytes = match payload {
            ImagePayload::Url(url) => {
                let bytes = client.get_bytes(&url).await?;
                (
                    bytes.to_vec(),
                    extension_from_url(&url).unwrap_or_else(|| "png".to_string()),
                )
            }
            ImagePayload::Base64(data) => {
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(data.trim())
                    .context("Failed to decode image payload: invalid base64 data.")?;
                (decoded, "png".to_string())
            }
        };

        let filename = if total == 1 {
            timestamped_filename("image", &bytes.1)
        } else {
            timestamped_filename(&format!("image_{}", index + 1), &bytes.1)
        };
        let path = output_path(&options.output_dir, &filename);
        write_bytes(&path, &bytes.0)?;
        let (r, g, b) = palette::MINIMAX_GREEN_RGB;
        println!("{} {}", "Saved".truecolor(r, g, b).bold(), path.display());
        saved_paths.push(path);
    }

    Ok(saved_paths)
}

enum ImagePayload {
    Url(String),
    Base64(String),
}

fn extract_image_payloads(response: &Value) -> Vec<ImagePayload> {
    let mut payloads = Vec::new();
    if let Some(data) = response.get("data") {
        if let Some(items) = data.as_array() {
            for item in items {
                if let Some(url) = item
                    .get("url")
                    .or_else(|| item.get("image_url"))
                    .and_then(|value| value.as_str())
                {
                    payloads.push(ImagePayload::Url(url.to_string()));
                } else if let Some(b64) = item
                    .get("b64_json")
                    .or_else(|| item.get("image_base64"))
                    .or_else(|| item.get("image"))
                    .and_then(|value| value.as_str())
                {
                    payloads.push(ImagePayload::Base64(b64.to_string()));
                }
            }
        } else if let Some(obj) = data.as_object() {
            if let Some(urls) = obj.get("image_urls").and_then(|value| value.as_array()) {
                for url in urls.iter().filter_map(|value| value.as_str()) {
                    payloads.push(ImagePayload::Url(url.to_string()));
                }
            }
            if let Some(url) = obj
                .get("image_url")
                .or_else(|| obj.get("url"))
                .and_then(|value| value.as_str())
            {
                payloads.push(ImagePayload::Url(url.to_string()));
            }
            if let Some(b64s) = obj
                .get("image_base64")
                .or_else(|| obj.get("b64_json"))
                .and_then(|value| value.as_array())
            {
                for b64 in b64s.iter().filter_map(|value| value.as_str()) {
                    payloads.push(ImagePayload::Base64(b64.to_string()));
                }
            }
            if let Some(b64) = obj
                .get("image")
                .or_else(|| obj.get("image_base64"))
                .or_else(|| obj.get("b64_json"))
                .and_then(|value| value.as_str())
            {
                payloads.push(ImagePayload::Base64(b64.to_string()));
            }
        }
    }

    if payloads.is_empty() {
        if let Some(url) = response
            .get("image_url")
            .or_else(|| response.get("url"))
            .and_then(|value| value.as_str())
        {
            payloads.push(ImagePayload::Url(url.to_string()));
        } else if let Some(b64) = response
            .get("image_base64")
            .or_else(|| response.get("b64_json"))
            .and_then(|value| value.as_str())
        {
            payloads.push(ImagePayload::Base64(b64.to_string()));
        }
    }

    payloads
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
    async fn generate_saves_image_to_output_dir() {
        let server = MockServer::start().await;
        let png_bytes = b"PNGDATA".to_vec();
        let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

        Mock::given(method("POST"))
            .and(path("/v1/image_generation"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "base_resp": { "status_code": 0, "status_msg": "success" },
                "data": [{ "b64_json": b64 }]
            })))
            .mount(&server)
            .await;

        let client = client_for_base_url(server.uri());
        let dir = tempfile::tempdir().expect("tempdir");
        let options = ImageGenerateOptions {
            model: "image-01".to_string(),
            prompt: "test".to_string(),
            negative_prompt: None,
            aspect_ratio: None,
            width: None,
            height: None,
            style: None,
            response_format: None,
            seed: None,
            n: None,
            prompt_optimizer: None,
            subject_reference: Vec::new(),
            output_dir: dir.path().to_path_buf(),
        };

        let paths = generate(&client, options).await.expect("generate image");
        assert_eq!(paths.len(), 1);
        let path = &paths[0];
        assert!(path.starts_with(dir.path()));
        assert!(path.exists());
        assert_eq!(std::fs::read(path).expect("read image"), png_bytes);
    }
}
