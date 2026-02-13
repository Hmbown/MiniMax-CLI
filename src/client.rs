//! HTTP clients for `MiniMax` APIs.
//!
//! This module centralizes retry behavior, base URLs, and streaming helpers
//! for the `MiniMax` CLI's network requests.

use std::pin::Pin;

use anyhow::Result;
use futures_util::StreamExt;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};

use crate::config::{Config, RetryPolicy};
use crate::llm_client::{LlmClient, StreamEventBox};
use crate::logging;
use crate::models::{MessageRequest, MessageResponse, StreamEvent};

// === Types ===

/// Client for `MiniMax` API requests with retry and base URL handling.
#[must_use]
pub struct MiniMaxClient {
    http_client: reqwest::Client,
    raw_http_client: reqwest::Client,
    base_url: String,
    retry: RetryPolicy,
}

/// Client for MiniMax text API requests using MiniMax's compatibility wire format.
#[derive(Clone)]
#[must_use]
pub struct MiniMaxTextClient {
    http_client: reqwest::Client,
    base_url: String,
    retry: RetryPolicy,
    #[allow(dead_code)] // For future model selection
    default_model: String,
}

// === Helpers ===

fn is_minimax_base_url(base_url: &str) -> bool {
    let base = base_url.to_lowercase();
    base.contains("api.minimax.io") || base.contains("api.minimaxi.com")
}

// === MiniMaxClient ===

impl MiniMaxClient {
    /// Create a `MiniMax` client from CLI configuration.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use crate::client::MiniMaxClient;
    /// # use crate::config::Config;
    /// # fn example(config: &Config) -> anyhow::Result<()> {
    /// let client = MiniMaxClient::new(config)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(config: &Config) -> Result<Self> {
        let api_key = config.minimax_api_key()?;
        let base_url = config.minimax_base_url();
        let retry = config.retry_policy();

        logging::info(format!("MiniMax base URL: {base_url}"));
        logging::info(format!(
            "Retry policy: enabled={}, max_retries={}, initial_delay={}s, max_delay={}s",
            retry.enabled, retry.max_retries, retry.initial_delay, retry.max_delay
        ));

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {api_key}"))?,
        );

        let http_client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;
        let raw_http_client = reqwest::Client::new();

        Ok(Self {
            http_client,
            raw_http_client,
            base_url,
            retry,
        })
    }

    /// Send a JSON POST request and deserialize the response body.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use crate::client::MiniMaxClient;
    /// # async fn example(client: &MiniMaxClient) -> anyhow::Result<()> {
    /// let response: serde_json::Value = client
    ///     .post_json("/v1/mock", &serde_json::json!({ "foo": "bar" }))
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T> {
        let url = self.url(path);
        let response =
            send_with_retry(&self.retry, || self.http_client.post(&url).json(body)).await?;
        self.parse_json_response(response).await
    }

    /// Send a JSON POST request and return the raw response.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use crate::client::MiniMaxClient;
    /// # async fn example(client: &MiniMaxClient) -> anyhow::Result<()> {
    /// let response = client
    ///     .post_json_raw("/v1/mock", &serde_json::json!({ "foo": "bar" }))
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn post_json_raw(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<reqwest::Response> {
        let url = self.url(path);
        let response =
            send_with_retry(&self.retry, || self.http_client.post(&url).json(body)).await?;
        Ok(response)
    }

    /// Send a JSON GET request with optional query params.
    pub async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: Option<&[(&str, &str)]>,
    ) -> Result<T> {
        let url = self.url(path);
        let response = if let Some(query) = query {
            let mut url = reqwest::Url::parse(&url)?;
            url.query_pairs_mut().extend_pairs(query.iter().copied());
            send_with_retry(&self.retry, || self.http_client.get(url.clone())).await?
        } else {
            send_with_retry(&self.retry, || self.http_client.get(&url)).await?
        };
        self.parse_json_response(response).await
    }

    /// Send a multipart POST request and deserialize the response body.
    pub async fn post_multipart<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        form: reqwest::multipart::Form,
    ) -> Result<T> {
        let url = self.url(path);
        let response = self.http_client.post(&url).multipart(form).send().await?;
        self.parse_json_response(response).await
    }

    /// Fetch raw bytes from a URL, using auth headers for MiniMax-hosted URLs.
    pub async fn get_bytes(&self, url: &str) -> Result<bytes::Bytes> {
        let client = if is_minimax_base_url(url) {
            &self.http_client
        } else {
            &self.raw_http_client
        };
        let response = send_with_retry(&self.retry, || client.get(url)).await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|e| format!("(failed to read body: {e})"));
            anyhow::bail!("Failed to fetch bytes: HTTP {status}: {text}");
        }
        Ok(response.bytes().await?)
    }

    /// Fetch raw bytes from a path with query params, returning an optional content type.
    pub async fn get_bytes_with_query(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<(bytes::Bytes, Option<String>)> {
        let url = self.url(path);
        let client = if is_minimax_base_url(&url) {
            &self.http_client
        } else {
            &self.raw_http_client
        };
        let mut url = reqwest::Url::parse(&url)?;
        url.query_pairs_mut().extend_pairs(query.iter().copied());
        let response = send_with_retry(&self.retry, || client.get(url.clone())).await?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        Ok((response.bytes().await?, content_type))
    }

    fn url(&self, path: &str) -> String {
        if path.starts_with("http") {
            path.to_string()
        } else {
            format!(
                "{}/{}",
                self.base_url.trim_end_matches('/'),
                path.trim_start_matches('/')
            )
        }
    }

    async fn parse_json_response<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T> {
        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .await
                .unwrap_or_else(|e| format!("(failed to read body: {e})"));
            anyhow::bail!("Failed to call MiniMax API: HTTP {status}: {text}");
        }
        Ok(response.json::<T>().await?)
    }
}

