use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// AI inference, agent orchestration, and NLP service.
///
/// Wraps the Olympus AI Gateway (Python) via the Go API Gateway.
/// Routes: `/ai/*`, `/agent/*`.
pub struct AiService {
    http: Arc<OlympusHttpClient>,
}

impl AiService {
    /// Creates a new AiService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Sends a single-turn prompt to the AI gateway.
    pub async fn query(&self, prompt: &str, tier: Option<&str>) -> Result<Value> {
        let mut body = json!({
            "messages": [{"role": "user", "content": prompt}],
        });
        if let Some(t) = tier {
            body["tier"] = Value::String(t.to_string());
        }
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
