use std::sync::Arc;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Voice AI platform — agent configs, conversations, campaigns, phone numbers,
/// marketplace voices, calls, speaker profiles, analytics, caller profiles,
/// voicemail, ambiance beds, personas, templates, workflow templates,
/// escalation / business-hours config, and the edge voice pipeline (STT →
/// Ether → TTS via CF Containers).
///
/// Routes: `/voice-agents/*`, `/voice/phone-numbers/*`, `/voice/marketplace/*`,
/// `/voice/calls/*`, `/voice/speaker/*`, `/voice/profiles/*`,
/// `/voice/process` (edge pipeline REST), `/ws/voice` (edge pipeline WebSocket),
/// `/caller-profiles/*`, `/ether/voice/agents/*`.
///
/// Also includes the V2-005 cascade resolver endpoints
/// (`/voice-agents/configs/{id}/effective-config` + `.../pipeline`) which are
/// the only endpoints with a typed response shape — everything else returns
/// the raw envelope as [`serde_json::Value`] for forward-compatibility.
pub struct VoiceService {
    http: Arc<OlympusHttpClient>,
}

// ---------------------------------------------------------------------------
// V2-005 cascade-resolver typed models (Wave 1).
// ---------------------------------------------------------------------------

/// A single rung of the voice-defaults cascade.
///
/// Each rung (platform, app, tenant, agent) carries whatever subset of
/// configuration was set at that scope. Nil on the wire becomes `None`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VoiceDefaultsRung {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipeline: Option<String>,
    #[serde(
        default,
        rename = "pipelineConfig",
        skip_serializing_if = "Option::is_none"
    )]
    pub pipeline_config: Option<Map<String, Value>>,
    #[serde(
        default,
        rename = "tierOverride",
        skip_serializing_if = "Option::is_none"
    )]
    pub tier_override: Option<String>,
    #[serde(default, rename = "logLevel", skip_serializing_if = "Option::is_none")]
    pub log_level: Option<String>,
    #[serde(
        default,
        rename = "debugTranscriptsEnabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub debug_transcripts_enabled: Option<bool>,
    #[serde(
        default,
        rename = "v2ShadowEnabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub v2_shadow_enabled: Option<bool>,
    #[serde(
        default,
        rename = "v2PrimaryEnabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub v2_primary_enabled: Option<bool>,
}

/// The four rungs of the voice-defaults cascade in ascending-specificity
/// order: platform → app → tenant → agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VoiceDefaultsCascade {
    #[serde(default)]
    pub platform: Option<VoiceDefaultsRung>,
    #[serde(default)]
    pub app: Option<VoiceDefaultsRung>,
    #[serde(default)]
    pub tenant: Option<VoiceDefaultsRung>,
    #[serde(default)]
    pub agent: Option<VoiceDefaultsRung>,
}

/// Full merged view returned by
/// `GET /api/v1/voice-agents/configs/{id}/effective-config` (V2-005).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceEffectiveConfig {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    #[serde(rename = "tenantId")]
    pub tenant_id: String,
    pub pipeline: String,
    #[serde(rename = "pipelineConfig", default)]
    pub pipeline_config: Map<String, Value>,
    #[serde(
        rename = "tierOverride",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub tier_override: Option<String>,
    #[serde(rename = "logLevel")]
    pub log_level: String,
    #[serde(rename = "debugTranscriptsEnabled")]
    pub debug_transcripts_enabled: bool,
    #[serde(rename = "v2ShadowEnabled")]
    pub v2_shadow_enabled: bool,
    #[serde(rename = "v2PrimaryEnabled")]
    pub v2_primary_enabled: bool,

    // Telephony bindings (populated if an agent has an assigned phone number).
    #[serde(
        rename = "telephonyProvider",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub telephony_provider: Option<String>,
    #[serde(
        rename = "providerAccountRef",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub provider_account_ref: Option<String>,
    #[serde(
        rename = "preferredCodec",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub preferred_codec: Option<String>,
    #[serde(
        rename = "preferredSampleRate",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub preferred_sample_rate: Option<i64>,
    #[serde(
        rename = "hdAudioEnabled",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub hd_audio_enabled: Option<bool>,
    #[serde(
        rename = "webhookPathOverride",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub webhook_path_override: Option<String>,
    #[serde(rename = "v2Routed", default, skip_serializing_if = "Option::is_none")]
    pub v2_routed: Option<bool>,

    #[serde(rename = "voiceDefaults", default)]
    pub voice_defaults: VoiceDefaultsCascade,
    #[serde(rename = "resolvedAt")]
    pub resolved_at: String,
    #[serde(rename = "cascadeVersion")]
    pub cascade_version: String,
}

/// Pipeline-only view returned by
/// `GET /api/v1/voice-agents/configs/{id}/pipeline` (V2-005).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoicePipeline {
    #[serde(rename = "agentId")]
    pub agent_id: String,
    pub pipeline: String,
    #[serde(rename = "pipelineConfig", default)]
    pub pipeline_config: Map<String, Value>,
    #[serde(rename = "resolvedAt")]
    pub resolved_at: String,
    #[serde(rename = "cascadeVersion")]
    pub cascade_version: String,
}

// ---------------------------------------------------------------------------
// Filter / builder options (Wave 2 — dart parity).
// ---------------------------------------------------------------------------

/// Pagination options for `list_configs`, `list_conversations`, etc.
#[derive(Default, Debug, Clone, Copy)]
pub struct PageOptions<'a> {
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub tenant_id: Option<&'a str>,
}

/// Filter options for [`VoiceService::list_personas`].
#[derive(Default, Debug, Clone, Copy)]
pub struct ListPersonasOptions<'a> {
    pub category: Option<&'a str>,
    pub industry: Option<&'a str>,
    pub premium_only: Option<bool>,
}

/// Filter options for [`VoiceService::list_gemini_voices`].
#[derive(Default, Debug, Clone, Copy)]
pub struct ListGeminiVoicesOptions<'a> {
    pub language: Option<&'a str>,
}

/// Filter options for [`VoiceService::list_voices`] (marketplace).
#[derive(Default, Debug, Clone, Copy)]
pub struct ListVoicesOptions<'a> {
    pub language: Option<&'a str>,
    pub gender: Option<&'a str>,
    pub limit: Option<u32>,
}

/// Filter options for [`VoiceService::search_numbers`].
#[derive(Default, Debug, Clone, Copy)]
pub struct SearchNumbersOptions<'a> {
    pub area_code: Option<&'a str>,
    pub contains: Option<&'a str>,
    pub country: Option<&'a str>,
    pub limit: Option<u32>,
}

/// Filter options for [`VoiceService::list_conversations`].
#[derive(Default, Debug, Clone, Copy)]
pub struct ListConversationsOptions<'a> {
    pub agent_id: Option<&'a str>,
    pub status: Option<&'a str>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub tenant_id: Option<&'a str>,
}