// === MiniMaxTextClient ===

impl MiniMaxTextClient {
    /// Create a MiniMax text client using the default model.
    pub fn new(config: &Config) -> Result<Self> {
        let model = config
            .default_text_model
            .clone()
            .unwrap_or_else(|| "MiniMax-M2.5".to_string());
        Self::with_model(config, model)
    }

    /// Create a MiniMax text client pinned to a specific model.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use crate::client::MiniMaxTextClient;
    /// # use crate::config::Config;
    /// # fn example(config: &Config) -> anyhow::Result<()> {
    /// let client = MiniMaxTextClient::with_model(config, "MiniMax-M2.5".to_string())?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_model(config: &Config, model: String) -> Result<Self> {
        let base_url = config.minimax_text_base_url();
        let api_key = config.minimax_text_api_key()?;
        let retry = config.retry_policy();
        let is_minimax = is_minimax_base_url(&base_url);

        logging::info(format!("MiniMax text API base URL: {base_url}"));
        logging::info(format!(
            "Retry policy: enabled={}, max_retries={}, initial_delay={}s, max_delay={}s",
            retry.enabled, retry.max_retries, retry.initial_delay, retry.max_delay
        ));

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("x-api-key", HeaderValue::from_str(&api_key)?);
        if is_minimax {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {api_key}"))?,
            );
        }
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

        let http_client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        Ok(Self {
            http_client,
            base_url,
            retry,
            default_model: model,
        })
    }

    /// Get the default model name
    #[allow(dead_code)] // For future model selection
    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    /// Create a non-streaming MiniMax text message request.
    pub async fn create_message(&self, request: MessageRequest) -> Result<MessageResponse> {
        let url = format!("{}/v1/messages", self.base_url);
        let mut request = request;
        request.stream = Some(false);

        let response =
            send_with_retry(&self.retry, || self.http_client.post(&url).json(&request)).await?;
        Ok(response.json::<MessageResponse>().await?)
    }

    /// Create a streaming MiniMax text message request.
    pub async fn create_message_stream(
        &self,
        request: MessageRequest,
    ) -> Result<impl futures_util::Stream<Item = Result<StreamEvent>>> {
        let url = format!("{}/v1/messages", self.base_url);
        let mut request = request;
        request.stream = Some(true);

        let response =
            send_with_retry(&self.retry, || self.http_client.post(&url).json(&request)).await?;

        Ok(parse_sse_stream(response.bytes_stream()))
    }
}

