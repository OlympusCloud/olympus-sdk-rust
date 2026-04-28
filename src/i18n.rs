//! Localized error manifest consumer (#3638 / parent #3626).
//!
//! Fetches the canonical error-code → localized-message manifest from
//! `GET /v1/i18n/errors` and exposes it via the SDK so callers can render a
//! user-facing message in the locale of their choice. Mirrors the Python
//! (#3636), TypeScript (#3635), and Go (#3637) consumers.
//!
//! Public surface:
//!
//! * [`ErrorManifest`] / [`ErrorManifestEntry`] — strongly-typed structs
//!   matching the wire shape produced by
//!   `backend/rust/shared/src/localization_service.rs::error_manifest()`.
//! * [`I18nService`] — exposed on [`crate::OlympusClient::i18n`]. Provides
//!   [`I18nService::fetch_error_manifest`] and
//!   [`I18nService::localize_error_code`].
//! * [`crate::OlympusError::localized_message`] — synchronous lookup against
//!   the module-level cache; falls back to the server-provided English
//!   message when the cache is empty (call `fetch_error_manifest()` once at
//!   startup to warm it).
//!
//! Concurrency: a `tokio::sync::Mutex` guards the first-fetch path so N
//! concurrent `fetch_error_manifest()` calls collapse to ONE network
//! round-trip.
//!
//! Locale fallback chain (matches the Rust platform contract): a request
//! for `es-MX` falls through to `es` then `en`. `en` is the documented
//! ground truth and the manifest invariants test on the Rust platform side
//! guarantees it is always present for every code.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::Result;
use crate::http::OlympusHttpClient;

const DEFAULT_TTL: Duration = Duration::from_secs(3600);

/// One row in the error manifest — the canonical UPPER_SNAKE error code
/// paired with per-locale messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorManifestEntry {
    pub code: String,
    pub messages: HashMap<String, String>,
}

/// Top-level shape served by `GET /v1/i18n/errors`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorManifest {
    pub version: String,
    pub locales: Vec<String>,
    pub errors: Vec<ErrorManifestEntry>,
}

impl ErrorManifest {
    /// Build a code-indexed map for fast lookup.
    pub fn by_code(&self) -> HashMap<&str, &ErrorManifestEntry> {
        self.errors.iter().map(|e| (e.code.as_str(), e)).collect()
    }
}

// ---------------------------------------------------------------------------
// Module-level cache
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct CachedManifest {
    manifest: Arc<ErrorManifest>,
    by_code: Arc<HashMap<String, ErrorManifestEntry>>,
    fetched_at: Instant,
    ttl: Duration,
}

#[derive(Debug, Default)]
struct ManifestState {
    cached: std::sync::Mutex<Option<CachedManifest>>,
    fetch_lock: Mutex<()>,
}

static MANIFEST_STATE: OnceLock<ManifestState> = OnceLock::new();

fn state() -> &'static ManifestState {
    MANIFEST_STATE.get_or_init(ManifestState::default)
}

fn cache_get_if_fresh() -> Option<Arc<ErrorManifest>> {
    let guard = state().cached.lock().ok()?;
    let entry = guard.as_ref()?;
    if entry.fetched_at.elapsed() < entry.ttl {
        Some(entry.manifest.clone())
    } else {
        None
    }
}

fn cache_get_unchecked() -> Option<Arc<ErrorManifest>> {
    let guard = state().cached.lock().ok()?;
    guard.as_ref().map(|e| e.manifest.clone())
}

fn cache_get_by_code() -> Option<Arc<HashMap<String, ErrorManifestEntry>>> {
    let guard = state().cached.lock().ok()?;
    guard.as_ref().map(|e| e.by_code.clone())
}

fn cache_store(manifest: ErrorManifest, ttl: Duration) {
    let by_code: HashMap<String, ErrorManifestEntry> = manifest
        .errors
        .iter()
        .map(|e| (e.code.clone(), e.clone()))
        .collect();
    let mut guard = state().cached.lock().expect("cache lock poisoned");
    *guard = Some(CachedManifest {
        manifest: Arc::new(manifest),
        by_code: Arc::new(by_code),
        fetched_at: Instant::now(),
        ttl,
    });
}

#[doc(hidden)]
pub fn _reset_cache_for_tests() {
    let mut guard = state().cached.lock().expect("cache lock poisoned");
    *guard = None;
}

#[doc(hidden)]
pub fn _seed_cache_for_tests(manifest: ErrorManifest, ttl: Duration) {
    cache_store(manifest, ttl);
}

// ---------------------------------------------------------------------------
// Locale fallback chain
// ---------------------------------------------------------------------------

