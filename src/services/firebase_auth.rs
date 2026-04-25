//! Firebase federation models. The actual `login_with_firebase` and
//! `link_firebase` methods live on [`crate::services::auth::AuthService`];
//! this module just exports the types used in their signatures and in the
//! typed-error variants on [`crate::error::OlympusError`].
//!
//! Backend: `POST /auth/firebase/exchange` and `POST /auth/firebase/link`,
//! served by `backend/rust/auth/src/handlers/mod.rs` (gcp#3293, deployed
//! to dev). See gcp#3473 for the SDK fanout.

use serde::{Deserialize, Serialize};

/// Result of a successful `POST /auth/firebase/link`.
///
/// `linked_at` is the wall-clock at which the link was first established
/// (RFC3339 string). For idempotent re-link calls this is the ORIGINAL
/// link time, not "now". Kept as `String` rather than a `DateTime<Utc>` so
/// the SDK has zero `chrono` dependency surface; callers that want a
/// typed timestamp can `linked_at.parse::<DateTime<Utc>>()` themselves.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FirebaseLinkResult {
    pub olympus_id: String,
    pub firebase_uid: String,
    pub linked_at: String,
}

/// One candidate tenant returned in a 409 `multiple_tenants_match` response
/// from `/auth/firebase/exchange`. Apps render a picker populated with
/// these and retry with an explicit `tenant_slug`.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct FirebaseTenantOption {
    pub tenant_id: String,
    pub tenant_slug: String,
    pub tenant_name: String,
}

/// Optional inputs to [`crate::services::auth::AuthService::login_with_firebase`].
#[derive(Clone, Debug, Default)]
pub struct LoginWithFirebaseOptions {
    /// Skip auto-resolution by passing an explicit slug.
    pub tenant_slug: Option<String>,
    /// First-time signup flows where the caller has no existing identity
    /// link but has been issued an invite.
    pub invite_token: Option<String>,
}
