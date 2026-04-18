use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{OlympusError, Result};
use crate::http::OlympusHttpClient;

/// Marketing-funnel + pre-conversion lead capture.
///
/// Routes: `/connect/*`, `/leads`.
///
/// Issue OlympusCloud/olympus-cloud-gcp#3108 — the `/leads` endpoint is
/// intentionally unauthenticated so marketing surfaces can POST leads before
/// the user signs up. Idempotency is email-based over a 1h window.
pub struct ConnectService {
    http: Arc<OlympusHttpClient>,
}

/// Standard UTM tracking parameters captured from a landing page.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UTM {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medium: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub campaign: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub term: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Payload for creating a pre-conversion lead. See #3108.
#[derive(Debug, Clone, Default, Serialize)]
pub struct CreateLeadRequest {
    /// Lead's email — used for idempotency dedup (1h window per lead).
    pub email: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utm: Option<UTM>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Map<String, Value>>,
}

/// Response from `POST /leads`.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateLeadResponse {
    /// `lead_id` on the wire; exposed as `lead_id` on the Rust struct too.
    #[serde(alias = "leadId")]
    pub lead_id: String,
    pub status: String, // "created" or "deduped"
    #[serde(alias = "createdAt")]
    pub created_at: String,
}

impl ConnectService {
    /// Creates a new ConnectService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Creates a pre-conversion lead. Safe to retry — deduplicates on email.
    ///
    /// Backing endpoint: `POST /api/v1/leads`.
    pub async fn create_lead(
        &self,
        request: &CreateLeadRequest,
    ) -> Result<CreateLeadResponse> {
        if request.email.is_empty() {
            return Err(OlympusError::Config("email is required".into()));
        }
        let body = serde_json::to_value(request).map_err(OlympusError::from)?;
        let raw: Value = self.http.post("/leads", &body).await?;
        let resp: CreateLeadResponse =
            serde_json::from_value(raw).map_err(OlympusError::from)?;
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_lead_request_serializes_without_empty_optionals() {
        let req = CreateLeadRequest {
            email: "scott@example.com".into(),
            name: Some("Scott".into()),
            utm: Some(UTM {
                source: Some("twitter".into()),
                campaign: Some("spring-launch".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let v = serde_json::to_value(&req).expect("serialize");
        assert_eq!(v["email"], "scott@example.com");
        assert_eq!(v["name"], "Scott");
        assert!(v.get("phone").is_none());
        assert_eq!(v["utm"]["source"], "twitter");
        assert_eq!(v["utm"]["campaign"], "spring-launch");
        assert!(v["utm"].get("medium").is_none());
    }

    #[test]
    fn create_lead_response_deserializes_snake_case() {
        let raw = r#"{
            "lead_id": "lead-xyz",
            "status": "created",
            "created_at": "2026-04-18T03:00:00Z"
        }"#;
        let resp: CreateLeadResponse = serde_json::from_str(raw).expect("deserialize");
        assert_eq!(resp.lead_id, "lead-xyz");
        assert_eq!(resp.status, "created");
    }

    #[test]
    fn create_lead_response_deserializes_camel_case_alias() {
        let raw = r#"{
            "leadId": "lead-abc",
            "status": "deduped",
            "createdAt": "2026-04-18T00:00:00Z"
        }"#;
        let resp: CreateLeadResponse = serde_json::from_str(raw).expect("deserialize");
        assert_eq!(resp.lead_id, "lead-abc");
        assert_eq!(resp.status, "deduped");
    }
}