/// Filter options for [`VoiceService::list_messages`].
#[derive(Default, Debug, Clone, Copy)]
pub struct ListVoiceMessagesOptions<'a> {
    pub department: Option<&'a str>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

/// Filter options for [`VoiceService::list_voicemails`].
#[derive(Default, Debug, Clone, Copy)]
pub struct ListVoicemailsOptions<'a> {
    pub caller_phone: Option<&'a str>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

/// Filter options for [`VoiceService::get_analytics`].
#[derive(Default, Debug, Clone, Copy)]
pub struct GetAnalyticsOptions<'a> {
    pub agent_id: Option<&'a str>,
    pub from: Option<&'a str>,
    pub to: Option<&'a str>,
}

/// Filter options for [`VoiceService::list_caller_profiles`].
#[derive(Debug, Clone, Copy)]
pub struct ListCallerProfilesOptions {
    pub limit: u32,
    pub offset: u32,
}

impl Default for ListCallerProfilesOptions {
    fn default() -> Self {
        Self {
            limit: 50,
            offset: 0,
        }
    }
}

/// Parameters for [`VoiceService::create_agent`].
#[derive(Default, Debug, Clone)]
pub struct CreateAgentRequest<'a> {
    pub from_template_id: Option<&'a str>,
    pub name: Option<&'a str>,
    pub voice_id: Option<&'a str>,
    pub persona: Option<&'a str>,
    pub greeting: Option<&'a str>,
    pub phone_number: Option<&'a str>,
    pub location_id: Option<&'a str>,
    pub ambiance_config: Option<Value>,
    pub voice_overrides: Option<Value>,
    pub business_hours: Option<Value>,
    pub escalation_rules: Option<Value>,
}

/// Parameters for [`VoiceService::update_agent`].
#[derive(Default, Debug, Clone)]
pub struct UpdateAgentRequest<'a> {
    pub name: Option<&'a str>,
    pub voice_id: Option<&'a str>,
    pub persona: Option<&'a str>,
    pub greeting: Option<&'a str>,
    pub ambiance_config: Option<Value>,
    pub voice_overrides: Option<Value>,
    pub business_hours: Option<Value>,
    pub escalation_rules: Option<Value>,
    pub is_active: Option<bool>,
}

/// Parameters for [`VoiceService::clone_agent`].
#[derive(Default, Debug, Clone, Copy)]
pub struct CloneAgentRequest<'a> {
    pub new_name: Option<&'a str>,
    pub phone_number: Option<&'a str>,
    pub location_id: Option<&'a str>,
}

/// Parameters for [`VoiceService::preview_agent_voice`].
#[derive(Debug, Clone)]
pub struct PreviewAgentVoiceRequest<'a> {
    pub sample_text: &'a str,
    pub voice_id: Option<&'a str>,
    pub voice_overrides: Option<Value>,
}

/// Parameters for [`VoiceService::instantiate_agent_template`].
#[derive(Debug, Clone, Copy)]
pub struct InstantiateAgentTemplateRequest<'a> {
    pub name: &'a str,
    pub phone_number: Option<&'a str>,
    pub location_id: Option<&'a str>,
}

/// Parameters for [`VoiceService::publish_agent_as_template`].
#[derive(Debug, Clone, Copy)]
pub struct PublishAgentAsTemplateRequest<'a> {
    pub scope: &'a str,
    pub description: Option<&'a str>,
}

/// Parameters for [`VoiceService::upload_ambiance_bed`].
#[derive(Debug, Clone)]
pub struct UploadAmbianceBedRequest<'a> {
    pub name: &'a str,
    pub audio_bytes: &'a [u8],
    pub time_of_day: Option<&'a str>,
    pub description: Option<&'a str>,
}

/// Parameters for [`VoiceService::update_agent_ambiance`].
#[derive(Default, Debug, Clone)]
pub struct UpdateAgentAmbianceRequest<'a> {
    pub enabled: Option<bool>,
    pub intensity: Option<f64>,
    pub default_r2_key: Option<&'a str>,
    pub time_of_day_variants: Option<Map<String, Value>>,
}

/// Parameters for [`VoiceService::update_agent_voice_overrides`].
#[derive(Default, Debug, Clone, Copy)]
pub struct UpdateAgentVoiceOverridesRequest<'a> {
    pub pitch: Option<f64>,
    pub speed: Option<f64>,
    pub warmth: Option<f64>,
    pub regional_dialect: Option<&'a str>,
}

/// Parameters for [`VoiceService::provision_agent`].
#[derive(Debug, Clone)]
pub struct ProvisionAgentRequest<'a> {
    pub agent_id: &'a str,
    pub tenant_id: &'a str,
    pub voice_name: &'a str,
    pub profile: Value,
    pub greeting_text: &'a str,
}

/// Parameters for [`VoiceService::process_audio`].
#[derive(Debug, Clone)]
pub struct ProcessAudioRequest<'a> {
    pub audio_bytes: &'a [u8],
    pub language: Option<&'a str>,
    pub agent_id: Option<&'a str>,
    pub voice_id: Option<&'a str>,
    pub session_id: Option<&'a str>,
}

impl VoiceService {
    /// Creates a new VoiceService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    // -----------------------------------------------------------------------
    // Agents
    // -----------------------------------------------------------------------

