use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::{OlympusError, Result};
use crate::http::OlympusHttpClient;

/// AI inference, agent orchestration, and NLP service.
///
/// Wraps the Olympus AI Gateway (Python) via the Go API Gateway.
/// Routes: `/ai/*`, `/agent/*`.
pub struct AiService {
    http: Arc<OlympusHttpClient>,
}

/// Options for a single-turn AI query.
///
/// `required_capabilities` activates capability-based routing (#2919).
/// Non-text values bypass the text tier selector and route to the cheapest
/// model in the Ether catalog matching the capabilities. Values: text,
/// vision, audio_in, audio_out, audio_live, video_in, video_generation,
/// video_live, image_generation, embedding, reasoning, agentic_coding,
/// world_model, robotics_control, medical_specialist, legal_specialist,
/// financial_specialist, scientific_specialist, function_calling,
/// structured_output, long_context.
#[derive(Default)]
pub struct QueryOptions<'a> {
    pub tier: Option<&'a str>,
    pub context: Option<Value>,
    pub required_capabilities: Option<Vec<String>>,
}

/// Options for image generation.
#[derive(Default)]
pub struct GenerateImageOptions<'a> {
    pub preferred_provider: Option<&'a str>,
}

/// Options for video generation.
#[derive(Default)]
pub struct GenerateVideoOptions<'a> {
    pub duration_seconds: Option<u32>,
    pub preferred_provider: Option<&'a str>,
}

impl AiService {
    /// Creates a new AiService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Sends a single-turn prompt to the AI gateway (simple variant).
    pub async fn query(&self, prompt: &str, tier: Option<&str>) -> Result<Value> {
        self.query_with_options(
            prompt,
            QueryOptions {
                tier,
                ..Default::default()
            },
        )
        .await
    }

    /// Sends a single-turn prompt with full options including capability routing.
    pub async fn query_with_options(&self, prompt: &str, opts: QueryOptions<'_>) -> Result<Value> {
        let mut body = json!({
            "messages": [{"role": "user", "content": prompt}],
        });
        if let Some(t) = opts.tier {
            body["tier"] = Value::String(t.to_string());
        }
        if let Some(ctx) = opts.context {
            body["context"] = ctx;
        }
        if let Some(caps) = opts.required_capabilities {
            body["required_capabilities"] =
                Value::Array(caps.into_iter().map(Value::String).collect());
        }
        self.http.post("/ai/chat", &body).await
    }

    /// Generate an image from a text prompt using the cheapest matching provider
    /// in the Ether catalog (Flux Schnell free, DALL-E 3, Imagen 4, etc.).
    ///
    /// Returns a JSON map with `image_url` or `image_b64`.
    pub async fn generate_image(
        &self,
        prompt: &str,
        opts: GenerateImageOptions<'_>,
    ) -> Result<Value> {
        let mut body = json!({
            "messages": [{"role": "user", "content": prompt}],
            "required_capabilities": ["image_generation"],
        });
        if let Some(p) = opts.preferred_provider {
            body["preferred_provider"] = Value::String(p.to_string());
        }
        self.http.post("/ai/chat", &body).await
    }

    /// Generate a video from a text prompt (Veo / Kling / Pika / Luma / Hailuo).
    /// Returns async job reference — poll `/ai/video-jobs/{id}` for completion.
    pub async fn generate_video(
        &self,
        prompt: &str,
        opts: GenerateVideoOptions<'_>,
    ) -> Result<Value> {
        let mut body = json!({
            "messages": [{"role": "user", "content": prompt}],
            "required_capabilities": ["video_generation"],
        });
        if let Some(d) = opts.duration_seconds {
            body["duration_seconds"] = Value::Number(d.into());
        }
        if let Some(p) = opts.preferred_provider {
            body["preferred_provider"] = Value::String(p.to_string());
        }
        self.http.post("/ai/chat", &body).await
    }

    /// Call a vertical specialist model (medical/legal/financial/scientific).
    /// Routes to Med-Gemini, Harvey, BloombergGPT, ESM-3, etc.
    pub async fn specialist_query(
        &self,
        prompt: &str,
        specialty: &str,
        context: Option<&str>,
    ) -> Result<Value> {
        let capability = match specialty {
            "medical" => "medical_specialist",
            "legal" => "legal_specialist",
            "financial" => "financial_specialist",
            "scientific" => "scientific_specialist",
            _ => {
                return Err(OlympusError::Config(format!(
                    "unknown specialty '{}' (must be medical/legal/financial/scientific)",
                    specialty
                )));
            }
        };
        let mut messages = Vec::new();
        if let Some(ctx) = context {
            messages.push(json!({"role": "system", "content": ctx}));
        }
        messages.push(json!({"role": "user", "content": prompt}));
        let body = json!({
            "messages": messages,
            "required_capabilities": ["reasoning", capability],
        });
        self.http.post("/ai/chat", &body).await
    }

    /// Invokes a LangGraph agent synchronously.
    pub async fn invoke(&self, agent: &str, task: &str, params: Option<Value>) -> Result<Value> {
        let mut body = json!({
            "agent": agent,
            "task": task,
        });
        if let Some(p) = params {
            body["params"] = p;
        }
        self.http.post("/agent/invoke", &body).await
    }

    /// Sends a multi-turn chat completion request.
    pub async fn chat(&self, messages: Value, model: Option<&str>) -> Result<Value> {
        let mut body = json!({
            "messages": messages,
        });
        if let Some(m) = model {
            body["model"] = Value::String(m.to_string());
        }
        self.http.post("/ai/chat", &body).await
    }
}
