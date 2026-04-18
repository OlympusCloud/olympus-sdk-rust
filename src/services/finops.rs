use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// FinOps service for AI cost management, budget enforcement, and cost anomaly detection.
///
/// Wraps the Olympus FinOps endpoints (Python) via the Go API Gateway.
/// Routes: `/finops/*`.
///
/// Related issues: #2920 (1.0 Governance Readiness), #2941 (FinOps Dashboard),
/// #2942 (Budget Enforcement), #2943 (Cost Anomaly Detection)
pub struct FinOpsService {
    http: Arc<OlympusHttpClient>,
}

/// Budget configuration for enforcement.
pub struct BudgetConfig<'a> {
    /// Tenant ID to set budget for.
    pub tenant_id: &'a str,
    /// Monthly budget in USD cents.
    pub monthly_budget_cents: u64,
    /// Alert threshold as a fraction (0.0 to 1.0). Triggers alert when spending exceeds this fraction of budget.
    pub alert_threshold: f64,
    /// Hard limit — whether to block requests when budget is exceeded.
    pub hard_limit: bool,
}

impl FinOpsService {
    /// Creates a new FinOpsService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    // ─── FinOps Dashboard (#2941) ─────────────────────────────────

    /// Get the FinOps cost dashboard with aggregated metrics.
    pub async fn dashboard(&self, period: Option<&str>) -> Result<Value> {
        let mut path = "/finops/dashboard".to_string();
        if let Some(p) = period {
            path.push_str(&format!("?period={}", p));
        }
        self.http.get(&path).await
    }

    /// Get per-model cost breakdown.
    pub async fn cost_by_model(&self, period: Option<&str>) -> Result<Value> {
        let mut path = "/finops/costs/by-model".to_string();
        if let Some(p) = period {
            path.push_str(&format!("?period={}", p));
        }
        self.http.get(&path).await
    }

    /// Get per-tenant cost breakdown.
    pub async fn cost_by_tenant(&self, period: Option<&str>) -> Result<Value> {
        let mut path = "/finops/costs/by-tenant".to_string();
        if let Some(p) = period {
            path.push_str(&format!("?period={}", p));
        }
        self.http.get(&path).await
    }

    /// Get cost trend over time.
    pub async fn cost_trend(
        &self,
        start_date: &str,
        end_date: &str,
        granularity: Option<&str>,
    ) -> Result<Value> {
        let mut path = format!(
            "/finops/costs/trend?start={}&end={}",
            start_date, end_date
        );
        if let Some(g) = granularity {
            path.push_str(&format!("&granularity={}", g));
        }
        self.http.get(&path).await
    }

    // ─── Budget Enforcement (#2942) ───────────────────────────────

    /// Set or update a budget for a tenant.
    pub async fn set_budget(&self, config: BudgetConfig<'_>) -> Result<Value> {
        let body = json!({
            "tenant_id": config.tenant_id,
            "monthly_budget_cents": config.monthly_budget_cents,
            "alert_threshold": config.alert_threshold,
            "hard_limit": config.hard_limit,
        });
        self.http.post("/finops/budgets", &body).await
    }

    /// Get budget status for a tenant.
    pub async fn get_budget(&self, tenant_id: &str) -> Result<Value> {
        self.http
            .get(&format!("/finops/budgets/{}", tenant_id))
            .await
    }

    /// List all budgets.
    pub async fn list_budgets(&self) -> Result<Value> {
        self.http.get("/finops/budgets").await
    }

    /// Get budget alerts (tenants approaching or exceeding limits).
    pub async fn budget_alerts(&self) -> Result<Value> {
        self.http.get("/finops/budgets/alerts").await
    }

    // ─── Cost Anomaly Detection (#2943) ───────────────────────────

    /// Get detected cost anomalies.
    pub async fn list_anomalies(
        &self,
        severity: Option<&str>,
    ) -> Result<Value> {
        let mut path = "/finops/anomalies".to_string();
        if let Some(s) = severity {
            path.push_str(&format!("?severity={}", s));
        }
        self.http.get(&path).await
    }

    /// Acknowledge a cost anomaly.
    pub async fn acknowledge_anomaly(
        &self,
        anomaly_id: &str,
        notes: &str,
    ) -> Result<Value> {
        let body = json!({
            "anomaly_id": anomaly_id,
            "notes": notes,
        });
        self.http.post("/finops/anomalies/ack", &body).await
    }

    /// Get cost optimization recommendations.
    pub async fn recommendations(&self) -> Result<Value> {
        self.http.get("/finops/recommendations").await
    }

    // ─── AI Cost Attribution ──────────────────────────────────────

    /// Get AI inference cost attribution by feature.
    pub async fn ai_cost_attribution(
        &self,
        period: Option<&str>,
    ) -> Result<Value> {
        let mut path = "/finops/ai-costs/attribution".to_string();
        if let Some(p) = period {
            path.push_str(&format!("?period={}", p));
        }
        self.http.get(&path).await
    }

    /// Get token usage statistics across all AI models.
    pub async fn token_usage(&self, period: Option<&str>) -> Result<Value> {
        let mut path = "/finops/ai-costs/tokens".to_string();
        if let Some(p) = period {
            path.push_str(&format!("?period={}", p));
        }
        self.http.get(&path).await
    }
}
