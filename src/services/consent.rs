//! ConsentService — app-scoped permissions (olympus-cloud-gcp#3254 / #3234 epic).
//!
//! Surface matches §6 of docs/platform/APP-SCOPED-PERMISSIONS.md. Every method
//! hits a platform endpoint; no client-side state. The fast-path bitset check
//! lives on [`crate::OlympusClient`] directly (`has_scope_bit`).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Server-rendered consent copy + stable hash.
///
/// `prompt_hash` must be echoed back on [`ConsentService::grant`] calls so the
/// server can verify the user saw the current catalog copy.
///
/// Shape matches `GET /platform/consent-prompt` (#3242).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentPrompt {
    pub app_id: String,
    pub scope: String,
    pub prompt_text: String,
    pub prompt_hash: String,
    #[serde(default)]
    pub is_destructive: bool,
    #[serde(default)]
    pub requires_mfa: bool,
    #[serde(default)]
    pub app_may_request: bool,
}

/// A grant row from `platform_app_tenant_grants` or `platform_app_user_grants`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grant {
    pub tenant_id: String,
    pub app_id: String,
    pub scope: String,
    pub granted_at: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granted_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
}

/// Holder type — who must consent for this scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    Tenant,
    User,
}

impl Holder {
    fn path_suffix(self) -> &'static str {
        match self {
            Holder::Tenant => "tenant-grants",
            Holder::User => "user-grants",
        }
    }
}

/// Consent surface for tenant-admin and end-user scope grants.
pub struct ConsentService {
    http: Arc<OlympusHttpClient>,
}

impl ConsentService {
    pub(crate) fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// List active (non-revoked) scope grants for an app.
    pub async fn list_granted(
        &self,
        app_id: &str,
        tenant_id: Option<&str>,
        holder: Holder,
    ) -> Result<Vec<Grant>> {
        let path = format!(
            "/api/v1/platform/apps/{}/{}",
            urlencoding::encode(app_id),
            holder.path_suffix()
        );
        let body = if let Some(tid) = tenant_id {
            self.http
                .get_with_query(&path, &[("tenant_id", tid)])
                .await?
        } else {
            self.http.get(&path).await?
        };
        let rows = body
            .get("grants")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(rows
            .into_iter()
            .filter_map(|row| serde_json::from_value(row).ok())
            .collect())
    }

    /// Fetch the consent prompt + hash for a scope.
    ///
    /// Call BEFORE `grant(.., holder=User, prompt_hash=..)` so the returned
    /// `prompt_hash` can be echoed back as proof that what the user saw
    /// matches what the server stores.
    pub async fn describe(&self, app_id: &str, scope: &str) -> Result<ConsentPrompt> {
        let body = self
            .http
            .get_with_query(
                "/platform/consent-prompt",
                &[("app_id", app_id), ("scope", scope)],
            )
            .await?;
        Ok(serde_json::from_value(body)?)
    }

    /// Grant a scope.
    ///
    /// Tenant scopes require `tenant_admin` role; user scopes require the
    /// caller's own JWT. For `holder=User`, `prompt_hash` MUST match the
    /// current server copy (fetched via [`describe`]).
    pub async fn grant(
        &self,
        app_id: &str,
        scope: &str,
        holder: Holder,
        tenant_id: Option<&str>,
        user_id: Option<&str>,
        prompt_hash: Option<&str>,
    ) -> Result<Grant> {
        let path = format!(
            "/api/v1/platform/apps/{}/{}",
            urlencoding::encode(app_id),
            holder.path_suffix()
        );
        let mut body = json!({ "scope": scope });
        if let Some(obj) = body.as_object_mut() {
            if let Some(tid) = tenant_id {
                obj.insert("tenant_id".into(), Value::String(tid.into()));
            }
            if let Some(uid) = user_id {
                obj.insert("user_id".into(), Value::String(uid.into()));
            }
            if let Some(ph) = prompt_hash {
                obj.insert("consent_prompt_hash".into(), Value::String(ph.into()));
            }
        }
        let resp = self.http.post(&path, &body).await?;
        Ok(serde_json::from_value(resp)?)
    }

    /// Revoke a scope (soft-delete — sets `revoked_at`).
    pub async fn revoke(&self, app_id: &str, scope: &str, holder: Holder) -> Result<()> {
        let path = format!(
            "/api/v1/platform/apps/{}/{}/{}",
            urlencoding::encode(app_id),
            holder.path_suffix(),
            urlencoding::encode(scope)
        );
        self.http.delete(&path).await?;
        Ok(())
    }
}
