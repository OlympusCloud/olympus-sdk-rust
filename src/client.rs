use std::sync::Arc;

use crate::config::OlympusConfig;
use crate::error::Result;
use crate::http::OlympusHttpClient;
use crate::services::admin_billing::AdminBillingService;
use crate::services::admin_cpaas::AdminCpaasService;
use crate::services::admin_ether::AdminEtherService;
use crate::services::admin_gating::AdminGatingService;
use crate::services::admin_ops::AdminOpsService;
use crate::services::agent_workflows::AgentWorkflowsService;
use crate::services::ai::AiService;
use crate::services::auth::AuthService;
use crate::services::business::BusinessService;
use crate::services::chaos::ChaosService;
use crate::services::commerce::CommerceService;
use crate::services::connect::ConnectService;
use crate::services::consent::ConsentService;
use crate::services::creator::CreatorService;
use crate::services::enterprise_context::EnterpriseContextService;
use crate::services::ethical_ai::EthicalAiService;
use crate::services::finops::FinOpsService;
use crate::services::governance::GovernanceService;
use crate::services::messages::MessagesService;
use crate::services::platform::PlatformService;
use crate::services::pos::PosService;
use crate::services::sre_analytics::SreAnalyticsService;
use crate::services::tuning::TuningService;
use crate::services::voice::VoiceService;
use crate::services::voice_orders::VoiceOrdersService;

/// Main entry point for the Olympus Cloud SDK.
///
/// Provides typed async access to all platform services via lazy-initialized
/// service accessors.
///
/// # Example
///
/// ```rust,no_run
/// use olympus_sdk::OlympusClient;
///
/// #[tokio::main]
/// async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
///     let client = OlympusClient::new("com.my-app", "oc_live_...");
///     let orders = client.commerce().list_orders(None).await?;
///     Ok(())
/// }
/// ```
pub struct OlympusClient {
    http: Arc<OlympusHttpClient>,
}

impl OlympusClient {
    /// Creates a new client with the given app_id and api_key, using production defaults.
    pub fn new(app_id: impl Into<String>, api_key: impl Into<String>) -> Self {
        let config = OlympusConfig::new(app_id, api_key);
        Self::from_config(config)
    }

    /// Creates a new client from an explicit configuration.
    pub fn from_config(config: OlympusConfig) -> Self {
        let http = OlympusHttpClient::new(Arc::new(config))
            .expect("failed to build HTTP client");
        Self {
            http: Arc::new(http),
        }
    }

    /// Returns a new client, or an error if the HTTP client cannot be constructed.
    pub fn try_from_config(config: OlympusConfig) -> Result<Self> {
        let http = OlympusHttpClient::new(Arc::new(config))?;
        Ok(Self {
            http: Arc::new(http),
        })
    }

    /// Returns the authentication service.
    pub fn auth(&self) -> AuthService {
        AuthService::new(Arc::clone(&self.http))
    }

    /// Returns the commerce/orders service.
    pub fn commerce(&self) -> CommerceService {
        CommerceService::new(Arc::clone(&self.http))
    }

    /// Returns the AI inference and agent service.
    pub fn ai(&self) -> AiService {
        AiService::new(Arc::clone(&self.http))
    }

    /// Returns the creator platform service.
    pub fn creator(&self) -> CreatorService {
        CreatorService::new(Arc::clone(&self.http))
    }

    /// Returns the tenant lifecycle (platform) service.
    pub fn platform(&self) -> PlatformService {
        PlatformService::new(Arc::clone(&self.http))
    }

    /// Returns the business data access service.
    pub fn business(&self) -> BusinessService {
        BusinessService::new(Arc::clone(&self.http))
    }

    /// Returns the POS voice order integration service.
    pub fn pos(&self) -> PosService {
        PosService::new(Arc::clone(&self.http))
    }

