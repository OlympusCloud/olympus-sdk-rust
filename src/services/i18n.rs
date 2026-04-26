//! `I18nService` — error code i18n manifest consumer (issue #3638).
//!
//! Wraps `GET /v1/i18n/errors`, the centralised error code → localized
//! message manifest served by the Rust platform service. Consumers use it
//! to render user-friendly translations of platform errors that arrive in
//! the `{ error: { code, message } }` envelope without bundling per-app
//! translations.
//!
//! ```rust,no_run
//! # use olympus_sdk::OlympusClient;
//! # async fn run() -> std::result::Result<(), Box<dyn std::error::Error>> {
//! let client = OlympusClient::new("com.my-app", "oc_live_...");
//!
//! // Fetch the manifest (cached for 1h).
//! let manifest = client.i18n().errors("en").await?;
//!
//! // Look up a single code in the user's locale, with `en` fallback.
//! let msg = client.i18n().localize("ORDER_NOT_FOUND", "es").await?;
//! # Ok(()) }
//! ```
//!
//! Caching: the manifest is identical for every caller, so we cache the
//! parsed result in-memory for 1h (matches the backend's
//! `Cache-Control: public, max-age=3600`). Concurrent cold callers share
//! a single in-flight request via a `tokio::sync::Mutex` — we never issue
//! two parallel requests for the same payload.
//!
//! The `locale` argument on [`I18nService::errors`] is intentionally
//! accepted-but-ignored at the network layer: the backend always ships
//! every locale in one payload. We keep it on the public surface for API
//! symmetry with [`I18nService::localize`].

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::{OlympusError, Result};
use crate::http::OlympusHttpClient;

/// Cache TTL — must match the backend `Cache-Control: max-age=3600`.
pub const I18N_CACHE_TTL: Duration = Duration::from_secs(60 * 60);

/// One row in the error manifest.
///
/// `code` is the canonical UPPER_SNAKE error code emitted in the
/// `{ error: { code, message } }` envelope (e.g. `ORDER_NOT_FOUND`,
/// `VALIDATION_ERROR`). `messages` maps a locale string (BCP-47-ish, e.g.
/// `en`, `es`, `fr`) to the human-readable translation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorManifestEntry {
    /// Canonical UPPER_SNAKE error code.
    pub code: String,
    /// Locale → message map. Always includes every locale listed in the
    /// parent [`ErrorManifest::locales`]; new locales added later may
    /// be missing for older codes (call sites should fall back to `en`).
    #[serde(default)]
    pub messages: HashMap<String, String>,
}

impl ErrorManifestEntry {
    /// Pick the message for `locale`, falling back to `en` when missing.
    /// Returns `None` if neither is present (caller should fall back to
    /// the raw code).
    pub fn message_for(&self, locale: &str) -> Option<&str> {
        self.messages
            .get(locale)
            .filter(|s| !s.is_empty())
            .map(String::as_str)
            .or_else(|| self.messages.get("en").map(String::as_str))
    }
}

/// Top-level shape served by `GET /v1/i18n/errors`.
///
/// The manifest is identical for every caller — the response is cached at
/// the edge for 1 hour and the SDK caches the parsed result in-memory for
/// the same window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorManifest {
    /// Schema version. Bumped on breaking shape changes (additive code
    /// changes stay at `1.0`).
    #[serde(default = "default_version")]
    pub version: String,
    /// Locales the manifest carries human-authored translations for.
    /// Apps should fall back to `en` when their preferred locale isn't
    /// listed.
    #[serde(default)]
    pub locales: Vec<String>,
    /// One entry per canonical error code.
    #[serde(default)]
    pub errors: Vec<ErrorManifestEntry>,
}

fn default_version() -> String {
    "1.0".to_string()
}

impl ErrorManifest {
    /// Look up a manifest entry by canonical code.
    pub fn entry_for(&self, code: &str) -> Option<&ErrorManifestEntry> {
        self.errors.iter().find(|e| e.code == code)
    }
}

/// Internal cache state. Held in an `Arc<Mutex<…>>` so multiple
/// [`I18nService`] handles created from a single [`crate::OlympusClient`]
/// share the same cached manifest.
#[derive(Default)]
pub(crate) struct I18nState {
    cached: Option<ErrorManifest>,
    expires_at: Option<Instant>,
}

/// `I18nService` fetches + caches + localizes against the platform error
/// manifest at `GET /v1/i18n/errors`.
///
/// Constructed via [`crate::OlympusClient::i18n`]. Cheap to clone — the
/// shared cache state lives behind an `Arc<Mutex<…>>`.
#[derive(Clone)]
pub struct I18nService {
    http: Arc<OlympusHttpClient>,
    state: Arc<Mutex<I18nState>>,
}