/// Build the locale fallback chain.
///
/// * `"es-MX"`     → `["es-MX", "es", "en"]`
/// * `"en"`        → `["en"]`
/// * `"zh-Hant-TW"`→ `["zh-Hant-TW", "zh", "en"]` (strip on first dash)
/// * `""`          → `["en"]`
pub fn candidate_locales(locale: &str) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    if !locale.is_empty() {
        candidates.push(locale.to_string());
        seen.insert(locale.to_string());
    }
    if let Some(dash) = locale.find('-') {
        if dash > 0 {
            let base = &locale[..dash];
            if !seen.contains(base) {
                candidates.push(base.to_string());
                seen.insert(base.to_string());
            }
        }
    }
    if !seen.contains("en") {
        candidates.push("en".to_string());
    }
    candidates
}

/// Resolve `code` to a localized message against `manifest`.
pub fn resolve_manifest(
    manifest: &ErrorManifest,
    code: &str,
    locale: &str,
    fallback: Option<&str>,
) -> String {
    for entry in &manifest.errors {
        if entry.code != code {
            continue;
        }
        for candidate in candidate_locales(locale) {
            if let Some(msg) = entry.messages.get(&candidate) {
                if !msg.is_empty() {
                    return msg.clone();
                }
            }
        }
        return fallback_or_code(code, fallback);
    }
    fallback_or_code(code, fallback)
}

fn fallback_or_code(code: &str, fallback: Option<&str>) -> String {
    match fallback {
        Some(f) if !f.is_empty() => f.to_string(),
        _ => code.to_string(),
    }
}

/// Synchronous lookup against the module-level cache. Used by
/// [`crate::OlympusError::localized_message`].
pub fn resolve_from_cache(code: &str, locale: &str, fallback: Option<&str>) -> String {
    let by_code = match cache_get_by_code() {
        Some(b) => b,
        None => return fallback_or_code(code, fallback),
    };
    let entry = match by_code.get(code) {
        Some(e) => e,
        None => return fallback_or_code(code, fallback),
    };
    for candidate in candidate_locales(locale) {
        if let Some(msg) = entry.messages.get(&candidate) {
            if !msg.is_empty() {
                return msg.clone();
            }
        }
    }
    fallback_or_code(code, fallback)
}

// ---------------------------------------------------------------------------
// Cache-Control: max-age parsing
// ---------------------------------------------------------------------------

fn parse_max_age(header: Option<&str>) -> Duration {
    let header = match header {
        Some(h) => h,
        None => return DEFAULT_TTL,
    };
    for part in header.split(',') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("max-age=") {
            if let Ok(secs) = rest.parse::<u64>() {
                return Duration::from_secs(secs);
            }
        }
    }
    DEFAULT_TTL
}

// ---------------------------------------------------------------------------
// I18nService exposed on OlympusClient.i18n
// ---------------------------------------------------------------------------

/// Localized error manifest consumer.
#[derive(Debug, Clone)]
pub struct I18nService {
    http: Arc<OlympusHttpClient>,
}

impl I18nService {
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Fetch the manifest, refreshing when stale. Concurrent first-fetch
    /// callers from N tasks all share the same in-flight request via the
    /// async mutex — exactly ONE network round-trip.
    pub async fn fetch_error_manifest(&self) -> Result<Arc<ErrorManifest>> {
        self.fetch_internal(false).await
    }

    /// Force a refetch, bypassing the cache. Use after rotating the
    /// manifest version on the server.
    pub async fn fetch_error_manifest_forced(&self) -> Result<Arc<ErrorManifest>> {
        self.fetch_internal(true).await
    }

    async fn fetch_internal(&self, force: bool) -> Result<Arc<ErrorManifest>> {
        if !force {
            if let Some(m) = cache_get_if_fresh() {
                return Ok(m);
            }
        }

        let _guard = state().fetch_lock.lock().await;
        // Re-check after the lock — another task may have populated the cache.
        if !force {
            if let Some(m) = cache_get_if_fresh() {
                return Ok(m);
            }
        }

        let resp = self.http.get_response("/v1/i18n/errors").await?;
        let cache_control = resp
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let ttl = parse_max_age(cache_control.as_deref());
        let manifest: ErrorManifest = resp.json().await?;
        cache_store(manifest, ttl);
        // SAFETY: cache was just populated.
        Ok(cache_get_unchecked().expect("cache populated above"))
    }