    /// Returns the AI Agent Workflow Orchestration service (#2915).
    ///
    /// Tenant-scoped multi-agent DAG pipelines with cron/event triggers,
    /// capability routing, and billing. Distinct from marketplace workflows.
    pub fn agent_workflows(&self) -> AgentWorkflowsService {
        AgentWorkflowsService::new(Arc::clone(&self.http))
    }

    /// Returns the Enterprise Context service (#2993).
    ///
    /// Company 360 context assembly for AI agents -- brand, locations, menu,
    /// specials, FAQs, upsells, inventory, caller profile, graph relationships.
    pub fn enterprise_context(&self) -> EnterpriseContextService {
        EnterpriseContextService::new(Arc::clone(&self.http))
    }

    /// Returns the Messages service (#2997).
    ///
    /// Message queue with department routing. AI agents route messages to
    /// business departments when they cannot fully handle a request.
    pub fn messages(&self) -> MessagesService {
        MessagesService::new(Arc::clone(&self.http))
    }

    /// Returns the Voice Orders service (#2999).
    ///
    /// AI voice order placement with price validation, Spanner storage,
    /// and POS push (Toast/Square/Clover).
    pub fn voice_orders(&self) -> VoiceOrdersService {
        VoiceOrdersService::new(Arc::clone(&self.http))
    }

    /// Returns the Voice AI service with V2-005 cascade resolver (#3162).
    /// v0.4.0 Wave 1.
    pub fn voice(&self) -> VoiceService {
        VoiceService::new(Arc::clone(&self.http))
    }

    /// Returns the Marketing Connect service — /leads funnel (#3108).
    /// v0.4.0 Wave 1.
    pub fn connect(&self) -> ConnectService {
        ConnectService::new(Arc::clone(&self.http))
    }

    /// Returns the Admin Operations service (#243).
    ///
    /// Platform-level admin operations: impersonation, billing, sales,
    /// support tickets, onboarding, and devbox lifecycle management.
    pub fn admin_ops(&self) -> AdminOpsService {
        AdminOpsService::new(Arc::clone(&self.http))
    }

    /// Returns the Ether AI model catalog admin service.
    ///
    /// CRUD for models and tiers, plus hot-reload of the catalog cache.
    pub fn admin_ether(&self) -> AdminEtherService {
        AdminEtherService::new(Arc::clone(&self.http))
    }

    /// Returns the CPaaS provider configuration and health admin service.
    ///
    /// Controls Telnyx-primary / Twilio-fallback routing, provider preferences
    /// per scope (tenant, brand, location), and circuit-breaker health.
    pub fn admin_cpaas(&self) -> AdminCpaasService {
        AdminCpaasService::new(Arc::clone(&self.http))
    }

    /// Returns the billing plan catalog and usage metering admin service.
    ///
    /// Manages the global plan catalog, add-ons, minute packs, and usage recording.
    pub fn admin_billing(&self) -> AdminBillingService {
        AdminBillingService::new(Arc::clone(&self.http))
    }

    /// Returns the feature flag and gating admin service.
    ///
    /// Manages feature definitions, plan-level assignments, resource limits,
    /// and feature evaluation.
    pub fn admin_gating(&self) -> AdminGatingService {
        AdminGatingService::new(Arc::clone(&self.http))
    }

    /// Returns the AI tuning jobs, persona generation, and chaos audio service.
    ///
    /// Model fine-tuning lifecycle, synthetic persona generation for load
    /// testing, and audio noise simulation for chaos testing voice pipelines.
    pub fn tuning(&self) -> TuningService {
        TuningService::new(Arc::clone(&self.http))
    }

    /// Returns the Chaos Engineering service (#2938, #2939, #2940).
    ///
    /// Fault injection queue, DR drills with RTO/RPO analysis,
    /// and gameday orchestration for resilience testing.
    pub fn chaos(&self) -> ChaosService {
        ChaosService::new(Arc::clone(&self.http))
    }

    /// Returns the Ethical AI governance service (#2935, #2936, #2937).
    ///
    /// Bias detection, red-teaming, model cards, AI safety policy,
    /// and explainability reporting.
    pub fn ethical_ai(&self) -> EthicalAiService {
        EthicalAiService::new(Arc::clone(&self.http))
    }