    /// List all voice agent configurations.
    pub async fn list_configs(&self, opts: PageOptions<'_>) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(p) = opts.page {
            query.push(("page", p.to_string()));
        }
        if let Some(l) = opts.limit {
            query.push(("limit", l.to_string()));
        }
        if let Some(t) = opts.tenant_id {
            query.push(("tenant_id", t.to_string()));
        }
        if query.is_empty() {
            self.http.get("/voice-agents/configs").await
        } else {
            let query_refs: Vec<(&str, &str)> =
                query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            self.http
                .get_with_query("/voice-agents/configs", &query_refs)
                .await
        }
    }

    /// Get a single voice agent configuration.
    pub async fn get_config(&self, config_id: &str) -> Result<Value> {
        let path = format!("/voice-agents/configs/{}", urlencoding::encode(config_id));
        self.http.get(&path).await
    }

    /// Create a new voice agent configuration from a raw body.
    pub async fn create_config(&self, config: Value) -> Result<Value> {
        self.http.post("/voice-agents/configs", &config).await
    }

    /// Update an existing voice agent configuration.
    pub async fn update_config(&self, config_id: &str, config: Value) -> Result<Value> {
        let path = format!("/voice-agents/configs/{}", urlencoding::encode(config_id));
        self.http.put(&path, &config).await
    }

    /// Delete a voice agent configuration.
    pub async fn delete_config(&self, config_id: &str) -> Result<()> {
        let path = format!("/voice-agents/configs/{}", urlencoding::encode(config_id));
        self.http.delete(&path).await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // V2-005 — Cascade resolver (effective-config + pipeline)
    // -----------------------------------------------------------------------

    /// Resolves the effective voice-agent configuration after cascading
    /// platform → app → tenant → agent voice defaults.
    ///
    /// Backing endpoint: `GET /api/v1/voice-agents/configs/{id}/effective-config`
    /// (Python cascade resolver — V2-005).
    pub async fn get_effective_config(&self, agent_id: &str) -> Result<VoiceEffectiveConfig> {
        let path = format!("/voice-agents/configs/{}/effective-config", agent_id);
        let raw: Value = self.http.get(&path).await?;
        let cfg: VoiceEffectiveConfig =
            serde_json::from_value(raw).map_err(crate::error::OlympusError::from)?;
        Ok(cfg)
    }

    /// Resolves only the pipeline view of an agent's configuration. Cheaper
    /// than [`get_effective_config`](Self::get_effective_config) when callers
    /// only need the pipeline name + config.
    ///
    /// Backing endpoint: `GET /api/v1/voice-agents/configs/{id}/pipeline`.
    pub async fn get_pipeline(&self, agent_id: &str) -> Result<VoicePipeline> {
        let path = format!("/voice-agents/configs/{}/pipeline", agent_id);
        let raw: Value = self.http.get(&path).await?;
        let pipe: VoicePipeline =
            serde_json::from_value(raw).map_err(crate::error::OlympusError::from)?;
        Ok(pipe)
    }

    // -----------------------------------------------------------------------
    // Voice Pool (#232)
    // -----------------------------------------------------------------------

    /// Get the voice pool (persona rotation) for an agent.
    pub async fn get_pool(&self, agent_id: &str) -> Result<Value> {
        let path = format!("/voice-agents/{}/pool", urlencoding::encode(agent_id));
        self.http.get(&path).await
    }

    /// Add a persona to an agent's voice pool.
    pub async fn add_to_pool(&self, agent_id: &str, entry: Value) -> Result<Value> {
        let path = format!("/voice-agents/{}/pool", urlencoding::encode(agent_id));
        self.http.post(&path, &entry).await
    }

    /// Remove a persona from an agent's voice pool.
    pub async fn remove_from_pool(&self, agent_id: &str, entry_id: &str) -> Result<()> {
        let path = format!(
            "/voice-agents/{}/pool/{}",
            urlencoding::encode(agent_id),
            urlencoding::encode(entry_id)
        );
        self.http.delete(&path).await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Schedule (#232)
    // -----------------------------------------------------------------------

    /// Get the operating schedule for an agent.
    pub async fn get_schedule(&self, agent_id: &str) -> Result<Value> {
        let path = format!("/voice-agents/{}/schedule", urlencoding::encode(agent_id));
        self.http.get(&path).await
    }

    /// Update the operating schedule for an agent.
    pub async fn update_schedule(&self, agent_id: &str, request: Value) -> Result<Value> {
        let path = format!("/voice-agents/{}/schedule", urlencoding::encode(agent_id));
        self.http.put(&path, &request).await
    }

    // -----------------------------------------------------------------------
    // Provisioning wizard (#84)
    // -----------------------------------------------------------------------

    /// Start the provisioning wizard for a new agent.
    pub async fn provision_agent(&self, req: ProvisionAgentRequest<'_>) -> Result<Value> {
        let path = format!(
            "/ether/voice/agents/{}/provision-wizard",
            urlencoding::encode(req.agent_id)
        );
        let body = json!({
            "tenant_id": req.tenant_id,
            "voice_name": req.voice_name,
            "profile": req.profile,
            "greeting_text": req.greeting_text,
        });
        self.http.post(&path, &body).await
    }

    /// Get the current status of a provisioning job.
    pub async fn get_provisioning_status(&self, agent_id: &str, job_id: &str) -> Result<Value> {
        let path = format!(
            "/ether/voice/agents/{}/provisioning-status",
            urlencoding::encode(agent_id)
        );
        self.http.get_with_query(&path, &[("job_id", job_id)]).await
    }

    // -----------------------------------------------------------------------
    // Self-service agent CRUD (orderecho-ai#119)
    // -----------------------------------------------------------------------

    /// List all voice agents for the current tenant (alias for
    /// [`list_configs`](Self::list_configs)).
    pub async fn list_agents(&self, opts: PageOptions<'_>) -> Result<Value> {
        self.list_configs(opts).await
    }

    /// Get a single agent by ID. Alias for
    /// [`get_config`](Self::get_config).
    pub async fn get_agent(&self, agent_id: &str) -> Result<Value> {
        self.get_config(agent_id).await
    }

    /// Create a new voice agent from a typed request.
    pub async fn create_agent(&self, req: CreateAgentRequest<'_>) -> Result<Value> {
        let mut body = Map::new();
        if let Some(v) = req.from_template_id {
            body.insert("from_template_id".into(), Value::String(v.into()));
        }
        if let Some(v) = req.name {
            body.insert("name".into(), Value::String(v.into()));
        }
        if let Some(v) = req.voice_id {
            body.insert("voice_id".into(), Value::String(v.into()));
        }
        if let Some(v) = req.persona {
            body.insert("persona".into(), Value::String(v.into()));
        }
        if let Some(v) = req.greeting {
            body.insert("greeting".into(), Value::String(v.into()));
        }
        if let Some(v) = req.phone_number {
            body.insert("phone_number".into(), Value::String(v.into()));
        }
        if let Some(v) = req.location_id {
            body.insert("location_id".into(), Value::String(v.into()));
        }
        if let Some(v) = req.ambiance_config {
            body.insert("ambiance_config".into(), v);
        }
        if let Some(v) = req.voice_overrides {
            body.insert("voice_overrides".into(), v);
        }
        if let Some(v) = req.business_hours {
            body.insert("business_hours".into(), v);
        }
        if let Some(v) = req.escalation_rules {
            body.insert("escalation_rules".into(), v);
        }
        self.http
            .post("/voice-agents/configs", &Value::Object(body))
            .await
    }

    /// Update mutable fields on an existing agent.
    pub async fn update_agent(&self, agent_id: &str, req: UpdateAgentRequest<'_>) -> Result<Value> {
        let mut body = Map::new();
        if let Some(v) = req.name {
            body.insert("name".into(), Value::String(v.into()));
        }
        if let Some(v) = req.voice_id {
            body.insert("voice_id".into(), Value::String(v.into()));
        }
        if let Some(v) = req.persona {
            body.insert("persona".into(), Value::String(v.into()));
        }
        if let Some(v) = req.greeting {
            body.insert("greeting".into(), Value::String(v.into()));
        }
        if let Some(v) = req.ambiance_config {
            body.insert("ambiance_config".into(), v);
        }
        if let Some(v) = req.voice_overrides {
            body.insert("voice_overrides".into(), v);
        }
        if let Some(v) = req.business_hours {
            body.insert("business_hours".into(), v);
        }
        if let Some(v) = req.escalation_rules {
            body.insert("escalation_rules".into(), v);
        }
        if let Some(v) = req.is_active {
            body.insert("is_active".into(), Value::Bool(v));
        }
        let path = format!("/voice-agents/configs/{}", urlencoding::encode(agent_id));
        self.http.put(&path, &Value::Object(body)).await
    }

    /// Delete a voice agent. Alias for
    /// [`delete_config`](Self::delete_config).
    pub async fn delete_agent(&self, agent_id: &str) -> Result<()> {
        self.delete_config(agent_id).await
    }

    /// Clone an existing agent.
    pub async fn clone_agent(&self, agent_id: &str, req: CloneAgentRequest<'_>) -> Result<Value> {
        let mut body = Map::new();
        if let Some(v) = req.new_name {
            body.insert("new_name".into(), Value::String(v.into()));
        }
        if let Some(v) = req.phone_number {
            body.insert("phone_number".into(), Value::String(v.into()));
        }
        if let Some(v) = req.location_id {
            body.insert("location_id".into(), Value::String(v.into()));
        }
        let path = format!(
            "/voice-agents/configs/{}/clone",
            urlencoding::encode(agent_id)
        );
        self.http.post(&path, &Value::Object(body)).await
    }

    /// Generate a TTS preview clip for an agent.
    pub async fn preview_agent_voice(
        &self,
        agent_id: &str,
        req: PreviewAgentVoiceRequest<'_>,
    ) -> Result<Value> {
        let mut body = json!({ "sample_text": req.sample_text });
        if let Some(v) = req.voice_id {
            body["voice_id"] = Value::String(v.into());
        }
        if let Some(v) = req.voice_overrides {
            body["voice_overrides"] = v;
        }
        let path = format!(
            "/voice-agents/configs/{}/preview",
            urlencoding::encode(agent_id)
        );
        self.http.post(&path, &body).await
    }

    /// List the catalog of available Gemini Live voices.
    pub async fn list_gemini_voices(&self, opts: ListGeminiVoicesOptions<'_>) -> Result<Value> {
        if let Some(lang) = opts.language {
            self.http
                .get_with_query("/voice/voices", &[("language", lang)])
                .await
        } else {
            self.http.get("/voice/voices").await
        }
    }

    // -----------------------------------------------------------------------
    // Persona library
    // -----------------------------------------------------------------------

    /// List curated voice personas.
    pub async fn list_personas(&self, opts: ListPersonasOptions<'_>) -> Result<Value> {
        let premium_str;
        let mut query: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = opts.category {
            query.push(("category", v));
        }
        if let Some(v) = opts.industry {
            query.push(("industry", v));
        }
        if let Some(v) = opts.premium_only {
            premium_str = v.to_string();
            query.push(("premium_only", premium_str.as_str()));
        }
        if query.is_empty() {
            self.http.get("/voice/personas").await
        } else {
            self.http.get_with_query("/voice/personas", &query).await
        }
    }

    /// Get a single persona by ID or slug.
    pub async fn get_persona(&self, id_or_slug: &str) -> Result<Value> {
        let path = format!("/voice/personas/{}", urlencoding::encode(id_or_slug));
        self.http.get(&path).await
    }

    /// Apply a persona to an existing agent.
    pub async fn apply_persona_to_agent(
        &self,
        agent_id: &str,
        persona_id_or_slug: &str,
    ) -> Result<Value> {
        let path = format!(
            "/voice-agents/configs/{}/apply-persona",
            urlencoding::encode(agent_id)
        );
        let body = json!({ "persona": persona_id_or_slug });
        self.http.post(&path, &body).await
    }

    // -----------------------------------------------------------------------
    // Agent templates
    // -----------------------------------------------------------------------

    /// List voice agent templates.
    pub async fn list_agent_templates(&self, scope: Option<&str>) -> Result<Value> {
        if let Some(s) = scope {
            self.http
                .get_with_query("/voice-agents/templates", &[("scope", s)])
                .await
        } else {
            self.http.get("/voice-agents/templates").await
        }
    }

    /// Instantiate a new agent from an existing template.
    pub async fn instantiate_agent_template(
        &self,
        template_id: &str,
        req: InstantiateAgentTemplateRequest<'_>,
    ) -> Result<Value> {
        let mut body = json!({ "name": req.name });
        if let Some(v) = req.phone_number {
            body["phone_number"] = Value::String(v.into());
        }
        if let Some(v) = req.location_id {
            body["location_id"] = Value::String(v.into());
        }
        let path = format!(
            "/voice-agents/templates/{}/instantiate",
            urlencoding::encode(template_id)
        );
        self.http.post(&path, &body).await
    }

    /// Publish the current agent as a template.
    pub async fn publish_agent_as_template(
        &self,
        agent_id: &str,
        req: PublishAgentAsTemplateRequest<'_>,
    ) -> Result<Value> {
        let mut body = json!({ "scope": req.scope });
        if let Some(v) = req.description {
            body["description"] = Value::String(v.into());
        }
        let path = format!(
            "/voice-agents/configs/{}/publish-template",
            urlencoding::encode(agent_id)
        );
        self.http.post(&path, &body).await
    }

    /// List available agent templates (no-arg alias retained for dart
    /// parity with `listTemplates()`).
    pub async fn list_templates(&self) -> Result<Value> {
        self.http.get("/voice-agents/templates").await
    }

    // -----------------------------------------------------------------------
    // Background ambiance
    // -----------------------------------------------------------------------

    /// List the curated library of ambient beds.
    pub async fn list_ambiance_library(&self, category: Option<&str>) -> Result<Value> {
        if let Some(c) = category {
            self.http
                .get_with_query("/voice/ambiance/library", &[("category", c)])
                .await
        } else {
            self.http.get("/voice/ambiance/library").await
        }
    }

    /// Upload a custom ambient bed. The audio is base64-encoded before being
    /// sent as JSON.
    pub async fn upload_ambiance_bed(&self, req: UploadAmbianceBedRequest<'_>) -> Result<Value> {
        let audio_b64 = base64::engine::general_purpose::STANDARD.encode(req.audio_bytes);
        let mut body = json!({
            "name": req.name,
            "audio_base64": audio_b64,
        });
        if let Some(v) = req.time_of_day {
            body["time_of_day"] = Value::String(v.into());
        }
        if let Some(v) = req.description {
            body["description"] = Value::String(v.into());
        }
        self.http.post("/voice/ambiance/upload", &body).await
    }

    /// Update an agent's ambiance configuration.
    pub async fn update_agent_ambiance(
        &self,
        agent_id: &str,
        req: UpdateAgentAmbianceRequest<'_>,
    ) -> Result<Value> {
        let mut body = Map::new();
        if let Some(v) = req.enabled {
            body.insert("enabled".into(), Value::Bool(v));
        }
        if let Some(v) = req.intensity {
            body.insert(
                "intensity".into(),
                serde_json::Number::from_f64(v)
                    .map(Value::Number)
                    .unwrap_or(Value::Null),
            );
        }
        if let Some(v) = req.default_r2_key {
            body.insert("default_r2_key".into(), Value::String(v.into()));
        }
        if let Some(v) = req.time_of_day_variants {
            body.insert("time_of_day_variants".into(), Value::Object(v));
        }
        let path = format!(
            "/voice-agents/configs/{}/ambiance",
            urlencoding::encode(agent_id)
        );
        self.http.patch(&path, &Value::Object(body)).await
    }

    /// Update an agent's voice tuning overrides.
    pub async fn update_agent_voice_overrides(
        &self,
        agent_id: &str,
        req: UpdateAgentVoiceOverridesRequest<'_>,
    ) -> Result<Value> {
        let mut body = Map::new();
        if let Some(v) = req.pitch {
            body.insert(
                "pitch".into(),
                serde_json::Number::from_f64(v)
                    .map(Value::Number)
                    .unwrap_or(Value::Null),
            );
        }
        if let Some(v) = req.speed {
            body.insert(
                "speed".into(),
                serde_json::Number::from_f64(v)
                    .map(Value::Number)
                    .unwrap_or(Value::Null),
            );
        }
        if let Some(v) = req.warmth {
            body.insert(
                "warmth".into(),
                serde_json::Number::from_f64(v)
                    .map(Value::Number)
                    .unwrap_or(Value::Null),
            );
        }
        if let Some(v) = req.regional_dialect {
            body.insert("regional_dialect".into(), Value::String(v.into()));
        }
        let path = format!(
            "/voice-agents/configs/{}/voice-overrides",
            urlencoding::encode(agent_id)
        );
        self.http.patch(&path, &Value::Object(body)).await
    }

    // -----------------------------------------------------------------------
    // Workflow templates
    // -----------------------------------------------------------------------

    /// List all workflow templates for the current tenant.
    pub async fn list_workflow_templates(&self, opts: PageOptions<'_>) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(p) = opts.page {
            query.push(("page", p.to_string()));
        }
        if let Some(l) = opts.limit {
            query.push(("limit", l.to_string()));
        }
        if query.is_empty() {
            self.http.get("/voice/workflow-templates").await
        } else {
            let query_refs: Vec<(&str, &str)> =
                query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            self.http
                .get_with_query("/voice/workflow-templates", &query_refs)
                .await
        }
    }

    /// Create a new workflow template.
    pub async fn create_workflow_template(&self, request: Value) -> Result<Value> {
        self.http.post("/voice/workflow-templates", &request).await
    }

    /// Get a single workflow template by ID.
    pub async fn get_workflow_template(&self, id: &str) -> Result<Value> {
        let path = format!("/voice/workflow-templates/{}", urlencoding::encode(id));
        self.http.get(&path).await
    }

    /// Delete a workflow template.
    pub async fn delete_workflow_template(&self, id: &str) -> Result<()> {
        let path = format!("/voice/workflow-templates/{}", urlencoding::encode(id));
        self.http.delete(&path).await?;
        Ok(())
    }

    /// Instantiate a workflow from a template.
    pub async fn create_workflow_instance(
        &self,
        template_id: &str,
        params: Value,
    ) -> Result<Value> {
        let path = format!(
            "/voice/workflow-templates/{}/instances",
            urlencoding::encode(template_id)
        );
        self.http.post(&path, &params).await
    }

    // -----------------------------------------------------------------------
    // Voicemail (#232)
    // -----------------------------------------------------------------------

    /// List voicemails for the tenant.
    pub async fn list_voicemails(&self, opts: ListVoicemailsOptions<'_>) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(v) = opts.caller_phone {
            query.push(("caller_phone", v.to_string()));
        }
        if let Some(p) = opts.page {
            query.push(("page", p.to_string()));
        }
        if let Some(l) = opts.limit {
            query.push(("limit", l.to_string()));
        }
        if query.is_empty() {
            self.http.get("/voice/voicemails").await
        } else {
            let query_refs: Vec<(&str, &str)> =
                query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            self.http
                .get_with_query("/voice/voicemails", &query_refs)
                .await
        }
    }

    /// Update a voicemail (mark as read, resolve, etc.).
    pub async fn update_voicemail(&self, id: &str, data: Value) -> Result<Value> {
        let path = format!("/voice/voicemails/{}", urlencoding::encode(id));
        self.http.patch(&path, &data).await
    }

    /// Get a signed URL for a voicemail audio recording.
    pub async fn get_voicemail_audio_url(&self, id: &str) -> Result<Value> {
        let path = format!("/voice/voicemails/{}/audio", urlencoding::encode(id));
        self.http.get(&path).await
    }

    // -----------------------------------------------------------------------
    // Conversations + department messages
    // -----------------------------------------------------------------------

    /// List voice conversations with optional filters.
    pub async fn list_conversations(&self, opts: ListConversationsOptions<'_>) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(v) = opts.agent_id {
            query.push(("agent_id", v.to_string()));
        }
        if let Some(v) = opts.status {
            query.push(("status", v.to_string()));
        }
        if let Some(v) = opts.page {
            query.push(("page", v.to_string()));
        }
        if let Some(v) = opts.limit {
            query.push(("limit", v.to_string()));
        }
        if let Some(v) = opts.tenant_id {
            query.push(("tenant_id", v.to_string()));
        }
        if query.is_empty() {
            self.http.get("/voice-agents/conversations").await
        } else {
            let query_refs: Vec<(&str, &str)> =
                query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            self.http
                .get_with_query("/voice-agents/conversations", &query_refs)
                .await
        }
    }

    /// Get a single conversation with its transcript and metadata.
    pub async fn get_conversation(&self, conversation_id: &str) -> Result<Value> {
        let path = format!(
            "/voice-agents/conversations/{}",
            urlencoding::encode(conversation_id)
        );
        self.http.get(&path).await
    }

    /// List department messages.
    pub async fn list_messages(&self, opts: ListVoiceMessagesOptions<'_>) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(v) = opts.department {
            query.push(("department", v.to_string()));
        }
        if let Some(v) = opts.page {
            query.push(("page", v.to_string()));
        }
        if let Some(v) = opts.limit {
            query.push(("limit", v.to_string()));
        }
        if query.is_empty() {
            self.http.get("/voice/messages").await
        } else {
            let query_refs: Vec<(&str, &str)> =
                query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            self.http
                .get_with_query("/voice/messages", &query_refs)
                .await
        }
    }

    // -----------------------------------------------------------------------
    // Analytics
    // -----------------------------------------------------------------------

    /// Get voice analytics (call volume, duration, sentiment, etc.).
    pub async fn get_analytics(&self, opts: GetAnalyticsOptions<'_>) -> Result<Value> {
        let mut query: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = opts.agent_id {
            query.push(("agent_id", v));
        }
        if let Some(v) = opts.from {
            query.push(("from", v));
        }
        if let Some(v) = opts.to {
            query.push(("to", v));
        }
        if query.is_empty() {
            self.http.get("/voice-agents/analytics").await
        } else {
            self.http
                .get_with_query("/voice-agents/analytics", &query)
                .await
        }
    }

    // -----------------------------------------------------------------------
    // Campaigns
    // -----------------------------------------------------------------------

    /// List outbound voice campaigns.
    pub async fn list_campaigns(&self, opts: PageOptions<'_>) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(p) = opts.page {
            query.push(("page", p.to_string()));
        }
        if let Some(l) = opts.limit {
            query.push(("limit", l.to_string()));
        }
        if query.is_empty() {
            self.http.get("/voice-agents/campaigns").await
        } else {
            let query_refs: Vec<(&str, &str)> =
                query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            self.http
                .get_with_query("/voice-agents/campaigns", &query_refs)
                .await
        }
    }

    /// Get a single campaign by ID.
    pub async fn get_campaign(&self, campaign_id: &str) -> Result<Value> {
        let path = format!(
            "/voice-agents/campaigns/{}",
            urlencoding::encode(campaign_id)
        );
        self.http.get(&path).await
    }

    /// Create a new outbound campaign.
    pub async fn create_campaign(&self, campaign: Value) -> Result<Value> {
        self.http.post("/voice-agents/campaigns", &campaign).await
    }

    /// Update an existing campaign.
    pub async fn update_campaign(&self, campaign_id: &str, campaign: Value) -> Result<Value> {
        let path = format!(
            "/voice-agents/campaigns/{}",
            urlencoding::encode(campaign_id)
        );
        self.http.put(&path, &campaign).await
    }

    /// Delete a campaign.
    pub async fn delete_campaign(&self, campaign_id: &str) -> Result<()> {
        let path = format!(
            "/voice-agents/campaigns/{}",
            urlencoding::encode(campaign_id)
        );
        self.http.delete(&path).await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Phone Numbers
    // -----------------------------------------------------------------------

    /// List provisioned phone numbers.
    pub async fn list_numbers(&self, opts: PageOptions<'_>) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(p) = opts.page {
            query.push(("page", p.to_string()));
        }
        if let Some(l) = opts.limit {
            query.push(("limit", l.to_string()));
        }
        if query.is_empty() {
            self.http.get("/voice/phone-numbers").await
        } else {
            let query_refs: Vec<(&str, &str)> =
                query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            self.http
                .get_with_query("/voice/phone-numbers", &query_refs)
                .await
        }
    }

    /// Get details for a single phone number.
    pub async fn get_number(&self, number_id: &str) -> Result<Value> {
        let path = format!("/voice/phone-numbers/{}", urlencoding::encode(number_id));
        self.http.get(&path).await
    }

    /// Provision a new phone number.
    pub async fn provision_number(&self, request: Value) -> Result<Value> {
        self.http
            .post("/voice/phone-numbers/provision", &request)
            .await
    }

    /// Release a provisioned phone number.
    pub async fn release_number(&self, number_id: &str) -> Result<()> {
        let path = format!("/voice/phone-numbers/{}", urlencoding::encode(number_id));
        self.http.delete(&path).await?;
        Ok(())
    }

    /// Assign a phone number to a voice agent.
    pub async fn assign_number(&self, number_id: &str, agent_id: &str) -> Result<Value> {
        let path = format!(
            "/voice/phone-numbers/{}/assign",
            urlencoding::encode(number_id)
        );
        let body = json!({ "agent_id": agent_id });
        self.http.post(&path, &body).await
    }

    /// Search available phone numbers by area code or pattern.
    pub async fn search_numbers(&self, opts: SearchNumbersOptions<'_>) -> Result<Value> {
        let limit_str;
        let mut query: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = opts.area_code {
            query.push(("area_code", v));
        }
        if let Some(v) = opts.contains {
            query.push(("contains", v));
        }
        if let Some(v) = opts.country {
            query.push(("country", v));
        }
        if let Some(l) = opts.limit {
            limit_str = l.to_string();
            query.push(("limit", limit_str.as_str()));
        }
        if query.is_empty() {
            self.http.get("/voice/phone-numbers/search").await
        } else {
            self.http
                .get_with_query("/voice/phone-numbers/search", &query)
                .await
        }
    }

    /// Initiate a number port-in request.
    pub async fn port_number(&self, port_request: Value) -> Result<Value> {
        self.http
            .post("/voice/phone-numbers/port", &port_request)
            .await
    }

    /// Get the status of a port-in request.
    pub async fn get_port_status(&self, port_id: &str) -> Result<Value> {
        let path = format!("/voice/phone-numbers/port/{}", urlencoding::encode(port_id));
        self.http.get(&path).await
    }

    /// Cancel a pending port-in request.
    pub async fn cancel_port(&self, port_id: &str) -> Result<()> {
        let path = format!("/voice/phone-numbers/port/{}", urlencoding::encode(port_id));
        self.http.delete(&path).await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Marketplace (voices + packs)
    // -----------------------------------------------------------------------

    /// List available voices in the marketplace.
    pub async fn list_voices(&self, opts: ListVoicesOptions<'_>) -> Result<Value> {
        let limit_str;
        let mut query: Vec<(&str, &str)> = Vec::new();
        if let Some(v) = opts.language {
            query.push(("language", v));
        }
        if let Some(v) = opts.gender {
            query.push(("gender", v));
        }
        if let Some(l) = opts.limit {
            limit_str = l.to_string();
            query.push(("limit", limit_str.as_str()));
        }
        if query.is_empty() {
            self.http.get("/voice/marketplace/voices").await
        } else {
            self.http
                .get_with_query("/voice/marketplace/voices", &query)
                .await
        }
    }

    /// Get voices installed for the current tenant.
    pub async fn get_my_voices(&self) -> Result<Value> {
        self.http.get("/voice/marketplace/my-voices").await
    }

    /// List voice packs (bundles of voices).
    pub async fn list_packs(&self, limit: Option<u32>) -> Result<Value> {
        if let Some(l) = limit {
            let s = l.to_string();
            self.http
                .get_with_query("/voice/marketplace/packs", &[("limit", s.as_str())])
                .await
        } else {
            self.http.get("/voice/marketplace/packs").await
        }
    }

    /// Get a single voice pack by ID.
    pub async fn get_pack(&self, pack_id: &str) -> Result<Value> {
        let path = format!("/voice/marketplace/packs/{}", urlencoding::encode(pack_id));
        self.http.get(&path).await
    }

    /// Install a voice pack for the current tenant.
    pub async fn install_pack(&self, pack_id: &str) -> Result<Value> {
        let path = format!(
            "/voice/marketplace/packs/{}/install",
            urlencoding::encode(pack_id)
        );
        self.http.post(&path, &json!({})).await
    }

    // -----------------------------------------------------------------------
    // Calls
    // -----------------------------------------------------------------------

    /// End an active call by ID.
    pub async fn end_call(&self, call_id: &str) -> Result<()> {
        let path = format!("/voice/calls/{}/end", urlencoding::encode(call_id));
        self.http.post(&path, &json!({})).await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Speaker
    // -----------------------------------------------------------------------

    /// Get the speaker profile for a given speaker ID.
    pub async fn get_speaker_profile(&self, speaker_id: &str) -> Result<Value> {
        let path = format!("/voice/speaker/{}", urlencoding::encode(speaker_id));
        self.http.get(&path).await
    }

    /// Enroll a new speaker for voice recognition.
    pub async fn enroll_speaker(&self, enrollment: Value) -> Result<Value> {
        self.http.post("/voice/speaker/enroll", &enrollment).await
    }

    /// Add custom words or phrases for a speaker's vocabulary.
    pub async fn add_words(&self, speaker_id: &str, words: Vec<String>) -> Result<()> {
        let path = format!("/voice/speaker/{}/words", urlencoding::encode(speaker_id));
        let body = json!({ "words": words });
        self.http.post(&path, &body).await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Profiles
    // -----------------------------------------------------------------------

    /// List voice profiles for the tenant.
    pub async fn list_profiles(&self, opts: PageOptions<'_>) -> Result<Value> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(p) = opts.page {
            query.push(("page", p.to_string()));
        }
        if let Some(l) = opts.limit {
            query.push(("limit", l.to_string()));
        }
        if query.is_empty() {
            self.http.get("/voice/profiles").await
        } else {
            let query_refs: Vec<(&str, &str)> =
                query.iter().map(|(k, v)| (*k, v.as_str())).collect();
            self.http
                .get_with_query("/voice/profiles", &query_refs)
                .await
        }
    }

    /// Get a single voice profile by ID.
    pub async fn get_profile(&self, profile_id: &str) -> Result<Value> {
        let path = format!("/voice/profiles/{}", urlencoding::encode(profile_id));
        self.http.get(&path).await
    }

    /// Create a new voice profile.
    pub async fn create_profile(&self, profile: Value) -> Result<Value> {
        self.http.post("/voice/profiles", &profile).await
    }

    /// Update an existing voice profile.
    pub async fn update_profile(&self, profile_id: &str, profile: Value) -> Result<Value> {
        let path = format!("/voice/profiles/{}", urlencoding::encode(profile_id));
        self.http.put(&path, &profile).await
    }

    // -----------------------------------------------------------------------
    // Edge voice pipeline (CF Container — STT → Ether → TTS)
    // -----------------------------------------------------------------------

    /// Process recorded audio through the full edge voice pipeline.
    ///
    /// Sends audio to the CF Container voice pipeline which runs:
    /// STT (Workers AI Whisper, FREE) → Ether classification → AI response
    /// → TTS. Returns `{transcript, response, audio_url, pipeline_ms}`.
    pub async fn process_audio(&self, req: ProcessAudioRequest<'_>) -> Result<Value> {
        let audio_b64 = base64::engine::general_purpose::STANDARD.encode(req.audio_bytes);
        let mut body = json!({ "audio": audio_b64 });
        if let Some(v) = req.language {
            body["language"] = Value::String(v.into());
        }
        if let Some(v) = req.agent_id {
            body["agent_id"] = Value::String(v.into());
        }
        if let Some(v) = req.voice_id {
            body["voice_id"] = Value::String(v.into());
        }
        if let Some(v) = req.session_id {
            body["session_id"] = Value::String(v.into());
        }
        self.http.post("/voice/process", &body).await
    }

    /// Get the WebSocket URL for streaming voice interaction.
    ///
    /// The WebSocket endpoint at `/ws/voice` accepts:
    /// - `{type: "audio", data: "<base64>"}` — audio chunks
    /// - `{type: "barge_in"}` — interrupt current response
    /// - `{type: "ping"}` — keepalive
    ///
    /// And responds with:
    /// - `{type: "transcript", text: "..."}` — interim STT results
    /// - `{type: "response", text: "...", audio_url: "..."}` — AI response
    /// - `{type: "pong"}` — keepalive response
    ///
    /// Returns the full WebSocket URL based on the configured API base URL
    /// (swapping `https://` for `wss://`).
    pub fn get_voice_web_socket_url(&self, session_id: Option<&str>) -> String {
        let base = self
            .http
            .config()
            .base_url
            .replacen("https://", "wss://", 1);
        match session_id {
            Some(sid) => format!(
                "{}/ws/voice?session_id={}",
                base.trim_end_matches('/'),
                urlencoding::encode(sid)
            ),
            None => format!("{}/ws/voice", base.trim_end_matches('/')),
        }
    }

    /// Check edge voice pipeline health.
    pub async fn pipeline_health(&self) -> Result<Value> {
        self.http.get("/voice/pipeline/health").await
    }

    // -----------------------------------------------------------------------
    // Caller profiles (#2868)
    // -----------------------------------------------------------------------

    /// Look up a caller profile by phone number for personalized voice AI.
    ///
    /// Returns preferences, order history, loyalty tier, and past interactions.
    pub async fn get_caller_profile(&self, phone_number: &str) -> Result<Value> {
        let path = format!("/caller-profiles/{}", urlencoding::encode(phone_number));
        self.http.get(&path).await
    }

    /// List all caller profiles for the current tenant (paginated).
    pub async fn list_caller_profiles(&self, opts: ListCallerProfilesOptions) -> Result<Value> {
        let limit = opts.limit.to_string();
        let offset = opts.offset.to_string();
        self.http
            .get_with_query(
                "/caller-profiles",
                &[("limit", limit.as_str()), ("offset", offset.as_str())],
            )
            .await
    }

    /// Create or update a caller profile.
    pub async fn upsert_caller_profile(&self, profile: Value) -> Result<Value> {
        self.http.post("/caller-profiles", &profile).await
    }

    /// Delete a caller profile.
    pub async fn delete_caller_profile(&self, profile_id: &str) -> Result<()> {
        let path = format!("/caller-profiles/{}", urlencoding::encode(profile_id));
        self.http.delete(&path).await?;
        Ok(())
    }

    /// Record an order for a caller (updates stats + loyalty points).
    pub async fn record_caller_order(
        &self,
        phone_number: &str,
        order_data: Value,
    ) -> Result<Value> {
        let path = format!(
            "/caller-profiles/{}/orders",
            urlencoding::encode(phone_number)
        );
        self.http.post(&path, &order_data).await
    }

    // -----------------------------------------------------------------------
    // Escalation + business-hours config (#2870)
    // -----------------------------------------------------------------------

    /// Get voice agent escalation config (transfer targets, sentiment
    /// threshold).
    pub async fn get_escalation_config(&self, agent_id: &str) -> Result<Value> {
        let path = format!(
            "/voice-agents/{}/escalation-config",
            urlencoding::encode(agent_id)
        );
        self.http.get(&path).await
    }

    /// Update voice agent escalation config.
    pub async fn update_escalation_config(&self, agent_id: &str, config: Value) -> Result<Value> {
        let path = format!(
            "/voice-agents/{}/escalation-config",
            urlencoding::encode(agent_id)
        );
        self.http.put(&path, &config).await
    }

    /// Get voice agent business hours.
    pub async fn get_business_hours(&self, agent_id: &str) -> Result<Value> {
        let path = format!(
            "/voice-agents/{}/business-hours",
            urlencoding::encode(agent_id)
        );
        self.http.get(&path).await
    }

    /// Update voice agent business hours.
    pub async fn update_business_hours(&self, agent_id: &str, hours: Value) -> Result<Value> {
        let path = format!(
            "/voice-agents/{}/business-hours",
            urlencoding::encode(agent_id)
        );
        self.http.put(&path, &hours).await
    }

    // -----------------------------------------------------------------------
    // Agent testing (#170)
    // -----------------------------------------------------------------------

    /// Trigger an AI-to-AI test suite against a voice agent.
    ///
    /// The platform generates realistic caller scenarios, executes them
    /// against the agent, and returns a scorecard with transcripts and
    /// accuracy ratings.
    pub async fn test_agent(&self, tenant_id: &str, scenario_count: u32) -> Result<Value> {
        let body = json!({
            "tenant_id": tenant_id,
            "scenario_count": scenario_count,
        });
        self.http.post("/voice-agents/test", &body).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Canonical dev-gateway response captured 2026-04-18T01:31 UTC against
    // dev.api.olympuscloud.ai, agent 41f239da-c492-5fe6-9334-7bbc47804a36.
    const EFFECTIVE_CONFIG_FIXTURE: &str = r#"{
        "agentId": "41f239da-c492-5fe6-9334-7bbc47804a36",
        "tenantId": "550e8400-e29b-41d4-a716-446655449100",
        "pipeline": "olympus_native",
        "pipelineConfig": {
            "defaultLogLevel": "INFO",
            "tenantSeededAt": "2026-04-17-V2-005-fix-verification"
        },
        "tierOverride": "T3",
        "logLevel": "INFO",
        "debugTranscriptsEnabled": false,
        "v2ShadowEnabled": false,
        "v2PrimaryEnabled": false,
        "telephonyProvider": "telnyx",
        "providerAccountRef": "telnyx-dev-acct-v2-005-test",
        "preferredCodec": "opus",
        "preferredSampleRate": 48000,
        "hdAudioEnabled": true,
        "webhookPathOverride": "/v2/voice/inbound",
        "v2Routed": true,
        "voiceDefaults": {
            "platform": null,
            "app": null,
            "tenant": {
                "pipelineConfig": {
                    "defaultLogLevel": "INFO",
                    "tenantSeededAt": "2026-04-17-V2-005-fix-verification"
                },
                "tierOverride": "T3"
            },
            "agent": {
                "pipeline": "olympus_native",
                "pipelineConfig": {},
                "tierOverride": null,
                "logLevel": "INFO",
                "debugTranscriptsEnabled": false,
                "v2ShadowEnabled": false,
                "v2PrimaryEnabled": false
            }
        },
        "resolvedAt": "2026-04-18T01:31:52.064682+00:00",
        "cascadeVersion": "v2.1-rename"
    }"#;

    const PIPELINE_FIXTURE: &str = r#"{
        "agentId": "41f239da-c492-5fe6-9334-7bbc47804a36",
        "pipeline": "olympus_native",
        "pipelineConfig": {
            "defaultLogLevel": "INFO",
            "tenantSeededAt": "2026-04-17-V2-005-fix-verification"
        },
        "resolvedAt": "2026-04-18T01:32:52.722382+00:00",
        "cascadeVersion": "v2.1-rename"
    }"#;

    #[test]
    fn parses_canonical_effective_config() {
        let cfg: VoiceEffectiveConfig =
            serde_json::from_str(EFFECTIVE_CONFIG_FIXTURE).expect("deserialize");
        assert_eq!(cfg.agent_id, "41f239da-c492-5fe6-9334-7bbc47804a36");
        assert_eq!(cfg.tenant_id, "550e8400-e29b-41d4-a716-446655449100");
        assert_eq!(cfg.pipeline, "olympus_native");
        assert_eq!(cfg.tier_override.as_deref(), Some("T3"));
        assert_eq!(cfg.log_level, "INFO");
        assert_eq!(cfg.telephony_provider.as_deref(), Some("telnyx"));
        assert_eq!(cfg.preferred_sample_rate, Some(48000));
        assert_eq!(cfg.hd_audio_enabled, Some(true));
        assert_eq!(cfg.v2_routed, Some(true));
        assert_eq!(cfg.cascade_version, "v2.1-rename");
    }

    #[test]
    fn parses_cascade_rungs() {
        let cfg: VoiceEffectiveConfig =
            serde_json::from_str(EFFECTIVE_CONFIG_FIXTURE).expect("deserialize");
        assert!(cfg.voice_defaults.platform.is_none());
        assert!(cfg.voice_defaults.app.is_none());
        let tenant = cfg.voice_defaults.tenant.expect("tenant present");
        assert_eq!(tenant.tier_override.as_deref(), Some("T3"));
        let agent = cfg.voice_defaults.agent.expect("agent present");
        assert_eq!(agent.pipeline.as_deref(), Some("olympus_native"));
        assert_eq!(agent.debug_transcripts_enabled, Some(false));
    }

    #[test]
    fn tolerates_missing_optional_telephony() {
        let minimal = r#"{
            "agentId": "a",
            "tenantId": "t",
            "pipeline": "olympus_native",
            "pipelineConfig": {},
            "logLevel": "INFO",
            "debugTranscriptsEnabled": false,
            "v2ShadowEnabled": false,
            "v2PrimaryEnabled": false,
            "voiceDefaults": {},
            "resolvedAt": "2026-04-18T00:00:00Z",
            "cascadeVersion": "v2.1-rename"
        }"#;
        let cfg: VoiceEffectiveConfig = serde_json::from_str(minimal).expect("deserialize");
        assert!(cfg.telephony_provider.is_none());
        assert!(cfg.preferred_codec.is_none());
        assert!(cfg.voice_defaults.platform.is_none());
    }

    #[test]
    fn parses_canonical_pipeline() {
        let p: VoicePipeline = serde_json::from_str(PIPELINE_FIXTURE).expect("deserialize");
        assert_eq!(p.agent_id, "41f239da-c492-5fe6-9334-7bbc47804a36");
        assert_eq!(p.pipeline, "olympus_native");
        assert_eq!(p.cascade_version, "v2.1-rename");
    }

    #[test]
    fn effective_config_roundtrips() {
        let cfg: VoiceEffectiveConfig =
            serde_json::from_str(EFFECTIVE_CONFIG_FIXTURE).expect("deserialize");
        let v = serde_json::to_value(&cfg).expect("serialize");
        let back: VoiceEffectiveConfig = serde_json::from_value(v).expect("round-trip");
        assert_eq!(back.agent_id, cfg.agent_id);
        assert_eq!(back.cascade_version, cfg.cascade_version);
        assert_eq!(
            back.voice_defaults.tenant.map(|r| r.tier_override),
            Some(Some("T3".to_string()))
        );
    }
}
