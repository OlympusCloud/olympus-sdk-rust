use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Creator platform service for posts, media, and AI content generation.
///
/// Wraps the Olympus Creator service (Rust, port 8004) via the Go API Gateway.
/// Routes: `/api/v1/posts/*`, `/creator/*`.
pub struct CreatorService {
    http: Arc<OlympusHttpClient>,
}

impl CreatorService {
    /// Creates a new CreatorService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Lists posts with optional pagination.
    pub async fn list_posts(&self, page: Option<u32>, page_size: Option<u32>) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(p) = page {
            query.push(("page", p.to_string()));
        }
        if let Some(ps) = page_size {
            query.push(("page_size", ps.to_string()));
        }

        if query.is_empty() {
            self.http.get("/api/v1/posts").await
        } else {
            let pairs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            self.http.get_with_query("/api/v1/posts", &pairs).await
        }
    }

    /// Creates a new post.
    pub async fn create_post(&self, post: Value) -> Result<Value> {
        self.http.post("/api/v1/posts", &post).await
    }

    /// Generates AI content from a prompt and content type.
    pub async fn generate_content(
        &self,
        prompt: &str,
        content_type: Option<&str>,
    ) -> Result<Value> {
        let mut body = json!({
            "prompt": prompt,
        });
        if let Some(ct) = content_type {
            body["content_type"] = Value::String(ct.to_string());
        }
        self.http.post("/creator/ai/generate", &body).await
    }
}
