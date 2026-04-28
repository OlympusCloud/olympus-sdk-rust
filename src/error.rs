use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum OlympusError {
    /// Structured API error matching the canonical
    /// ``{error: {code, message, request_id}}`` envelope produced by the
    /// Olympus Cloud platform. ``code`` is the canonical UPPER_SNAKE
    /// identifier used by the i18n manifest (#3638 / #3626) — call
    /// :meth:`OlympusError::localized_message` to render it in the user's
    /// locale.
    #[error("OlympusApi({code}): {message} [status={status}, reqId={request_id:?}]")]
    Api {
        /// HTTP status from the upstream response.
        status: u16,
        /// Canonical UPPER_SNAKE error code (e.g., ``"NOT_FOUND"``).
        code: String,
        /// Server-provided English message — used as the final fallback in
        /// :meth:`localized_message` when the i18n manifest doesn't have a
        /// translation for the requested locale.
        message: String,
        /// Server-assigned request ID, when present.
        request_id: Option<String>,
    },

    #[error("Network error: {0}")]
    Network(String),

    #[error("JSON error: {0}")]
    Json(String),

    #[error("Authentication expired")]
    AuthExpired,

    #[error("Configuration error: {0}")]
    Config(String),
}

impl From<reqwest::Error> for OlympusError {
    fn from(err: reqwest::Error) -> Self {
        OlympusError::Network(err.to_string())
    }
}

impl From<serde_json::Error> for OlympusError {
    fn from(err: serde_json::Error) -> Self {
        OlympusError::Json(err.to_string())
    }
}

impl OlympusError {
    /// Resolves a localized message for ``locale`` against the cached i18n
    /// manifest (#3638 / parent #3626). Falls back through the locale chain
    /// (``es-MX`` → ``es`` → ``en``) and finally to the server-provided
    /// ``message`` if the manifest is missing or the code isn't listed.
    /// Never returns an empty string.
    ///
    /// Synchronous — call ``client.i18n().fetch_error_manifest().await``
    /// once at app startup to warm the module-level cache before relying
    /// on this lookup. Non-``Api`` variants return their default
    /// ``Display`` representation.
    pub fn localized_message(&self, locale: &str) -> String {
        match self {
            OlympusError::Api { code, message, .. } => {
                crate::i18n::resolve_from_cache(code, locale, Some(message))
            }
            other => other.to_string(),
        }
    }

    /// Returns the canonical error code if this is a structured API error.
    pub fn code(&self) -> Option<&str> {
        match self {
            OlympusError::Api { code, .. } => Some(code.as_str()),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, OlympusError>;
