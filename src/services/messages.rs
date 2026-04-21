use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Message queue with department routing service (#2997).
///
/// AI agents route messages to business departments (manager, catering,
/// sales, lost-and-found, reservations) when they cannot fully handle a
/// request. Notification dispatch via Twilio SMS + SendGrid email on create.
///
/// Routes: `/messages/*` (proxied via Go Gateway to Python).
pub struct MessagesService {
    http: Arc<OlympusHttpClient>,
}

/// Filter options for listing messages.
#[derive(Default)]
pub struct ListMessagesOptions<'a> {
    /// Filter by department (e.g. "manager", "catering", "sales").
    pub department: Option<&'a str>,
    /// Filter by status: "pending", "read", or "resolved".
    pub status: Option<&'a str>,
    /// Filter by location ID.
    pub location_id: Option<&'a str>,
    /// Maximum number of results (default 50, max 200).
    pub limit: Option<u32>,
}

/// Parameters for creating a message in the queue.
#[derive(Default)]
pub struct QueueMessageRequest<'a> {
    /// Target department (manager, catering, sales, lost_and_found, reservations, general).
    pub department: &'a str,
    /// The message body.
    pub message: &'a str,
    /// Optional caller phone number.
    pub caller_phone: Option<&'a str>,
    /// Optional caller name.
    pub caller_name: Option<&'a str>,
    /// Optional location ID.
    pub location_id: Option<&'a str>,
    /// Priority: "urgent", "high", "normal", "low" (default "normal").
    pub priority: Option<&'a str>,
    /// Origin of the message (default "voice").
    pub source: Option<&'a str>,
}

impl MessagesService {
    /// Creates a new MessagesService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Creates a message in the queue and triggers notification dispatch.
    pub async fn queue(&self, req: QueueMessageRequest<'_>) -> Result<Value> {
        let mut body = json!({
            "department": req.department,
            "message": req.message,
        });
        if let Some(phone) = req.caller_phone {
            body["caller_phone"] = Value::String(phone.to_string());
        }
        if let Some(name) = req.caller_name {
            body["caller_name"] = Value::String(name.to_string());
        }
        if let Some(loc) = req.location_id {
            body["location_id"] = Value::String(loc.to_string());
        }
        if let Some(p) = req.priority {
            body["priority"] = Value::String(p.to_string());
        }
        if let Some(s) = req.source {
            body["source"] = Value::String(s.to_string());
        }
        self.http.post("/messages/queue", &body).await
    }

    /// Lists messages with optional filters.
    pub async fn list(&self, opts: ListMessagesOptions<'_>) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(dept) = opts.department {
            query.push(("department", dept.to_string()));
        }
        if let Some(s) = opts.status {
            query.push(("status", s.to_string()));
        }
        if let Some(loc) = opts.location_id {
            query.push(("location_id", loc.to_string()));
        }
        if let Some(l) = opts.limit {
            query.push(("limit", l.to_string()));
        }
        let query_refs: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();

        if query_refs.is_empty() {
            self.http.get("/messages").await
        } else {
            self.http.get_with_query("/messages", &query_refs).await
        }
    }

    /// Updates message status or assignment.
    ///
    /// * `msg_id` -- The message ID.
    /// * `status` -- Optional new status: "pending", "read", or "resolved".
    /// * `assigned_to` -- Optional user to assign the message to.
    pub async fn update(
        &self,
        msg_id: &str,
        status: Option<&str>,
        assigned_to: Option<&str>,
    ) -> Result<Value> {
        let mut body = json!({});
        if let Some(s) = status {
            body["status"] = Value::String(s.to_string());
        }
        if let Some(a) = assigned_to {
            body["assigned_to"] = Value::String(a.to_string());
        }
        self.http
            .patch(&format!("/messages/{}", msg_id), &body)
            .await
    }

    /// Resolves a message by setting its status to "resolved".
    pub async fn resolve(&self, msg_id: &str) -> Result<Value> {
        self.update(msg_id, Some("resolved"), None).await
    }

    /// Lists configured departments with routing rules.
    pub async fn list_departments(&self) -> Result<Value> {
        self.http.get("/messages/departments").await
    }

    /// Configures routing for a department.
    ///
    /// * `department` -- The department to configure.
    /// * `notification_channels` -- Notification channels (e.g. `["sms"]`).
    /// * `recipients` -- List of recipient objects.
    /// * `escalation_after_minutes` -- Minutes before escalation (default 15).
    /// * `is_active` -- Whether the department is active.
    pub async fn configure_department(&self, department: &str, config: Value) -> Result<Value> {
        self.http
            .put(&format!("/messages/departments/{}", department), &config)
            .await
    }
}
