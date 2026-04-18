use std::sync::{Arc, RwLock};

use reqwest::{header::HeaderMap, Client, RequestBuilder, Response};
use serde_json::Value;

use crate::config::OlympusConfig;
use crate::error::{OlympusError, Result};

const SDK_VERSION: &str = "rust/0.3.0";

/// Callback fired when the server returns `X-Olympus-Catalog-Stale: true`
/// (§4.7 rolling window). Consumers should schedule a background token refresh
/// at a randomized 0–15 min offset. Called at most once per stale-token window.
pub type StaleCatalogHandler = Arc<dyn Fn() + Send + Sync>;

/// Mutable token + callback state. Held in an `Arc<RwLock>` so
/// `OlympusHttpClient` remains `Clone` and can be shared across async tasks.
#[derive(Default)]
struct HttpState {
    access_token: Option<String>,
    app_token: Option<String>,
    on_stale_catalog: Option<StaleCatalogHandler>,
    // Debounce: last access_token value that already fired the stale handler.
    stale_notified_for_token: Option<String>,
}

/// HTTP transport layer that wraps `reqwest::Client` with Olympus auth headers.
#[derive(Clone)]
pub struct OlympusHttpClient {
    client: Client,
    config: Arc<OlympusConfig>,
    state: Arc<RwLock<HttpState>>,
}

