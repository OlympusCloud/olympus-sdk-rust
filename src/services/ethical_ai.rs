use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Ethical AI governance service for bias detection, red-teaming, model cards,
/// and AI safety policy enforcement.
///
/// Wraps the Olympus Ethical AI endpoints (Python) via the Go API Gateway.
/// Routes: `/ethical-ai/*`.
///
/// Related issues: #2920 (1.0 Governance Readiness), #2935 (Bias Detection),
/// #2936 (Model Cards), #2937 (Red-Team)
pub struct EthicalAiService {
    http: Arc<OlympusHttpClient>,
}

/// Options for running a bias audit.
#[derive(Default)]
pub struct BiasAuditOptions<'a> {
    /// Model identifier to audit.
    pub model_id: &'a str,
    /// Dataset identifier for bias evaluation.
    pub dataset_id: &'a str,
    /// Protected attributes to check: gender, race, age, disability, etc.
    pub protected_attributes: Vec<String>,
    /// Fairness metrics: demographic_parity, equalized_odds, predictive_parity.
    pub metrics: Vec<String>,
}

/// Options for submitting a red-team prompt.
#[derive(Default)]
pub struct RedTeamOptions<'a> {
    /// The adversarial prompt to test.
    pub prompt: &'a str,
    /// Target model identifier.
    pub model_id: &'a str,
    /// Attack category: jailbreak, injection, extraction, hallucination.
    pub attack_category: &'a str,
}

impl EthicalAiService {
    /// Creates a new EthicalAiService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    // ─── Bias Detection (#2935) ───────────────────────────────────

    /// Run a bias audit on a model with specific protected attributes.
    pub async fn run_bias_audit(&self, opts: BiasAuditOptions<'_>) -> Result<Value> {
        let body = json!({
            "model_id": opts.model_id,
            "dataset_id": opts.dataset_id,
            "protected_attributes": opts.protected_attributes,
            "metrics": opts.metrics,
        });
        self.http.post("/ethical-ai/bias/audit", &body).await
    }

    /// Get bias audit results by audit ID.
    pub async fn get_bias_report(&self, audit_id: &str) -> Result<Value> {
        self.http
            .get(&format!("/ethical-ai/bias/report?audit_id={}", audit_id))
            .await
    }

    /// List all bias audits for the tenant.
    pub async fn list_bias_audits(&self) -> Result<Value> {
        self.http.get("/ethical-ai/bias/audits").await
    }

    // ─── Red-Teaming (#2937) ──────────────────────────────────────

    /// Submit a red-team adversarial prompt for testing.
    pub async fn submit_redteam_prompt(&self, opts: RedTeamOptions<'_>) -> Result<Value> {
        let body = json!({
            "prompt": opts.prompt,
            "model_id": opts.model_id,
            "attack_category": opts.attack_category,
        });
        self.http.post("/ethical-ai/redteam/submit", &body).await
    }

    /// Get red-team campaign results.
    pub async fn get_redteam_results(&self, campaign_id: &str) -> Result<Value> {
        self.http
            .get(&format!(
                "/ethical-ai/redteam/results?campaign_id={}",
                campaign_id
            ))
            .await
    }

    /// List all red-team campaigns.
    pub async fn list_redteam_campaigns(&self) -> Result<Value> {
        self.http.get("/ethical-ai/redteam/campaigns").await
    }

    // ─── Model Cards (#2936) ──────────────────────────────────────

    /// Create or update a model card.
    pub async fn upsert_model_card(&self, model_id: &str, card: Value) -> Result<Value> {
        let mut body = card;
        body["model_id"] = Value::String(model_id.to_string());
        self.http.post("/ethical-ai/model-cards", &body).await
    }

    /// Get a model card by model ID.
    pub async fn get_model_card(&self, model_id: &str) -> Result<Value> {
        self.http
            .get(&format!("/ethical-ai/model-cards/{}", model_id))
            .await
    }

    /// List all registered model cards.
    pub async fn list_model_cards(&self) -> Result<Value> {
        self.http.get("/ethical-ai/model-cards").await
    }

    // ─── AI Safety Policy ─────────────────────────────────────────

    /// Get the current AI safety policy configuration.
    pub async fn get_safety_policy(&self) -> Result<Value> {
        self.http.get("/ethical-ai/safety/policy").await
    }

    /// Update AI safety policy rules.
    pub async fn update_safety_policy(&self, policy: Value) -> Result<Value> {
        self.http.post("/ethical-ai/safety/policy", &policy).await
    }

    /// Get the AI safety compliance dashboard.
    pub async fn safety_dashboard(&self) -> Result<Value> {
        self.http.get("/ethical-ai/safety/dashboard").await
    }

    // ─── Explainability ───────────────────────────────────────────

    /// Get explainability report for a specific AI inference.
    pub async fn explain_inference(&self, inference_id: &str) -> Result<Value> {
        self.http
            .get(&format!("/ethical-ai/explainability/{}", inference_id))
            .await
    }

    /// Get aggregate explainability metrics.
    pub async fn explainability_metrics(&self) -> Result<Value> {
        self.http.get("/ethical-ai/explainability/metrics").await
    }
}
