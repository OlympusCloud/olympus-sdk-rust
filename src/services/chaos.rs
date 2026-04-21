use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Chaos engineering service for fault injection, DR drills, and gameday execution.
///
/// Wraps the Olympus Chaos Engineering endpoints (Python) via the Go API Gateway.
/// Routes: `/chaos/queue/*`, `/chaos/dr-drills/*`, `/chaos/gamedays/*`.
///
/// Related issues: #2938, #2939, #2940, #2920 (1.0 Governance Readiness)
pub struct ChaosService {
    http: Arc<OlympusHttpClient>,
}

/// Configuration for a chaos fault injection experiment.
#[derive(Default)]
pub struct FaultConfig<'a> {
    /// Fault type: latency, cpu_stress, memory_pressure, network_partition, disk_io, pod_kill.
    pub fault_type: &'a str,
    /// Target service name.
    pub target_service: &'a str,
    /// Duration in seconds.
    pub duration_secs: u32,
    /// Blast radius (0.0 to 1.0 — fraction of instances affected).
    pub blast_radius: f64,
    /// Whether to require approval before execution.
    pub requires_approval: bool,
}

/// Configuration for a DR drill.
#[derive(Default)]
pub struct DrDrillConfig<'a> {
    /// Drill type: region_failover, zone_failover, service_failover, database_failover.
    pub drill_type: &'a str,
    /// Target region or zone.
    pub target: &'a str,
    /// Maximum drill duration in seconds before auto-abort.
    pub max_duration_secs: u32,
}

impl ChaosService {
    /// Creates a new ChaosService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    // ─── Chaos Queue (#2938) ──────────────────────────────────────

    /// Enqueue a fault injection experiment for later execution.
    pub async fn enqueue_fault(&self, config: FaultConfig<'_>) -> Result<Value> {
        let body = json!({
            "fault_type": config.fault_type,
            "target_service": config.target_service,
            "duration_secs": config.duration_secs,
            "blast_radius": config.blast_radius,
            "requires_approval": config.requires_approval,
        });
        self.http.post("/chaos/queue/enqueue", &body).await
    }

    /// List pending fault injection experiments.
    pub async fn list_pending(&self) -> Result<Value> {
        self.http.get("/chaos/queue/pending").await
    }

    /// Execute the next pending fault injection experiment.
    pub async fn execute_next(&self) -> Result<Value> {
        self.http.post("/chaos/queue/execute", &json!({})).await
    }

    /// Get experiment results history.
    pub async fn experiment_results(&self, limit: Option<u32>) -> Result<Value> {
        let mut path = "/chaos/queue/results".to_string();
        if let Some(l) = limit {
            path.push_str(&format!("?limit={}", l));
        }
        self.http.get(&path).await
    }

    // ─── DR Drills (#2939) ────────────────────────────────────────

    /// Start a disaster recovery drill.
    pub async fn start_dr_drill(&self, config: DrDrillConfig<'_>) -> Result<Value> {
        let body = json!({
            "drill_type": config.drill_type,
            "target": config.target,
            "max_duration_secs": config.max_duration_secs,
        });
        self.http.post("/chaos/dr-drills/start", &body).await
    }

    /// List currently active DR drills.
    pub async fn list_active_drills(&self) -> Result<Value> {
        self.http.get("/chaos/dr-drills/active").await
    }

    /// Stop a running DR drill.
    pub async fn stop_drill(&self, drill_id: &str) -> Result<Value> {
        let body = json!({ "drill_id": drill_id });
        self.http.post("/chaos/dr-drills/stop", &body).await
    }

    /// Get a DR drill report with RTO/RPO analysis.
    pub async fn drill_report(&self, drill_id: &str) -> Result<Value> {
        self.http
            .get(&format!("/chaos/dr-drills/report?drill_id={}", drill_id))
            .await
    }

    // ─── Gameday Execution (#2940) ────────────────────────────────

    /// Create a gameday plan.
    pub async fn create_gameday(&self, name: &str, scenarios: Value) -> Result<Value> {
        let body = json!({
            "name": name,
            "scenarios": scenarios,
        });
        self.http.post("/chaos/gamedays/create", &body).await
    }

    /// List all gameday plans.
    pub async fn list_gamedays(&self) -> Result<Value> {
        self.http.get("/chaos/gamedays").await
    }

    /// Execute a gameday plan.
    pub async fn execute_gameday(&self, gameday_id: &str) -> Result<Value> {
        let body = json!({ "gameday_id": gameday_id });
        self.http.post("/chaos/gamedays/execute", &body).await
    }

    /// Get a gameday post-mortem report.
    pub async fn gameday_report(&self, gameday_id: &str) -> Result<Value> {
        self.http
            .get(&format!("/chaos/gamedays/report?gameday_id={}", gameday_id))
            .await
    }
}