    /// Resolve a `code` against the manifest, fetching it if not yet
    /// cached. Falls back to `fallback` (or the bare `code`) when the
    /// manifest is unreachable.
    pub async fn localize_error_code(
        &self,
        code: &str,
        locale: &str,
        fallback: Option<&str>,
    ) -> Result<String> {
        match self.fetch_error_manifest().await {
            Ok(manifest) => Ok(resolve_manifest(&manifest, code, locale, fallback)),
            Err(_) => Ok(fallback_or_code(code, fallback)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_manifest() -> ErrorManifest {
        ErrorManifest {
            version: "1.0".into(),
            locales: vec!["en".into(), "es".into(), "fr".into()],
            errors: vec![
                ErrorManifestEntry {
                    code: "BAD_REQUEST".into(),
                    messages: [
                        ("en", "The request is invalid."),
                        ("es", "La solicitud no es válida."),
                        ("fr", "La requête n'est pas valide."),
                    ]
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                },
                ErrorManifestEntry {
                    code: "VALIDATION_ERROR".into(),
                    messages: [
                        ("en", "Validation failed."),
                        ("es", "La validación falló."),
                        ("fr", "La validation a échoué."),
                    ]
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                },
            ],
        }
    }

    #[test]
    fn candidate_locales_strips_region_tag_and_appends_en() {
        assert_eq!(
            candidate_locales("es-MX"),
            vec!["es-MX".to_string(), "es".to_string(), "en".to_string()]
        );
    }

    #[test]
    fn candidate_locales_dedupes_en() {
        assert_eq!(candidate_locales("en"), vec!["en".to_string()]);
    }

    #[test]
    fn candidate_locales_strips_on_first_dash_for_three_part() {
        assert_eq!(
            candidate_locales("zh-Hant-TW"),
            vec!["zh-Hant-TW".to_string(), "zh".to_string(), "en".to_string()]
        );
    }

    #[test]
    fn candidate_locales_handles_empty() {
        assert_eq!(candidate_locales(""), vec!["en".to_string()]);
    }

    #[test]
    fn resolve_prefers_exact_locale() {
        let m = build_manifest();
        assert_eq!(
            resolve_manifest(&m, "BAD_REQUEST", "es", None),
            "La solicitud no es válida."
        );
    }

    #[test]
    fn resolve_falls_back_to_base_locale() {
        let m = build_manifest();
        assert_eq!(
            resolve_manifest(&m, "BAD_REQUEST", "es-MX", None),
            "La solicitud no es válida."
        );
    }

    #[test]
    fn resolve_falls_back_to_en_for_unknown_locale() {
        let m = build_manifest();
        assert_eq!(
            resolve_manifest(&m, "BAD_REQUEST", "de", None),
            "The request is invalid."
        );
    }

    #[test]
    fn resolve_falls_back_to_caller_for_unknown_code() {
        let m = build_manifest();
        assert_eq!(
            resolve_manifest(&m, "TOTALLY_FAKE", "es", Some("server-said-so")),
            "server-said-so"
        );
    }

    #[test]
    fn resolve_returns_code_when_nothing_matches_and_no_fallback() {
        let m = build_manifest();
        assert_eq!(
            resolve_manifest(&m, "TOTALLY_FAKE", "es", None),
            "TOTALLY_FAKE"
        );
    }

    #[test]
    fn parse_max_age_extracts_seconds() {
        assert_eq!(
            parse_max_age(Some("public, max-age=3600")),
            Duration::from_secs(3600)
        );
    }

    #[test]
    fn parse_max_age_returns_default_for_missing_header() {
        assert_eq!(parse_max_age(None), DEFAULT_TTL);
    }

    #[test]
    fn parse_max_age_returns_default_when_unparseable() {
        assert_eq!(parse_max_age(Some("public, no-store")), DEFAULT_TTL);
        assert_eq!(parse_max_age(Some("max-age=garbage")), DEFAULT_TTL);
    }

    #[test]
    fn parse_max_age_handles_zero() {
        assert_eq!(parse_max_age(Some("max-age=0")), Duration::from_secs(0));
    }

    #[test]
    fn resolve_from_cache_returns_fallback_when_empty() {
        _reset_cache_for_tests();
        assert_eq!(
            resolve_from_cache("BAD_REQUEST", "es", Some("server fallback")),
            "server fallback"
        );
    }

    #[test]
    fn resolve_from_cache_uses_warm_cache() {
        _reset_cache_for_tests();
        _seed_cache_for_tests(build_manifest(), Duration::from_secs(3600));
        assert_eq!(
            resolve_from_cache("BAD_REQUEST", "es", Some("server")),
            "La solicitud no es válida."
        );
        // Reset so subsequent tests aren't affected (cache is module-global).
        _reset_cache_for_tests();
    }
}
