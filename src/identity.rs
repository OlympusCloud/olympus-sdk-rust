//! IdentityApi — canonical `/identity/invite*` + `/identity/remove_from_tenant`
//! SDK surface (#3403 §4.2 + §4.4).
//!
//! Wraps the Olympus Auth service `identity_invite` handler (shipped in
//! PR #3410) exposed through the Go API Gateway. Apps use this to invite
//! staff / managers, list pending invites, accept or revoke invites, and
//! remove users from a tenant while preserving their global Firebase
//! identity.
//!
//! **Naming note**: the existing `OlympusClient::identity()` accessor
//! returns the global Olympus-ID / age-verification
//! [`crate::services::identity::IdentityService`]. This module is a
//! distinct surface under [`OlympusClient::identity_invites`].
//!
//! # Route map
//!
//! | Method | Route                                   | SDK method                          |
//! |--------|-----------------------------------------|-------------------------------------|
//! | POST   | /identity/invite                        | [`IdentityApi::invite`]             |
//! | POST   | /identity/invites/:token/accept         | [`IdentityApi::accept`]             |
//! | GET    | /identity/invites                        | [`IdentityApi::list`]               |
//! | POST   | /identity/invites/:id/revoke            | [`IdentityApi::revoke`]             |
//! | POST   | /identity/remove_from_tenant            | [`IdentityApi::remove_from_tenant`] |

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::client::OlympusClient;
use crate::error::Result;

// ---------------------------------------------------------------------------
// Request / response shapes — mirror backend identity_invite handler exactly.
// ---------------------------------------------------------------------------

/// Payload for [`IdentityApi::invite`]. `ttl_seconds` (if set) is capped at
/// 30 days server-side; unset falls back to the backend default of 7 days.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InviteCreateRequest {
    pub email: String,
    /// Must match a role from `docs/platform/roles.yaml`. The backend rejects
    /// unknown roles with `422` before any DB write.
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<i64>,
}

/// Lifecycle state of a pending invite. Serialized as `snake_case` on the
/// wire to match the backend `InviteStatus` enum.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InviteStatus {
    Pending,
    Accepted,
    Revoked,
    Expired,
}

/// A single invite row. The `token` field is ONLY populated on the
/// [`IdentityApi::invite`] response — list/revoke/accept responses omit it
/// (the server stores only the SHA-256 token hash after issuance).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteHandle {
    pub id: String,
    /// Signed invite-token JWT. Returned exclusively on create. Deliver this
    /// to the invitee over email / SMS / deep link — it's the only material
    /// required to accept the invite.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    pub email: String,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location_id: Option<String>,
    pub tenant_id: String,
    /// RFC3339 UTC timestamp.
    pub expires_at: String,
    pub status: InviteStatus,
    /// RFC3339 UTC timestamp.
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_at: Option<String>,
}

/// Response payload from [`IdentityApi::remove_from_tenant`]. The global
/// Firebase identity is NOT deleted — only the tenant-scoped `auth_users`
/// row is removed, plus any role assignments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveFromTenantResponse {
    pub tenant_id: String,
    pub user_id: String,
    /// RFC3339 UTC timestamp.
    pub removed_at: String,
}

// ---------------------------------------------------------------------------
// IdentityApi
// ---------------------------------------------------------------------------

/// Access to `/identity/invite*` endpoints. Obtain via
/// [`OlympusClient::identity_invites`].
///
/// Borrow pattern — holds a shared reference to the parent client. Drop
/// when done; cheap to construct per call site.
pub struct IdentityApi<'a> {
    client: &'a OlympusClient,
}

impl<'a> IdentityApi<'a> {
    /// Constructs a new `IdentityApi`. Usually obtained via
    /// [`OlympusClient::identity_invites`] rather than directly.
    pub fn new(client: &'a OlympusClient) -> Self {
        Self { client }
    }

    /// `POST /identity/invite` — create a pending invite and mint the signed
    /// invite token. Requires `manager` or `tenant_admin` on the server.
    ///
    /// The returned [`InviteHandle::token`] is the only place the plaintext
    /// token is ever exposed — after this call the backend stores only the
    /// SHA-256 hash. Distribute the token to the invitee over a secure
    /// channel before the response leaves the SDK.
    pub async fn invite(&self, req: InviteCreateRequest) -> Result<InviteHandle> {
        let body = serde_json::to_value(&req)?;
        let raw = self.client.http().post("/identity/invite", &body).await?;
        Ok(serde_json::from_value(raw)?)
    }

