use std::sync::Arc;

use serde_json::Value;

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Enterprise Context service -- Company 360 for any AI agent (#2993).
///
/// Assembles complete tenant data (brand, locations, menu, specials, FAQs,
/// upsells, inventory, caller profile, graph relationships) in a single
/// response. Cached for 5 minutes per (tenant_id, location_id).
///
/// Routes: `/enterprise-context/*` (proxied via Go Gateway to Python).
pub struct EnterpriseContextService {
    http: Arc<OlympusHttpClient>,
}

impl EnterpriseContextService {
    /// Creates a new EnterpriseContextService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Fetches the full Company 360 context for a given tenant and location.
    ///
    /// * `tenant_id` -- The tenant whose context to assemble.
    /// * `location_id` -- The specific location, or `None` for the default.
    /// * `agent_type` -- One of `"voice"`, `"chat"`, `"pantheon"`, `"workflow"`.
    /// * `caller_phone` -- Optional caller phone number for profile lookup.
    pub async fn get(
        &self,
        tenant_id: &str,
        location_id: Option<&str>,
        agent_type: &str,
        caller_phone: Option<&str>,
    ) -> Result<Value> {
        let path = match location_id {
            Some(loc) => format!("/enterprise-context/{}/{}", tenant_id, loc),
            None => format!("/enterprise-context/{}", tenant_id),
        };

        let mut query: Vec<(&str, &str)> = vec![("agent_type", agent_type)];
        if let Some(phone) = caller_phone {
            query.push(("caller_phone", phone));
        }

        self.http.get_with_query(&path, &query).await
    }
}
