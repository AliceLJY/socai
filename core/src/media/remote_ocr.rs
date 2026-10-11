use std::time::Duration;

use anyhow::{Context, Result};
use base64::Engine;
use futures::StreamExt;
use serde_json::{json, Value};

use crate::media::common::detect_media_type;

const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_TEXT_CHARS: usize = 100_000;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

/// Authenticated, host-owned OCR service used by managed workers. The endpoint
/// is deliberately limited to loopback so Core cannot turn it into a generic
/// credentialed HTTP client. Desktop and CLI runs leave this unset and retain
/// the bundled local OCR engine.
#[derive(Clone)]
pub struct RemoteOcrConfig {
    endpoint: String,
    bearer_token: String,
    request_context: Value,
    client: reqwest::Client,
}

impl std::fmt::Debug for RemoteOcrConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteOcrConfig")
            .field("endpoint", &self.endpoint)
            .field("bearer_token", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl RemoteOcrConfig {
    pub fn new(
        endpoint: impl Into<String>,
        bearer_token: impl Into<String>,
        request_context: Value,
    ) -> Result<Self> {
        let endpoint = endpoint.into();
        let bearer_token = bearer_token.into();
        let parsed = reqwest::Url::parse(&endpoint).context("remote OCR endpoint is invalid")?;
        if parsed.scheme() != "http"
            || !matches!(parsed.host_str(), Some("127.0.0.1" | "::1" | "localhost"))
            || parsed.port().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.fragment().is_some()
        {
            anyhow::bail!("remote OCR endpoint must use HTTP loopback");
        }
        if !(32..=512).contains(&bearer_token.len()) {
            anyhow::bail!("remote OCR bearer token must contain between 32 and 512 bytes");
        }
        if !request_context.is_object() {
            anyhow::bail!("remote OCR request context must be an object");
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("remote OCR client could not be created")?;
        Ok(Self {
            endpoint,
            bearer_token,
            request_context,
            client,
        })
    }

    pub(crate) async fn recognize(&self, payload: &[u8]) -> Result<String> {
        if payload.len() > MAX_IMAGE_BYTES {
            anyhow::bail!(
                "image exceeds the managed OCR limit: {} bytes",
                payload.len()
            );
        }
        let mut request = self
            .request_context
            .as_object()
            .cloned()
            .context("remote OCR request context is unavailable")?;
        request.insert("tool".into(), Value::String("internal_ocr_image".into()));
        request.insert(
            "input".into(),
            json!({
                "content_type": detect_media_type(payload),
                "image_base64": base64::engine::general_purpose::STANDARD.encode(payload),
            }),
        );
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.bearer_token)
            .json(&Value::Object(request))
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .context("managed OCR request failed")?;
        let status = response.status();
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("managed OCR response could not be read")?;
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                anyhow::bail!("managed OCR response exceeded 1 MiB");
            }
            bytes.extend_from_slice(&chunk);
        }
        let body: Value =
            serde_json::from_slice(&bytes).context("managed OCR response was not valid JSON")?;
        if !status.is_success() || body.get("ok").and_then(Value::as_bool) != Some(true) {
            let error = body
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("managed OCR rejected the image");
            anyhow::bail!("{}", error.chars().take(1_000).collect::<String>());
        }
        let text = body
            .get("result")
            .and_then(|result| result.get("text"))
            .and_then(Value::as_str)
            .context("managed OCR response contained no text")?;
        Ok(text.chars().take(MAX_TEXT_CHARS).collect())
    }

    pub fn diagnostics(&self) -> Value {
        json!({
            "model": "managed OCR",
            "runtime": "host endpoint",
            "execution_provider": "remote",
        })
    }
}
