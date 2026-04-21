//! IdentityService — global, cross-tenant Olympus ID + age-verification.
//!
//! Wraps the Olympus Platform service (Rust) Identity handler via the Go API
//! Gateway, plus the `/identity/*` age-verification routes.
//!
//! An [`OlympusIdentity`] is keyed by Firebase UID and represents a single
//! human across every Olympus Cloud app. Call
//! [`IdentityService::get_or_create_from_firebase`] right after a successful
//! Firebase sign-in to materialize the global identity, then
//! [`IdentityService::link_to_tenant`] when the user first transacts with a
//! tenant so the global identity can be cross-referenced with the tenant's
//! commerce customer.
//!
//! Routes:
//! - `POST /platform/identities`          — get-or-create identity
//! - `POST /platform/identities/links`    — link identity to a tenant
//! - `POST /identity/scan-id`             — ID-document age verification (#3009)
//! - `GET  /identity/status/{phone}`      — verification status
//! - `POST /identity/verify-passphrase`   — bcrypt passphrase check
//! - `POST /identity/set-passphrase`      — bcrypt passphrase set
//! - `POST /identity/create-upload-session` — signed upload session for ID photo

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Global identity representing a consumer or business operator across all
/// Olympus Cloud apps. Backed by `platform_olympus_identities` in Spanner;
/// created on first Firebase sign-in and reused thereafter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OlympusIdentity {
    /// Server-assigned global identity UUID. Stable across tenants.
    pub id: String,
    /// Firebase Auth UID. Unique per signed-in user.
    pub firebase_uid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    /// Free-form JSON for cross-app preferences (theme, locale, accessibility).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_preferences: Option<Map<String, Value>>,
    /// Cross-tenant Stripe customer ID, used by Olympus Pay for federated
    /// checkout flows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stripe_customer_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A binding between an [`OlympusIdentity`] and a tenant-scoped commerce
/// customer. One Olympus identity can have many links — one per tenant the
/// user has done business with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityLink {
    /// The global identity this link belongs to.
    pub olympus_id: String,
    /// Tenant the user has a relationship with.
    pub tenant_id: String,
    /// Tenant-scoped commerce customer record.
    pub commerce_customer_id: String,
    /// When the link was first established (ISO-8601).
    pub linked_at: String,
}

/// Parameters for [`IdentityService::get_or_create_from_firebase`].
#[derive(Debug, Clone, Default)]
pub struct GetOrCreateIdentityRequest<'a> {
    pub firebase_uid: &'a str,
    pub email: Option<&'a str>,
    pub phone: Option<&'a str>,
    pub first_name: Option<&'a str>,
    pub last_name: Option<&'a str>,
    pub global_preferences: Option<Map<String, Value>>,
}

/// Olympus ID — global, cross-tenant identity + age verification.
pub struct IdentityService {
    http: Arc<OlympusHttpClient>,
}

impl IdentityService {
    /// Creates a new IdentityService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Get-or-create the global Olympus identity for a Firebase user.
    ///
    /// If an identity already exists for `firebase_uid` it is returned
    /// unchanged; the optional fields are only used when a new row has to
    /// be inserted. Safe to call on every sign-in — it is idempotent.
    pub async fn get_or_create_from_firebase(
        &self,
        req: GetOrCreateIdentityRequest<'_>,
    ) -> Result<OlympusIdentity> {
        let body = json!({
            "firebase_uid": req.firebase_uid,
            "email": req.email,
            "phone": req.phone,
            "first_name": req.first_name,
            "last_name": req.last_name,
            "global_preferences": req.global_preferences,
        });
        let raw = self.http.post("/platform/identities", &body).await?;
        Ok(serde_json::from_value(raw)?)
    }

    /// Link a global identity to a tenant-scoped commerce customer.
    ///
    /// Should be called the first time a federated user transacts with a
    /// new tenant — typically immediately after the tenant's commerce
    /// service creates the per-tenant customer record. Safe to call again;
    /// the platform de-duplicates by `(olympus_id, tenant_id)`.
    pub async fn link_to_tenant(
        &self,
        olympus_id: &str,
        tenant_id: &str,
        commerce_customer_id: &str,
    ) -> Result<()> {
        let body = json!({
            "olympus_id": olympus_id,
            "tenant_id": tenant_id,
            "commerce_customer_id": commerce_customer_id,
        });
        self.http.post("/platform/identities/links", &body).await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Age Verification (Document AI) — #3009
    // -----------------------------------------------------------------------

    /// Scan an ID document for age verification via Google Document AI.
    /// Image is processed and immediately deleted — only the DOB hash + age
    /// are stored.
    ///
    /// The image is sent as a JSON array of bytes (matches Dart SDK shape).
    pub async fn scan_id(&self, phone: &str, image_bytes: &[u8]) -> Result<Value> {
        let body = json!({
            "phone": phone,
            "image": image_bytes,
        });
        self.http.post("/identity/scan-id", &body).await
    }

    /// Check a caller's verification status.
    pub async fn check_verification_status(&self, phone: &str) -> Result<Value> {
        let path = format!("/identity/status/{}", urlencoding::encode(phone));
        self.http.get(&path).await
    }

    /// Verify a caller's passphrase (bcrypt comparison).
    pub async fn verify_passphrase(&self, phone: &str, passphrase: &str) -> Result<Value> {
        let body = json!({ "phone": phone, "passphrase": passphrase });
        self.http.post("/identity/verify-passphrase", &body).await
    }

    /// Set or update a caller's passphrase (bcrypt hashed).
    pub async fn set_passphrase(&self, phone: &str, passphrase: &str) -> Result<Value> {
        let body = json!({ "phone": phone, "passphrase": passphrase });
        self.http.post("/identity/set-passphrase", &body).await
    }

    /// Create a signed upload URL for the caller to upload their ID photo.
    pub async fn create_upload_session(&self) -> Result<Value> {
        self.http
            .post("/identity/create-upload-session", &json!({}))
            .await
    }
}