    /// `POST /identity/invites/:token/accept` — accept an invite using the
    /// signed token from [`IdentityApi::invite`].
    ///
    /// The caller presents a Firebase ID token so the backend can bind the
    /// accepted invite to the caller's global Firebase identity. On success
    /// the backend returns a full [`crate::tenant::ExchangedSession`]-shaped
    /// payload ready to be used for subsequent requests.
    pub async fn accept(&self, token: &str, firebase_id_token: &str) -> Result<Value> {
        let path = format!("/identity/invites/{}/accept", urlencoding::encode(token));
        let body = json!({ "firebase_id_token": firebase_id_token });
        self.client.http().post(&path, &body).await
    }

    /// `GET /identity/invites` — list all invites (pending, accepted,
    /// revoked, expired) for the caller's tenant. Capped at 500 rows
    /// server-side, ordered by `created_at DESC`.
    pub async fn list(&self) -> Result<Vec<InviteHandle>> {
        let raw = self.client.http().get("/identity/invites").await?;
        Ok(serde_json::from_value(raw)?)
    }

    /// `POST /identity/invites/:id/revoke` — revoke a pending invite. The
    /// invite's `status` flips to [`InviteStatus::Revoked`]; subsequent
    /// accept attempts for this token fail.
    pub async fn revoke(&self, invite_id: &str) -> Result<InviteHandle> {
        let path = format!(
            "/identity/invites/{}/revoke",
            urlencoding::encode(invite_id)
        );
        let raw = self.client.http().post(&path, &json!({})).await?;
        Ok(serde_json::from_value(raw)?)
    }

    /// `POST /identity/remove_from_tenant` — remove a user from the caller's
    /// tenant while preserving their global Firebase identity. Requires
    /// `tenant_admin` on the server.
    ///
    /// `user_id` is the tenant-scoped `auth_users.id` (UUID). `reason` is
    /// optional and surfaces on the `identity.removed_from_tenant` Pub/Sub
    /// event for audit retention.
    pub async fn remove_from_tenant(
        &self,
        user_id: &str,
        reason: Option<&str>,
    ) -> Result<RemoveFromTenantResponse> {
        let body = json!({
            "user_id": user_id,
            "reason": reason,
        });
        let raw = self
            .client
            .http()
            .post("/identity/remove_from_tenant", &body)
            .await?;
        Ok(serde_json::from_value(raw)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invite_status_serializes_lowercase() {
        assert_eq!(
            serde_json::to_value(InviteStatus::Pending).unwrap(),
            json!("pending")
        );
        assert_eq!(
            serde_json::to_value(InviteStatus::Accepted).unwrap(),
            json!("accepted")
        );
        assert_eq!(
            serde_json::to_value(InviteStatus::Revoked).unwrap(),
            json!("revoked")
        );
        assert_eq!(
            serde_json::to_value(InviteStatus::Expired).unwrap(),
            json!("expired")
        );
    }

    #[test]
    fn invite_status_deserializes_lowercase() {
        let s: InviteStatus = serde_json::from_str("\"pending\"").unwrap();
        assert_eq!(s, InviteStatus::Pending);
    }

    #[test]
    fn invite_create_request_skips_none_fields() {
        let req = InviteCreateRequest {
            email: "a@b.co".into(),
            role: "manager".into(),
            location_id: None,
            message: None,
            ttl_seconds: None,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["email"], "a@b.co");
        assert_eq!(json["role"], "manager");
        assert!(json.get("location_id").is_none());
        assert!(json.get("message").is_none());
        assert!(json.get("ttl_seconds").is_none());
    }

    #[test]
    fn invite_handle_without_token_deserializes() {
        let value = json!({
            "id": "inv_1",
            "email": "a@b.co",
            "role": "manager",
            "tenant_id": "t_1",
            "expires_at": "2026-05-01T00:00:00Z",
            "status": "pending",
            "created_at": "2026-04-20T00:00:00Z",
        });
        let h: InviteHandle = serde_json::from_value(value).unwrap();
        assert!(h.token.is_none());
        assert_eq!(h.status, InviteStatus::Pending);
    }
}