impl I18nService {
    pub(crate) fn new(http: Arc<OlympusHttpClient>, state: Arc<Mutex<I18nState>>) -> Self {
        Self { http, state }
    }

    /// Fetch the full error manifest.
    ///
    /// The response is cached for [`I18N_CACHE_TTL`] after the first
    /// call. Concurrent callers during a cold fetch share a single HTTP
    /// request — the cache mutex is held across the network call so the
    /// second caller observes the populated cache when it acquires the
    /// lock.
    ///
    /// The `locale` argument is decorative — the backend always ships
    /// every locale in one payload. Use [`Self::localize`] when you only
    /// need a single translated string.
    pub async fn errors(&self, _locale: &str) -> Result<ErrorManifest> {
        let mut guard = self.state.lock().await;
        if let (Some(cached), Some(expires)) = (&guard.cached, guard.expires_at) {
            if Instant::now() < expires {
                return Ok(cached.clone());
            }
        }
        // Cache miss or expired — fetch from network. Holding the
        // mutex across the await means concurrent cold callers serialize
        // and the second caller observes the populated cache.
        let value = self.http.get("/v1/i18n/errors").await?;
        let manifest: ErrorManifest = serde_json::from_value(value).map_err(OlympusError::from)?;
        guard.cached = Some(manifest.clone());
        guard.expires_at = Some(Instant::now() + I18N_CACHE_TTL);
        Ok(manifest)
    }

    /// Resolve `code` to a human-readable string in `locale`, falling
    /// back to `en` and finally to the raw code itself when neither is
    /// present. Triggers a manifest fetch on first call (or after the
    /// 1h cache expires). Empty/whitespace `code` short-circuits to the
    /// empty string.
    pub async fn localize(&self, code: &str, locale: &str) -> Result<String> {
        if code.trim().is_empty() {
            return Ok(String::new());
        }
        let manifest = self.errors(locale).await?;
        let Some(entry) = manifest.entry_for(code) else {
            return Ok(code.to_string());
        };
        Ok(entry
            .message_for(locale)
            .map(str::to_string)
            .unwrap_or_else(|| code.to_string()))
    }

    /// Localize an [`OlympusError::Api`] envelope by the structured code
    /// in its message body.
    ///
    /// Because [`OlympusError::Api`] only carries the textual `message`
    /// field, callers that need machine-code-driven localization should
    /// pass the canonical code explicitly via [`Self::localize_code`].
    /// This helper is a thin convenience that takes the (code, message)
    /// pair directly — typically extracted from the server envelope by
    /// the caller.
    pub async fn localize_code(
        &self,
        code: &str,
        server_message: &str,
        locale: &str,
    ) -> Result<String> {
        if code.is_empty() {
            return Ok(server_message.to_string());
        }
        let manifest = self.errors(locale).await?;
        let Some(entry) = manifest.entry_for(code) else {
            return Ok(if !server_message.is_empty() {
                server_message.to_string()
            } else {
                code.to_string()
            });
        };
        if let Some(msg) = entry.message_for(locale) {
            return Ok(msg.to_string());
        }
        Ok(if !server_message.is_empty() {
            server_message.to_string()
        } else {
            code.to_string()
        })
    }

    /// Drop any cached manifest. Useful for tests and for tenants that
    /// have flipped a manifest version mid-session.
    pub async fn clear_cache(&self) {
        let mut guard = self.state.lock().await;
        guard.cached = None;
        guard.expires_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_message_for_falls_back_to_en() {
        let mut messages = HashMap::new();
        messages.insert("en".to_string(), "english".to_string());
        messages.insert("es".to_string(), "spanish".to_string());
        let entry = ErrorManifestEntry {
            code: "X".into(),
            messages,
        };
        assert_eq!(entry.message_for("es"), Some("spanish"));
        assert_eq!(entry.message_for("de"), Some("english"));
    }

    #[test]
    fn entry_message_for_returns_none_when_empty() {
        let entry = ErrorManifestEntry {
            code: "Y".into(),
            messages: HashMap::new(),
        };
        assert_eq!(entry.message_for("en"), None);
    }

    #[test]
    fn manifest_entry_for_lookup() {
        let manifest = ErrorManifest {
            version: "1.0".into(),
            locales: vec!["en".into()],
            errors: vec![ErrorManifestEntry {
                code: "ABC".into(),
                messages: HashMap::new(),
            }],
        };
        assert!(manifest.entry_for("ABC").is_some());
        assert!(manifest.entry_for("XYZ").is_none());
    }
}
