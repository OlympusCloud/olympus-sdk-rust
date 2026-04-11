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
            Err(OlympusError::Api {
                status: status_code,
                message: body,
            })
        }
    }
}
