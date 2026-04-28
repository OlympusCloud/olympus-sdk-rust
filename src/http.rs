use std::sync::Arc;

use reqwest::{Client, RequestBuilder, Response};
use serde_json::Value;

use crate::config::OlympusConfig;
use crate::error::{OlympusError, Result};

const SDK_VERSION: &str = "rust/0.3.0";

/// HTTP transport layer that wraps `reqwest::Client` with Olympus auth headers.
#[derive(Debug, Clone)]
pub struct OlympusHttpClient {
    client: Client,
    config: Arc<OlympusConfig>,
}

impl OlympusHttpClient {
    /// Creates a new HTTP client from the given configuration.
    pub fn new(config: Arc<OlympusConfig>) -> Result<Self> {
        let client = Client::builder()
            .timeout(config.timeout())
            .build()?;

        Ok(Self { client, config })
    }

    /// Sends a GET request to the given path.
    pub async fn get(&self, path: &str) -> Result<Value> {
        let url = self.url(path);
        let req = self.apply_headers(self.client.get(&url));
        self.execute(req).await
    }

    /// Sends a GET request with query parameters.
    pub async fn get_with_query(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let url = self.url(path);
        let req = self.apply_headers(self.client.get(&url)).query(query);
        self.execute(req).await
    }

    /// Sends a POST request with a JSON body.
    pub async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        let url = self.url(path);
        let req = self.apply_headers(self.client.post(&url)).json(body);
        self.execute(req).await
    }

    /// Sends a PUT request with a JSON body.
    pub async fn put(&self, path: &str, body: &Value) -> Result<Value> {
        let url = self.url(path);
        let req = self.apply_headers(self.client.put(&url)).json(body);
        self.execute(req).await
    }

    /// Sends a PATCH request with a JSON body.
    pub async fn patch(&self, path: &str, body: &Value) -> Result<Value> {
        let url = self.url(path);
        let req = self.apply_headers(self.client.patch(&url)).json(body);
        self.execute(req).await
    }

    /// Sends a DELETE request to the given path.
    pub async fn delete(&self, path: &str) -> Result<Value> {
        let url = self.url(path);
        let req = self.apply_headers(self.client.delete(&url));
        self.execute(req).await
    }

    /// Builds the full URL from the base URL and the given path.
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.base_url.trim_end_matches('/'), path)
    }

    /// Applies standard Olympus authentication and SDK headers.
    fn apply_headers(&self, req: RequestBuilder) -> RequestBuilder {
        req.header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("X-App-Id", &self.config.app_id)
            .header("X-SDK-Version", SDK_VERSION)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
    }

    /// Executes a request and handles the response.
    async fn execute(&self, req: RequestBuilder) -> Result<Value> {
        let resp: Response = req.send().await?;
        let status = resp.status();

        if status.is_success() {
            // 204 No Content or empty body
            let bytes = resp.bytes().await?;
            if bytes.is_empty() {
                return Ok(Value::Object(serde_json::Map::new()));
            }
            let value: Value = serde_json::from_slice(&bytes)?;
            Ok(value)
        } else {
            let status_code = status.as_u16();
            let body = resp.text().await.unwrap_or_default();
            Err(parse_api_error(status_code, &body))
        }
    }

    /// Sends a GET request and returns the raw `reqwest::Response` so the
    /// caller can read response headers (e.g. `Cache-Control` for the
    /// i18n manifest in `crate::i18n::I18nService`). Errors still surface
    /// as `OlympusError::Api` via `parse_api_error`.
    pub(crate) async fn get_response(&self, path: &str) -> Result<Response> {
        let url = self.url(path);
        let req = self.apply_headers(self.client.get(&url));
        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status_code = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(parse_api_error(status_code, &body));
        }
        Ok(resp)
    }
}

/// Parse the canonical `{error: {code, message, request_id}}` envelope.
///
/// Falls back gracefully:
/// 1. Empty body or invalid JSON → `code="UNKNOWN"`, `message=<status text>`.
/// 2. JSON without an `error` object → `code="UNKNOWN"`, `message=<raw body>`.
fn parse_api_error(status_code: u16, body: &str) -> OlympusError {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return OlympusError::Api {
            status: status_code,
            code: "UNKNOWN".into(),
            message: format!("HTTP {status_code}"),
            request_id: None,
        };
    }

    if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
        if let Some(err) = parsed.get("error").and_then(|v| v.as_object()) {
            let code = err
                .get("code")
                .and_then(|v| v.as_str())
                .unwrap_or("UNKNOWN")
                .to_string();
            let message = err
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error")
                .to_string();
            let request_id = err
                .get("request_id")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            return OlympusError::Api {
                status: status_code,
                code,
                message,
                request_id,
            };
        }
        // Flat shape — unwrap top-level `code` / `message` if present.
        if let Some(obj) = parsed.as_object() {
            let code = obj
                .get("code")
                .and_then(|v| v.as_str())
                .unwrap_or("UNKNOWN")
                .to_string();
            let message = obj
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or(trimmed)
                .to_string();
            return OlympusError::Api {
                status: status_code,
                code,
                message,
                request_id: None,
            };
        }
    }

    OlympusError::Api {
        status: status_code,
        code: "UNKNOWN".into(),
        message: trimmed.to_string(),
        request_id: None,
    }
}
