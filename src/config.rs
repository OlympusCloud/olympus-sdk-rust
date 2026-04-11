use std::time::Duration;

/// Configuration for the Olympus Cloud SDK client.
#[derive(Debug, Clone)]
pub struct OlympusConfig {
    /// Application identifier (e.g., "com.my-restaurant").
    pub app_id: String,

    /// API key for authentication (e.g., "oc_live_...").
    pub api_key: String,

    /// Base URL for the API. Defaults to `https://api.olympuscloud.ai`.
    pub base_url: String,

    /// Request timeout in milliseconds. Defaults to 30000 (30 seconds).
    pub timeout_ms: u64,
}

impl OlympusConfig {
    /// Creates a new configuration with the given app_id and api_key,
    /// using production defaults for base_url and timeout.
    pub fn new(app_id: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            app_id: app_id.into(),
            api_key: api_key.into(),
            base_url: "https://api.olympuscloud.ai".to_string(),
            timeout_ms: 30_000,
        }
    }

    /// Sets a custom base URL. Returns self for builder-style chaining.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Sets the request timeout in milliseconds. Returns self for builder-style chaining.
    pub fn with_timeout_ms(mut self, ms: u64) -> Self {
        self.timeout_ms = ms;
        self
    }

    /// Returns the configured timeout as a `Duration`.
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}
