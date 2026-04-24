use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Voice marketplace reviews service (#3463).
///
/// Adds review parity with the marketplace-app reviews API for the voice
/// marketplace catalog (curated voices and personas).
///
/// Routes: `/voice/marketplace/voices/*/reviews`,
/// `/voice/marketplace/voices/reviews/*`.
pub struct VoiceMarketplaceService {
    http: Arc<OlympusHttpClient>,
}

impl VoiceMarketplaceService {
    /// Creates a new VoiceMarketplaceService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Lists published reviews for a marketplace voice with average rating.
    ///
    /// Returns the raw `{reviews, total, average, limit, offset}` payload.
    /// Each review's `author_tenant_id` is a 16-char HMAC hash (stable per
    /// tenant for de-duplication, one-way so the underlying tenant is not
    /// recoverable).
    pub async fn list_reviews(
        &self,
        voice_id: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Value> {
        let path = format!("/voice/marketplace/voices/{}/reviews", voice_id);
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(l) = limit {
            query.push(("limit", l.to_string()));
        }
        if let Some(o) = offset {
            query.push(("offset", o.to_string()));
        }
        if query.is_empty() {
            self.http.get(&path).await
        } else {
            let q: Vec<(&str, &str)> =
                query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            self.http.get_with_query(&path, &q).await
        }
    }

    /// Submits a 1..5 star review for a marketplace voice.
    ///
    /// One review per (user, voice) — duplicate submissions return 409.
    pub async fn submit_review(
        &self,
        voice_id: &str,
        rating: u32,
        text: &str,
    ) -> Result<Value> {
        let body = json!({ "rating": rating, "text": text });
        self.http
            .post(
                &format!("/voice/marketplace/voices/{}/reviews", voice_id),
                &body,
            )
            .await
    }

    /// Soft-deletes the caller's own review.
    pub async fn delete_review(&self, review_id: &str) -> Result<Value> {
        self.http
            .delete(&format!("/voice/marketplace/voices/reviews/{}", review_id))
            .await
    }
}
