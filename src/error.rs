use thiserror::Error;

/// SDK error types.
///
/// App-scoped permissions (olympus-cloud-gcp#3234 epic / #3254 issue) adds the
/// five typed variants: `ConsentRequired`, `ScopeDenied`, `BillingGraceExceeded`,
/// `DeviceChanged`, `ExceptionExpired`. Callers match on variants to handle
/// each case idiomatically. See docs/platform/APP-SCOPED-PERMISSIONS.md §6.
#[derive(Error, Debug)]
pub enum OlympusError {
    #[error("HTTP error: {status} {message}")]
    Api { status: u16, message: String },

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Authentication expired")]
    AuthExpired,

    #[error("Configuration error: {0}")]
    Config(String),

    /// The caller attempted to access a scope the user has not granted.
    /// Route to `consent_url` (when present) for the platform-served consent flow.
    #[error("Consent required for scope {scope}: {message}")]
    ConsentRequired {
        scope: String,
        consent_url: Option<String>,
        message: String,
        status: u16,
        request_id: Option<String>,
    },

    /// The scope is granted but the bitset check still failed — typically a
    /// stale JWT from before a revoke. Caller should refresh + retry.
    #[error("Scope denied: {scope}: {message}")]
    ScopeDenied {
        scope: String,
        message: String,
        status: u16,
        request_id: Option<String>,
    },

    /// The tenant's entitlement for this app is in a grace state that blocks
    /// the requested action.
    #[error("Billing grace exceeded: {message}")]
    BillingGraceExceeded {
        message: String,
        grace_until: Option<String>,
        upgrade_url: Option<String>,
        status: u16,
        request_id: Option<String>,
    },

    /// New device fingerprint detected; caller must complete a WebAuthn
    /// challenge (and possibly re-consent if destructive) before retrying.
    #[error("Device changed; WebAuthn required: {message}")]
    DeviceChanged {
        challenge: String,
        requires_reconsent: bool,
        message: String,
        status: u16,
        request_id: Option<String>,
    },

    /// An approved policy exception has transitioned to the `expired`
    /// terminal state. Consumer MUST file a new exception (§17.5).
    #[error("Exception expired: {exception_id}: {message}")]
    ExceptionExpired {
        exception_id: String,
        message: String,
        status: u16,
        request_id: Option<String>,
    },
}

impl OlympusError {
    /// Returns the scope string when the error is one of the scope-bearing
    /// variants (ConsentRequired, ScopeDenied). Convenience for consumer code
    /// that wants to branch on "any scope failure".
    pub fn scope(&self) -> Option<&str> {
        match self {
            OlympusError::ConsentRequired { scope, .. } => Some(scope),
            OlympusError::ScopeDenied { scope, .. } => Some(scope),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, OlympusError>;