    /// Returns the FinOps service (#2941, #2942, #2943).
    ///
    /// AI cost management dashboard, budget enforcement with hard limits,
    /// cost anomaly detection, and optimization recommendations.
    pub fn finops(&self) -> FinOpsService {
        FinOpsService::new(Arc::clone(&self.http))
    }

    /// Returns the SRE Analytics service (#2945, #2946, #2947).
    ///
    /// SLO tracking with error budgets, synthetic monitoring probes,
    /// capacity planning forecasts, and incident management.
    pub fn sre_analytics(&self) -> SreAnalyticsService {
        SreAnalyticsService::new(Arc::clone(&self.http))
    }

    // =======================================================================
    // App-scoped permissions (v2.0 — olympus-cloud-gcp#3254 / #3234 epic)
    // =======================================================================

    /// Returns the app-scoped consent service (v2.0).
    ///
    /// Platform-governed scope grants for tenant-admin + end-user consent.
    /// See docs/platform/APP-SCOPED-PERMISSIONS.md §6.
    pub fn consent(&self) -> ConsentService {
        ConsentService::new(Arc::clone(&self.http))
    }

    /// Returns the policy exception framework service (v2.0).
    ///
    /// Narrow scope: `session_ttl_role_ceiling` and `grace_policy_category`
    /// only. See §17 of APP-SCOPED-PERMISSIONS.md.
    pub fn governance(&self) -> GovernanceService {
        GovernanceService::new(Arc::clone(&self.http))
    }

    /// Attach the App JWT obtained from `/auth/app-tokens/mint`. Forwarded on
    /// every request as `X-App-Token` alongside the user JWT Authorization
    /// header per the §4.5 dual-JWT flow.
    pub fn set_app_token(&self, token: impl Into<String>) {
        self.http.set_app_token(token);
    }

    /// Clear the app token.
    pub fn clear_app_token(&self) {
        self.http.clear_app_token();
    }

    /// Replace the active access token.
    pub fn set_access_token(&self, token: impl Into<String>) {
        self.http.set_access_token(token);
    }

    /// Register a handler for `X-Olympus-Catalog-Stale: true` (§4.7 rolling
    /// window). Handler fires AT MOST ONCE per stale-token window — resets
    /// when the access or app token changes. Consumers should schedule a
    /// background refresh at a randomized 0–15 min offset.
    pub fn on_catalog_stale(&self, handler: Option<crate::http::StaleCatalogHandler>) {
        self.http.on_catalog_stale(handler);
    }

    /// Fast-path constant-time bitmask check against the decoded
    /// `app_scopes_bitset` claim in the current access token.
    ///
    /// Returns `false` when no token set, for platform-shell tokens without a
    /// bitset, when `bit_id` is out of range. Used by SDK service methods to
    /// fail-fast with [`OlympusError::ScopeDenied`] BEFORE an HTTP call.
    ///
    /// The decode (base64 + JSON parse) is cached per-token on the underlying
    /// HTTP state, so repeated calls with the same token amortize to a single
    /// decode; the cache invalidates automatically on
    /// [`Self::set_access_token`] / [`Self::clear_app_token`].
    pub fn has_scope_bit(&self, bit_id: usize) -> bool {
        let Some(bytes) = self.http.decoded_bitset_cached() else {
            return false;
        };
        let byte_idx = bit_id / 8;
        let bit_idx = bit_id % 8;
        match bytes.get(byte_idx) {
            Some(&b) => b & (1 << bit_idx) != 0,
            None => false,
        }
    }

    /// True when the current access token carries an `app_id` claim (minted
    /// via `/auth/app-tokens/mint` for an app-scoped session).
    pub fn is_app_scoped(&self) -> bool {
        self.http
            .decoded_claims_cached()
            .and_then(|c| c.get("app_id").and_then(|v| v.as_str()).map(|_| ()))
            .is_some()
    }
}