// === Retry + Streaming Helpers ===

async fn send_with_retry<F>(policy: &RetryPolicy, mut build: F) -> Result<reqwest::Response>
where
    F: FnMut() -> reqwest::RequestBuilder,
{
    let mut attempt: u32 = 0;

    loop {
        let result = build().send().await;

        match result {
            Ok(response) => {
                if response.status().is_success() {
                    return Ok(response);
                }

                let status = response.status();
                let retryable = status.as_u16() == 429 || status.is_server_error();

                if !policy.enabled || !retryable || attempt >= policy.max_retries {
                    let text = response
                        .text()
                        .await
                        .unwrap_or_else(|e| format!("(failed to read body: {e})"));
                    anyhow::bail!("Failed to send API request: HTTP {status}: {text}");
                }
                logging::warn(format!(
                    "Retryable HTTP {} (attempt {} of {})",
                    status.as_u16(),
                    attempt + 1,
                    policy.max_retries + 1
                ));
            }
            Err(err) => {
                if !policy.enabled || attempt >= policy.max_retries {
                    return Err(err.into());
                }
                logging::warn(format!(
                    "Request error: {} (attempt {} of {})",
                    err,
                    attempt + 1,
                    policy.max_retries + 1
                ));
            }
        }

        let delay = policy.delay_for_attempt(attempt);
        attempt += 1;
        logging::info(format!("Retrying after {:.2}s", delay.as_secs_f64()));
        tokio::time::sleep(delay).await;
    }
}

/// Parse an SSE stream into structured `MiniMax` stream events.
fn parse_sse_stream(
    stream: impl futures_util::Stream<Item = reqwest::Result<bytes::Bytes>> + Unpin,
) -> impl futures_util::Stream<Item = Result<StreamEvent>> {
    async_stream::try_stream! {
        let mut buffer = String::new();
        let mut stream = stream;

        while let Some(chunk_result) = stream.next().await {
            let chunk = match chunk_result {
                Ok(chunk) => chunk,
                Err(err) => {
                    logging::warn(format!("SSE stream chunk error: {err}"));
                    continue;
                }
            };
            let s = String::from_utf8_lossy(&chunk);
            buffer.push_str(&s);

            while let Some(pos) = buffer.find("\n\n") {
                let block = buffer[..pos].to_string();
                buffer.drain(..pos + 2);

                for line in block.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            return;
                        }
                        // Log raw SSE data for debugging
                        if data.contains("tool_use") || data.contains("input_json") {
                            logging::info(format!("SSE tool event: {}", data));
                        }
                        match serde_json::from_str::<StreamEvent>(data) {
                            Ok(event) => yield event,
                            Err(err) => {
                                logging::warn(format!("Failed to parse SSE event: {err}"));
                                logging::warn(format!("Raw SSE data: {data}"));
                            }
                        }
                    }
                }
            }
        }
    }
}

// === Trait Implementations ===

impl LlmClient for MiniMaxTextClient {
    fn provider_name(&self) -> &'static str {
        "minimax"
    }

    fn model(&self) -> &str {
        &self.default_model
    }

    async fn create_message(&self, request: MessageRequest) -> Result<MessageResponse> {
        // Delegate to existing method
        MiniMaxTextClient::create_message(self, request).await
    }

    async fn create_message_stream(&self, request: MessageRequest) -> Result<StreamEventBox> {
        let url = format!("{}/v1/messages", self.base_url);
        let mut request = request;
        request.stream = Some(true);

        let response =
            send_with_retry(&self.retry, || self.http_client.post(&url).json(&request)).await?;

        let stream = parse_sse_stream(response.bytes_stream());
        Ok(Pin::from(Box::new(stream)))
    }
}

