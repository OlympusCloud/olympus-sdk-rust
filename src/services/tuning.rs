use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// AI tuning jobs, synthetic persona generation, and chaos audio simulation.
///
/// Covers model fine-tuning lifecycle, synthetic persona generation for
/// load testing, and audio noise simulation for chaos testing voice pipelines.
///
/// Routes: `/v1/tuning/*`, `/v1/personas/*`, `/v1/chaos/audio/*`.
pub struct TuningService {
    http: Arc<OlympusHttpClient>,
}

impl TuningService {
    /// Creates a new TuningService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    // ─── Tuning Jobs ─────────────────────────────────────────────

    /// Create a new tuning job.
    ///
    /// `job_type` identifies the tuning strategy (e.g. `lora`, `full`,
    /// `distillation`, `rlhf`). `parameters` carries model-specific config
    /// such as `base_model`, `dataset_id`, `epochs`, `learning_rate`, etc.
    pub async fn create_tuning_job(
        &self,
        job_type: &str,
        parameters: &Value,
    ) -> Result<Value> {
        let body = json!({
            "job_type": job_type,
            "parameters": parameters,
        });
        self.http.post("/v1/tuning/jobs", &body).await
    }

    /// List tuning jobs with optional filters.
    ///
    /// `status` filters by job status (`queued`, `running`, `completed`,
    /// `failed`, `cancelled`). `limit` caps the number of results.
    pub async fn list_tuning_jobs(
        &self,
        status: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Value> {
        let mut params = Vec::new();
        if let Some(s) = status {
            params.push(("status", s.to_string()));
        }
        if let Some(l) = limit {
            params.push(("limit", l.to_string()));
        }
        if params.is_empty() {
            self.http.get("/v1/tuning/jobs").await
        } else {
            let query: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
            self.http.get_with_query("/v1/tuning/jobs", &query).await
        }
    }

    /// Get details for a single tuning job.
    pub async fn get_tuning_job(&self, job_id: &str) -> Result<Value> {
        self.http
            .get(&format!("/v1/tuning/jobs/{}", job_id))
            .await
    }

    /// Cancel a running or queued tuning job.
    pub async fn cancel_tuning_job(&self, job_id: &str) -> Result<Value> {
        self.http
            .post(&format!("/v1/tuning/jobs/{}/cancel", job_id), &json!({}))
            .await
    }

    /// Get the results of a completed tuning job.
    ///
    /// Returns metrics, evaluation scores, and the output model artifact reference.
    pub async fn get_tuning_results(&self, job_id: &str) -> Result<Value> {
        self.http
            .get(&format!("/v1/tuning/jobs/{}/results", job_id))
            .await
    }

    // ─── Synthetic Persona Generation ────────────────────────────

    /// Generate a single synthetic persona for load/QA testing.
    ///
    /// `config` specifies persona attributes such as `locale`, `accent`,
    /// `speaking_style`, `vocabulary_level`, `noise_profile`, and
    /// `intent_distribution`.
    pub async fn generate_persona(&self, config: &Value) -> Result<Value> {
        self.http.post("/v1/personas/generate", config).await
    }

    /// Generate a batch of synthetic personas.
    ///
    /// `count` is the number of personas to generate (1-1000).
    /// `distribution` defines the statistical distribution of persona
    /// characteristics.
    pub async fn generate_persona_batch(
        &self,
        count: u32,
        distribution: &Value,
    ) -> Result<Value> {
        let body = json!({
            "count": count,
            "distribution": distribution,
        });
        self.http.post("/v1/personas/batch", &body).await
    }

    // ─── Chaos Audio Simulation ──────────────────────────────────

    /// Simulate environmental noise on an audio sample for chaos testing.
    ///
    /// `audio_base64` is base64-encoded audio (WAV or MP3).
    /// `noise_type` selects the noise profile: `background_chatter`,
    /// `drive_thru_wind`, `kitchen_noise`, `traffic`, `rain`, `static`,
    /// `crowd`, or `machinery`.
    /// `intensity` is a 0.0-1.0 float controlling noise level.
    pub async fn simulate_noise(
        &self,
        audio_base64: &str,
        noise_type: &str,
        intensity: f64,
    ) -> Result<Value> {
        let body = json!({
            "audio_base64": audio_base64,
            "noise_type": noise_type,
            "intensity": intensity,
        });
        self.http.post("/v1/chaos/audio/simulate", &body).await
    }
}
