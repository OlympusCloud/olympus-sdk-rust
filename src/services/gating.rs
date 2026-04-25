//! GatingService — runtime feature gating + plan details (#3313).
//!
//! Wraps the Olympus Gating Engine via the Go API Gateway. Today this
//! module exposes the contextual-upgrade plan-details surface (#3313).
//! Other gating routes (`/policies/evaluate`, `/feature-flags`) live on
//! the platform service for now; they can move here as the surface grows.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Single tier in the plan matrix returned by `GET /platform/gating/plan-details` (#3313).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanEntry {
    pub tier_id: String,
    pub display_name: String,
    /// USD dollars (cents/100). `None` for "contact sales" tiers.
    pub monthly_price_usd: Option<f64>,
    pub features: Value,
    pub usage_limits: Value,
    #[serde(default)]
    pub ranks_higher_than_current: bool,
    #[serde(default)]
    pub is_current: bool,
    #[serde(default)]
    pub diff_vs_current: Vec<String>,
    #[serde(default)]
    pub contact_sales: bool,
}

/// Response shape for `GET /platform/gating/plan-details` (#3313).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanDetails {
    pub current_plan: Option<String>,
    pub plans: Vec<PlanEntry>,
    pub as_of: String,
}

/// Runtime feature gating + plan details.
#[derive(Clone)]
pub struct GatingService {
    http: Arc<OlympusHttpClient>,
}

impl GatingService {
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Fetch the full plan matrix + caller's `current_plan` + per-tier
    /// `diff_vs_current` for a contextual upgrade UI (#3313).
    ///
    /// `tenant_id` is optional — when `None`, the platform uses the JWT's
    /// tenant. Cross-tenant lookup requires tenant_admin or higher.
    pub async fn get_plan_details(&self, tenant_id: Option<&str>) -> Result<PlanDetails> {
        let body = if let Some(tid) = tenant_id {
            self.http
                .get_with_query("/platform/gating/plan-details", &[("tenant_id", tid)])
                .await?
        } else {
            self.http.get("/platform/gating/plan-details").await?
        };
        Ok(serde_json::from_value(body)?)
    }
}