// === MiniMaxCodingClient ===

/// Client for MiniMax Coding API requests with dedicated endpoint and model.
///
/// This client is optimized for coding tasks and uses a separate API endpoint
/// and model configuration from the standard MiniMax API.
#[derive(Clone)]
#[must_use]
pub struct MiniMaxCodingClient {
    http_client: reqwest::Client,
    base_url: String,
    retry: RetryPolicy,
    default_model: String,
}

impl MiniMaxCodingClient {
    /// Create a MiniMax Coding client from CLI configuration.
    pub fn new(config: &Config) -> Result<Self> {
        let api_key = config.coding_api_key()?;
        let base_url = config.coding_base_url();
        let model = config.coding_model();
        let retry = config.retry_policy();

        logging::info(format!("MiniMax Coding API base URL: {base_url}"));
        logging::info(format!("MiniMax Coding model: {model}"));
        logging::info(format!(
            "Retry policy: enabled={}, max_retries={}, initial_delay={}s, max_delay={}s",
            retry.enabled, retry.max_retries, retry.initial_delay, retry.max_delay
        ));

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {api_key}"))?,
        );

        let http_client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        Ok(Self {
            http_client,
            base_url,
            retry,
            default_model: model,
        })
    }

    /// Get the default model name
    #[must_use]
    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    /// Create a non-streaming coding message request.
    pub async fn create_message(&self, request: MessageRequest) -> Result<MessageResponse> {
        let url = format!("{}/v1/messages", self.base_url);
        let mut request = request;
        request.stream = Some(false);

        let response =
            send_with_retry(&self.retry, || self.http_client.post(&url).json(&request)).await?;
        Ok(response.json::<MessageResponse>().await?)
    }

    /// Create a streaming coding message request.
    #[allow(dead_code)]
    pub async fn create_message_stream(
        &self,
        request: MessageRequest,
    ) -> Result<impl futures_util::Stream<Item = Result<StreamEvent>>> {
        let url = format!("{}/v1/messages", self.base_url);
        let mut request = request;
        request.stream = Some(true);

        let response =
            send_with_retry(&self.retry, || self.http_client.post(&url).json(&request)).await?;

        Ok(parse_sse_stream(response.bytes_stream()))
    }

    /// Create a coding-specific request with optimized settings.
    ///
    /// This helper method creates a request configured for coding tasks,
    /// with appropriate default parameters for code generation.
    #[allow(dead_code)]
    pub fn create_coding_request(
        &self,
        messages: Vec<crate::models::Message>,
        system_prompt: Option<String>,
        max_tokens: Option<u32>,
    ) -> MessageRequest {
        MessageRequest {
            model: self.default_model.clone(),
            messages,
            max_tokens: max_tokens.unwrap_or(8192),
            system: system_prompt.map(crate::models::SystemPrompt::Text),
            tools: None,
            tool_choice: None,
            metadata: None,
            thinking: None,
            stream: Some(false),
            temperature: Some(0.2), // Lower temperature for more deterministic code
            top_p: Some(0.95),
        }
    }
}

impl LlmClient for MiniMaxCodingClient {
    fn provider_name(&self) -> &'static str {
        "minimax-coding"
    }

    fn model(&self) -> &str {
        &self.default_model
    }

    async fn create_message(&self, request: MessageRequest) -> Result<MessageResponse> {
        MiniMaxCodingClient::create_message(self, request).await
    }

    async fn create_message_stream(&self, request: MessageRequest) -> Result<StreamEventBox> {
        let url = format!("{}/v1/messages", self.base_url);
        let mut request = request;
        request.stream = Some(true);

        let response =
            send_with_retry(&self.retry, || self.http_client.post(&url).json(&request)).await?;

        let stream = parse_sse_stream(response.bytes_stream());
        Ok(Pin::from(Box::new(stream)))
    }
}
