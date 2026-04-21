use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// SRE analytics service for SLO tracking, synthetic monitoring, capacity planning,
/// and incident management.
///
/// Wraps the Olympus SRE Analytics endpoints (Python) via the Go API Gateway.
/// Routes: `/sre/*`.
///
/// Related issues: #2920 (1.0 Governance Readiness), #2945 (SLO Tracking),
/// #2946 (Synthetic Monitoring), #2947 (Capacity Planning)
pub struct SreAnalyticsService {
    http: Arc<OlympusHttpClient>,
}

/// SLO definition for tracking.
pub struct SloDefinition<'a> {
    /// Service identifier.
    pub service: &'a str,
    /// SLI type: availability, latency_p50, latency_p99, error_rate, throughput.
    pub sli_type: &'a str,
    /// Target value (e.g. 0.999 for 99.9% availability).
    pub target: f64,
    /// Rolling window in hours (e.g. 720 for 30 days).
    pub window_hours: u32,
}

/// Synthetic probe configuration.
pub struct SyntheticProbe<'a> {
    /// Probe name.
    pub name: &'a str,
    /// Target URL to probe.
    pub target_url: &'a str,
    /// Check interval in seconds.
    pub interval_secs: u32,
    /// Expected status code.
    pub expected_status: u16,
    /// Timeout in milliseconds.
    pub timeout_ms: u32,
    /// Probe regions: us-central1, europe-west1, asia-east1, etc.
    pub regions: Vec<String>,
}

impl SreAnalyticsService {
    /// Creates a new SreAnalyticsService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    // ─── SLO Tracking (#2945) ─────────────────────────────────────

    /// Create or update an SLO definition.
    pub async fn upsert_slo(&self, slo: SloDefinition<'_>) -> Result<Value> {
        let body = json!({
            "service": slo.service,
            "sli_type": slo.sli_type,
            "target": slo.target,
            "window_hours": slo.window_hours,
        });
        self.http.post("/sre/slos", &body).await
    }

    /// Get all SLO definitions.
    pub async fn list_slos(&self) -> Result<Value> {
        self.http.get("/sre/slos").await
    }

    /// Get current SLO burn rate and error budget for a service.
    pub async fn slo_status(&self, service: &str) -> Result<Value> {
        self.http
            .get(&format!("/sre/slos/{}/status", service))
            .await
    }

    /// Get the SLO dashboard with all services' error budgets.
    pub async fn slo_dashboard(&self) -> Result<Value> {
        self.http.get("/sre/slos/dashboard").await
    }

    // ─── Synthetic Monitoring (#2946) ─────────────────────────────

    /// Create a synthetic monitoring probe.
    pub async fn create_probe(&self, probe: SyntheticProbe<'_>) -> Result<Value> {
        let body = json!({
            "name": probe.name,
            "target_url": probe.target_url,
            "interval_secs": probe.interval_secs,
            "expected_status": probe.expected_status,
            "timeout_ms": probe.timeout_ms,
            "regions": probe.regions,
        });
        self.http.post("/sre/synthetic/probes", &body).await
    }

    /// List all synthetic probes.
    pub async fn list_probes(&self) -> Result<Value> {
        self.http.get("/sre/synthetic/probes").await
    }

    /// Get probe results/history.
    pub async fn probe_results(&self, probe_id: &str, limit: Option<u32>) -> Result<Value> {
        let mut path = format!("/sre/synthetic/probes/{}/results", probe_id);
        if let Some(l) = limit {
            path.push_str(&format!("?limit={}", l));
        }
        self.http.get(&path).await
    }

    /// Delete a synthetic probe.
    pub async fn delete_probe(&self, probe_id: &str) -> Result<Value> {
        self.http
            .delete(&format!("/sre/synthetic/probes/{}", probe_id))
            .await
    }

    // ─── Capacity Planning (#2947) ────────────────────────────────

    /// Get capacity planning forecast for a service.
    pub async fn capacity_forecast(
        &self,
        service: &str,
        horizon_days: Option<u32>,
    ) -> Result<Value> {
        let mut path = format!("/sre/capacity/forecast?service={}", service);
        if let Some(d) = horizon_days {
            path.push_str(&format!("&horizon_days={}", d));
        }
        self.http.get(&path).await
    }

    /// Get current resource utilization across services.
    pub async fn resource_utilization(&self) -> Result<Value> {
        self.http.get("/sre/capacity/utilization").await
    }

    /// Get scaling recommendations.
    pub async fn scaling_recommendations(&self) -> Result<Value> {
        self.http.get("/sre/capacity/recommendations").await
    }

    // ─── Incident Management ──────────────────────────────────────

    /// List active incidents.
    pub async fn list_incidents(&self, status: Option<&str>) -> Result<Value> {
        let mut path = "/sre/incidents".to_string();
        if let Some(s) = status {
            path.push_str(&format!("?status={}", s));
        }
        self.http.get(&path).await
    }

    /// Create an incident.
    pub async fn create_incident(
        &self,
        title: &str,
        severity: &str,
        affected_services: Vec<String>,
    ) -> Result<Value> {
        let body = json!({
            "title": title,
            "severity": severity,
            "affected_services": affected_services,
        });
        self.http.post("/sre/incidents", &body).await
    }

    /// Update incident status.
    pub async fn update_incident(
        &self,
        incident_id: &str,
        status: &str,
        notes: &str,
    ) -> Result<Value> {
        let body = json!({
            "status": status,
            "notes": notes,
        });
        self.http
            .post(&format!("/sre/incidents/{}/update", incident_id), &body)
            .await
    }

    /// Get incident timeline.
    pub async fn incident_timeline(&self, incident_id: &str) -> Result<Value> {
        self.http
            .get(&format!("/sre/incidents/{}/timeline", incident_id))
            .await
    }

    // ─── On-Call ──────────────────────────────────────────────────

    /// Get current on-call schedule.
    pub async fn oncall_schedule(&self) -> Result<Value> {
        self.http.get("/sre/oncall/schedule").await
    }

    /// Get on-call rotation for a specific service.
    pub async fn oncall_for_service(&self, service: &str) -> Result<Value> {
        self.http
            .get(&format!("/sre/oncall/service/{}", service))
            .await
    }
}
