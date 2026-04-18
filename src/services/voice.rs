use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::Result;
use crate::http::OlympusHttpClient;

/// Voice AI service covering Voice Agent V2 cascade resolver endpoints
/// (V2-005, issue OlympusCloud/olympus-cloud-gcp#3162) and related
/// voice-agent operations.
///
/// Routes: `/voice-agents/configs/*`.
pub struct VoiceService {
    http: Arc<OlympusHttpClient>,
}

/// A single rung of the voice-defaults cascade.
///
/// Each rung (platform, app, tenant, agent) carries whatever subset of
/// configuration was set at that scope. Nil on the wire becomes `None`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VoiceDefaultsRung {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipeline: Option<String>,
    #[serde(default, rename = "pipelineConfig", skip_serializing_if = "Option::is_none")]
    pub pipeline_config: Option<Map<String, Value>>,
    #[serde(default, rename = "tierOverride", skip_serializing_if = "Option::is_none")]
    pub tier_override: Option<String>,
    #[serde(default, rename = "logLevel", skip_serializing_if = "Option::is_none")]
    pub log_level: Option<String>,
    #[serde(default, rename = "debugTranscriptsEnabled", skip_serializing_if = "Option::is_none")]
    pub debug_transcripts_enabled: Option<bool>,
    #[serde(default, rename = "v2ShadowEnabled", skip_serializing_if = "Option::is_none")]
    pub v2_shadow_enabled: Option<bool>,
    #[serde(default, rename = "v2PrimaryEnabled", skip_serializing_if = "Option::is_none")]
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
    #[serde(rename = "tierOverride", default, skip_serializing_if = "Option::is_none")]
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
    #[serde(rename = "telephonyProvider", default, skip_serializing_if = "Option::is_none")]
    pub telephony_provider: Option<String>,
    #[serde(rename = "providerAccountRef", default, skip_serializing_if = "Option::is_none")]
    pub provider_account_ref: Option<String>,
    #[serde(rename = "preferredCodec", default, skip_serializing_if = "Option::is_none")]
    pub preferred_codec: Option<String>,
    #[serde(rename = "preferredSampleRate", default, skip_serializing_if = "Option::is_none")]
    pub preferred_sample_rate: Option<i64>,
    #[serde(rename = "hdAudioEnabled", default, skip_serializing_if = "Option::is_none")]
    pub hd_audio_enabled: Option<bool>,
    #[serde(rename = "webhookPathOverride", default, skip_serializing_if = "Option::is_none")]
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

impl VoiceService {
    /// Creates a new VoiceService instance.
    pub fn new(http: Arc<OlympusHttpClient>) -> Self {
        Self { http }
    }

    /// Resolves the effective voice-agent configuration after cascading
    /// platform → app → tenant → agent voice defaults.
    ///
    /// Backing endpoint: `GET /api/v1/voice-agents/configs/{id}/effective-config`
    /// (Python cascade resolver — V2-005).
    pub async fn get_effective_config(
        &self,
        agent_id: &str,
    ) -> Result<VoiceEffectiveConfig> {
        let path = format!("/voice-agents/configs/{}/effective-config", agent_id);
        let raw: Value = self.http.get(&path).await?;
        let cfg: VoiceEffectiveConfig = serde_json::from_value(raw)
            .map_err(crate::error::OlympusError::from)?;
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
        let pipe: VoicePipeline = serde_json::from_value(raw)
            .map_err(crate::error::OlympusError::from)?;
        Ok(pipe)
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
        let p: VoicePipeline =
            serde_json::from_str(PIPELINE_FIXTURE).expect("deserialize");
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