impl std::fmt::Debug for OlympusHttpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OlympusHttpClient")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl OlympusHttpClient {
    /// Creates a new HTTP client from the given configuration.
    pub fn new(config: Arc<OlympusConfig>) -> Result<Self> {
        let client = Client::builder()
            .timeout(config.timeout())
            .build()?;

        Ok(Self {
            client,
            config,
            state: Arc::new(RwLock::new(HttpState::default())),
        })
    }

    /// Set the user access token. Takes precedence over the API key.
    pub fn set_access_token(&self, token: impl Into<String>) {
        let mut s = self.state.write().expect("poisoned");
        s.access_token = Some(token.into());
        s.stale_notified_for_token = None;
    }

    /// Clear the user access token.
    pub fn clear_access_token(&self) {
        let mut s = self.state.write().expect("poisoned");
        s.access_token = None;
        s.stale_notified_for_token = None;
    }

    /// Set the App JWT (X-App-Token, §4.5 dual-JWT flow).
    pub fn set_app_token(&self, token: impl Into<String>) {
        let mut s = self.state.write().expect("poisoned");
        s.app_token = Some(token.into());
        s.stale_notified_for_token = None;
    }

    /// Clear the App JWT.
    pub fn clear_app_token(&self) {
        let mut s = self.state.write().expect("poisoned");
        s.app_token = None;
        s.stale_notified_for_token = None;
    }

    /// Internal — returns the current access token for JWT-bitset decoding.
    pub fn access_token_for_internal(&self) -> Option<String> {
        self.state.read().expect("poisoned").access_token.clone()
    }

    /// Register a stale-catalog handler (§4.7). Fires at most once per
    /// stale-token window; reset happens on any token mutation.
    pub fn on_catalog_stale(&self, handler: Option<StaleCatalogHandler>) {
        let mut s = self.state.write().expect("poisoned");
        s.on_stale_catalog = handler;
        s.stale_notified_for_token = None;
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

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.base_url.trim_end_matches('/'), path)
    }

    /// Applies standard Olympus authentication and SDK headers.
    fn apply_headers(&self, req: RequestBuilder) -> RequestBuilder {
        let (access, app) = {
            let s = self.state.read().expect("poisoned");
            (s.access_token.clone(), s.app_token.clone())
        };
        let auth_token = access.unwrap_or_else(|| self.config.api_key.clone());
        let mut builder = req
            .header("Authorization", format!("Bearer {}", auth_token))
            .header("X-App-Id", &self.config.app_id)
            .header("X-SDK-Version", SDK_VERSION)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json");
        if let Some(app_token) = app {
            builder = builder.header("X-App-Token", app_token);
        }
        builder
    }

    /// Fire the stale-catalog handler at most once per stale-token window.
    fn maybe_fire_stale_catalog(&self, headers: &HeaderMap) {
        let is_stale = headers
            .get("X-Olympus-Catalog-Stale")
            .and_then(|v| v.to_str().ok())
            .map(|v| v == "true")
            .unwrap_or(false);
        if !is_stale {
            return;
        }
        let mut s = self.state.write().expect("poisoned");
        let current = s.access_token.clone();
        if s.stale_notified_for_token == current {
            return;
        }
        s.stale_notified_for_token = current;
        if let Some(handler) = s.on_stale_catalog.clone() {
            // Drop write lock before invoking handler to avoid deadlocks if
            // the handler itself interacts with the client.
            drop(s);
            // Handler errors (panics) are caught below.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler()));
        }
    }

    /// Executes a request and handles the response.
    async fn execute(&self, req: RequestBuilder) -> Result<Value> {
        let resp: Response = req.send().await?;
        let status = resp.status();
        let status_code = status.as_u16();
        let headers = resp.headers().clone();

        if status.is_success() {
            self.maybe_fire_stale_catalog(&headers);
            let bytes = resp.bytes().await?;
            if bytes.is_empty() {
                return Ok(Value::Object(serde_json::Map::new()));
            }
            let value: Value = serde_json::from_slice(&bytes)?;
            Ok(value)
        } else {
            let body_text = resp.text().await.unwrap_or_default();
            // Parse body for error routing.
            let body_json: Option<Value> = serde_json::from_str(&body_text).ok();
            let (code, message, request_id) = extract_error_fields(&body_json, &body_text);

            if let Some(typed) = route_app_scoped_error(
                &code,
                &message,
                request_id.as_deref(),
                &body_json,
                &headers,
                status_code,
            ) {
                return Err(typed);
            }

            Err(OlympusError::Api {
                status: status_code,
                message: if !message.is_empty() { message } else { body_text },
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Typed error routing for app-scoped permissions (§6 + §17.7)
// ---------------------------------------------------------------------------

fn extract_error_fields(body: &Option<Value>, raw: &str) -> (String, String, Option<String>) {
    let mut code = String::new();
    let mut message = String::new();
    let mut request_id: Option<String> = None;

    if let Some(b) = body {
        if let Some(err) = b.get("error") {
            if let Some(c) = err.get("code").and_then(Value::as_str) {
                code = c.to_string();
            }
            if let Some(m) = err.get("message").and_then(Value::as_str) {
                message = m.to_string();
            }
            if let Some(r) = err.get("request_id").and_then(Value::as_str) {
                request_id = Some(r.to_string());
            }
        }
        if code.is_empty() {
            if let Some(c) = b.get("code").and_then(Value::as_str) {
                code = c.to_string();
            }
        }
        if message.is_empty() {
            if let Some(m) = b.get("message").and_then(Value::as_str) {
                message = m.to_string();
            }
        }
    }
    if message.is_empty() && !raw.is_empty() {
        message = raw.to_string();
    }
    (code, message, request_id)
}

fn extract_string(body: &Option<Value>, key: &str) -> Option<String> {
    let body = body.as_ref()?;
    if let Some(v) = body.get(key).and_then(Value::as_str) {
        return Some(v.to_string());
    }
    if let Some(err) = body.get("error") {
        if let Some(v) = err.get(key).and_then(Value::as_str) {
            return Some(v.to_string());
        }
    }
    None
}

fn extract_bool(body: &Option<Value>, key: &str) -> Option<bool> {
    let body = body.as_ref()?;
    if let Some(v) = body.get(key).and_then(Value::as_bool) {
        return Some(v);
    }
    if let Some(err) = body.get("error") {
        if let Some(v) = err.get(key).and_then(Value::as_bool) {
            return Some(v);
        }
    }
    None
}

fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

fn route_app_scoped_error(
    code: &str,
    message: &str,
    request_id: Option<&str>,
    body: &Option<Value>,
    headers: &HeaderMap,
    status: u16,
) -> Option<OlympusError> {
    let normalized = code.to_ascii_lowercase();
    let msg = message.to_string();
    let rid = request_id.map(|s| s.to_string());
    match normalized.as_str() {
        "scope_not_granted" | "consent_required" => Some(OlympusError::ConsentRequired {
            scope: extract_string(body, "scope").unwrap_or_else(|| "unknown".into()),
            consent_url: extract_string(body, "consent_url")
                .or_else(|| header_str(headers, "X-Olympus-Consent-URL")),
            message: msg,
            status,
            request_id: rid,
        }),
        "scope_denied" => Some(OlympusError::ScopeDenied {
            scope: extract_string(body, "scope").unwrap_or_else(|| "unknown".into()),
            message: msg,
            status,
            request_id: rid,
        }),
        "billing_grace_exceeded" => Some(OlympusError::BillingGraceExceeded {
            message: msg,
            grace_until: extract_string(body, "grace_until")
                .or_else(|| header_str(headers, "X-Olympus-Grace-Until")),
            upgrade_url: extract_string(body, "upgrade_url")
                .or_else(|| header_str(headers, "X-Olympus-Upgrade-URL")),
            status,
            request_id: rid,
        }),
        "webauthn_required" | "device_changed" => Some(OlympusError::DeviceChanged {
            challenge: extract_string(body, "challenge").unwrap_or_default(),
            requires_reconsent: extract_bool(body, "requires_reconsent").unwrap_or(false),
            message: msg,
            status,
            request_id: rid,
        }),
        "exception_expired" => Some(OlympusError::ExceptionExpired {
            exception_id: extract_string(body, "exception_id").unwrap_or_default(),
            message: msg,
            status,
            request_id: rid,
        }),
        _ => None,
    }
}
