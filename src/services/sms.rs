//! SmsService — outbound SMS + delivery status via the CPaaS abstraction.
//!
//! Two route families:
//! - `/voice/sms/*`       — voice-platform SMS (send, conversations)
//! - `/cpaas/messages/*`  — unified CPaaS messaging (SMS, MMS, status)
//!
//! The CPaaS layer is Telnyx-primary / Twilio-fallback (#2951) and handles
//! failover transparently.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Filter options for [`SmsService::get_conversations`].
#[derive(Default, Debug, Clone, Copy)]
pub struct GetConversationsOptions {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Parameters for [`SmsService::send_via_cpaas`].
#[derive(Debug, Clone)]
pub struct SendViaCpaasRequest<'a> {
    /// Sender E.164 phone number.
    pub from: &'a str,
    /// Destination E.164 phone number.
    pub to: &'a str,
    /// Message body.
    pub body: &'a str,
    /// Optional delivery-receipt webhook URL.
    pub webhook_url: Option<&'a str>,
}

/// SMS messaging — send outbound SMS, retrieve conversation history, and
/// query message delivery status.
pub struct SmsService {
    http: Arc<OlympusHttpClient>,
}

impl SmsService {
    /// Creates a new SmsService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    // -----------------------------------------------------------------------
    // Voice SMS (tenant-scoped)
    // -----------------------------------------------------------------------

    /// Send an outbound SMS through a voice agent config.
    ///
    /// * `config_id` — identifies the voice agent config (and its assigned
    ///   phone number).
    /// * `to`        — E.164 destination.
    /// * `body`      — message text.
    pub async fn send(&self, config_id: &str, to: &str, body: &str) -> Result<Value> {
        let payload = json!({
            "config_id": config_id,
            "to": to,
            "body": body,
        });
        self.http.post("/voice/sms/send", &payload).await
    }

    /// List threaded SMS conversations for a phone number.
    pub async fn get_conversations(
        &self,
        phone: &str,
        opts: GetConversationsOptions,
    ) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(l) = opts.limit {
            query.push(("limit", l.to_string()));
        }
        if let Some(o) = opts.offset {
            query.push(("offset", o.to_string()));
        }
        let path = format!("/voice/sms/conversations/{}", urlencoding::encode(phone));
        if query.is_empty() {
            self.http.get(&path).await
        } else {
            let query_refs: Vec<(&str, &str)> =
                query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            self.http.get_with_query(&path, &query_refs).await
        }
    }

    // -----------------------------------------------------------------------
    // CPaaS Messaging (provider-abstracted)
    // -----------------------------------------------------------------------

    /// Send an SMS via the unified CPaaS layer (Telnyx primary, Twilio
    /// fallback). Returns the message resource with provider-assigned ID
    /// and delivery status.
    pub async fn send_via_cpaas(&self, req: SendViaCpaasRequest<'_>) -> Result<Value> {
        let mut body = json!({
            "from": req.from,
            "to": req.to,
            "body": req.body,
        });
        if let Some(url) = req.webhook_url {
            body["webhook_url"] = Value::String(url.to_string());
        }
        self.http.post("/cpaas/messages/sms", &body).await
    }

    /// Get the delivery status and metadata of a sent message.
    pub async fn get_status(&self, message_id: &str) -> Result<Value> {
        let path = format!("/cpaas/messages/{}", urlencoding::encode(message_id));
        self.http.get(&path).await
    }
}
