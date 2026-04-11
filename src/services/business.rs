use std::sync::Arc;

use serde_json::Value;

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Business data access service for revenue dashboards and AI insights.
///
/// Wraps business data endpoints used by consumer apps.
/// Routes: `/business/*`.
pub struct BusinessService {
    http: Arc<OlympusHttpClient>,
}

impl BusinessService {
    /// Creates a new BusinessService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Returns revenue summary across today/week/month/year periods.
    pub async fn get_revenue_summary(&self) -> Result<Value> {
        self.http.get("/business/revenue/summary").await
    }

    /// Returns AI-generated business insights, optionally filtered by category.
    pub async fn get_insights(&self, category: Option<&str>) -> Result<Value> {
        match category {
            Some(c) => {
                self.http
                    .get_with_query("/business/insights", &[("category", c)])
                    .await
            }
            None => self.http.get("/business/insights").await,
        }
    }
}
