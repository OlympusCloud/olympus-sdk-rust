use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;

use crate::config::OlympusConfig;
use crate::error::{OlympusError, Result};
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
use crate::services::compliance::ComplianceService;
use crate::services::connect::ConnectService;
use crate::services::consent::ConsentService;
use crate::services::creator::CreatorService;
use crate::services::enterprise_context::EnterpriseContextService;
use crate::services::ethical_ai::EthicalAiService;
use crate::services::finops::FinOpsService;
use crate::services::governance::GovernanceService;
use crate::services::identity::IdentityService;
use crate::services::messages::MessagesService;
use crate::services::pay::PayService;
use crate::services::platform::PlatformService;
use crate::services::pos::PosService;
use crate::services::smart_home::SmartHomeService;
use crate::services::sms::SmsService;
use crate::services::sre_analytics::SreAnalyticsService;
use crate::services::tuning::TuningService;
use crate::services::voice::VoiceService;
use crate::services::voice_marketplace::VoiceMarketplaceService;
use crate::services::voice_orders::VoiceOrdersService;
use crate::session::{AuthSession, SessionEvent};
use crate::silent_refresh::{spawn_refresh_loop, SilentRefreshHandle, SilentRefreshState};

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
    /// Shared silent-refresh state: broadcast sender + current task abort
    /// handle. Constructed once per client and reused across start/stop
    /// cycles so pre-stop subscribers still observe post-start events.
    refresh: Arc<SilentRefreshState>,
}

impl OlympusClient {
    /// Creates a new client with the given app_id and api_key, using production defaults.
    pub fn new(app_id: impl Into<String>, api_key: impl Into<String>) -> Self {
        let config = OlympusConfig::new(app_id, api_key);
        Self::from_config(config)
    }

    /// Creates a new client from an explicit configuration.
    pub fn from_config(config: OlympusConfig) -> Self {
        let http = OlympusHttpClient::new(Arc::new(config)).expect("failed to build HTTP client");
        Self {
            http: Arc::new(http),
            refresh: SilentRefreshState::new(),
        }
    }

    /// Returns a new client, or an error if the HTTP client cannot be constructed.
    pub fn try_from_config(config: OlympusConfig) -> Result<Self> {
        let http = OlympusHttpClient::new(Arc::new(config))?;
        Ok(Self {
            http: Arc::new(http),
            refresh: SilentRefreshState::new(),
        })
    }

    /// Crate-internal accessor for the shared HTTP transport. Used by
    /// borrow-pattern API modules ([`crate::tenant::TenantApi`],
    /// [`crate::identity::IdentityApi`]) that hold a `&OlympusClient` rather
    /// than a cloned `Arc<OlympusHttpClient>`.
    pub(crate) fn http(&self) -> &Arc<OlympusHttpClient> {
        &self.http
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

    /// Returns the Voice Marketplace reviews service (#3463).
    ///
    /// List/submit/delete reviews for marketplace voices.
    pub fn voice_marketplace(&self) -> VoiceMarketplaceService {
        VoiceMarketplaceService::new(Arc::clone(&self.http))
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

    /// Returns the Identity service — global, cross-tenant Olympus ID +
    /// age-verification (v0.5.0 Wave 2).
    pub fn identity(&self) -> IdentityService {
        IdentityService::new(Arc::clone(&self.http))
    }

    /// Returns the SmartHome service — consumer smart-home platforms,
    /// devices, rooms, scenes, and automations (v0.5.0 Wave 2).
    pub fn smart_home(&self) -> SmartHomeService {
        SmartHomeService::new(Arc::clone(&self.http))
    }

    /// Returns the SMS service — outbound SMS + delivery status via the
    /// CPaaS abstraction (Telnyx primary, Twilio fallback) (v0.5.0 Wave 2).
    pub fn sms(&self) -> SmsService {
        SmsService::new(Arc::clone(&self.http))
    }

    /// Returns the Consent service — app-scoped permission grants + prompts.
    pub fn consent(&self) -> ConsentService {
        ConsentService::new(Arc::clone(&self.http))
    }

    /// Returns the Compliance service — dram-shop compliance ledger (#3316).
    ///
    /// Append-only event ledger plus jurisdiction/app rule lookup used
    /// cross-app by BarOS and PizzaOS for liability tracking.
    pub fn compliance(&self) -> ComplianceService {
        ComplianceService::new(Arc::clone(&self.http))
    }

    /// Returns the Pay service — payment processor routing config (#3312).
    ///
    /// Per-location preferred/fallback processor configuration with
    /// canonical-secrets credential references. Other payment surfaces
    /// (intents, refunds, balance, payouts, terminal) are not yet wrapped
    /// on this Rust SDK.
    pub fn pay(&self) -> PayService {
        PayService::new(Arc::clone(&self.http))
    }

    /// Returns the Gating service — runtime feature gating + plan details (#3313).
    pub fn gating(&self) -> crate::services::gating::GatingService {
        crate::services::gating::GatingService::new(Arc::clone(&self.http))
    }

    /// Returns the Governance service — narrow policy-exception framework.
    pub fn governance(&self) -> GovernanceService {
        GovernanceService::new(Arc::clone(&self.http))
    }

    /// Returns the Tenant lifecycle API (#3403 §2 + §4.4).
    ///
    /// Canonical `/tenant/*` surface — signup, update, retire/unretire,
    /// multi-tenant listing, cross-tenant switch. Complements
    /// [`Self::platform`] (the legacy signup/cleanup workflow) which remains
    /// for operator-driven flows.
    pub fn tenant(&self) -> crate::tenant::TenantApi<'_> {
        crate::tenant::TenantApi::new(self)
    }

    /// Returns the Identity Invite API (#3403 §4.2 + §4.4).
    ///
    /// Canonical `/identity/invite*` surface — invite staff/managers, list
    /// pending invites, accept/revoke invites, remove users from a tenant
    /// while preserving their Firebase identity. Distinct from
    /// [`Self::identity`] which wraps the global Olympus ID / age-verification
    /// surface.
    pub fn identity_invites(&self) -> crate::identity::IdentityApi<'_> {
        crate::identity::IdentityApi::new(self)
    }

    /// Returns the Apps install ceremony API (#3413 §3).
    ///
    /// Canonical `/apps/*` surface — install, list, uninstall, and consent
    /// ceremony (pending-install create/get/approve/deny). Wires to the
    /// Rust platform service routes shipped in olympus-cloud-gcp#3422.
    pub fn apps(&self) -> crate::apps::AppsApi<'_> {
        crate::apps::AppsApi::new(self)
    }

    // -----------------------------------------------------------------------
    // Token management — thin pass-throughs onto the shared HTTP client.
    // -----------------------------------------------------------------------

    /// Set the user access token (Authorization bearer).
    pub fn set_access_token(&self, token: impl Into<String>) {
        self.http.set_access_token(token);
    }

    /// Clear the user access token.
    pub fn clear_access_token(&self) {
        self.http.clear_access_token();
    }

    /// Set the App JWT (X-App-Token).
    pub fn set_app_token(&self, token: impl Into<String>) {
        self.http.set_app_token(token);
    }

    /// Clear the App JWT.
    pub fn clear_app_token(&self) {
        self.http.clear_app_token();
    }

    /// Set the refresh token used by the silent-refresh task (#3412). The
    /// SDK does not persist this value — pair with a platform-appropriate
    /// secure store in your app.
    pub fn set_refresh_token(&self, token: impl Into<String>) {
        self.http.set_refresh_token(token);
    }

    /// Clear the refresh token.
    pub fn clear_refresh_token(&self) {
        self.http.clear_refresh_token();
    }

    /// Register a stale-catalog handler (fires on `X-Olympus-Catalog-Stale:
    /// true`, debounced per-token).
    pub fn on_catalog_stale(&self, handler: Option<crate::http::StaleCatalogHandler>) {
        self.http.on_catalog_stale(handler);
    }

    // -----------------------------------------------------------------------
    // Silent token refresh + session events (#3403 §1.4 / #3412).
    // -----------------------------------------------------------------------

    /// Subscribe to session lifecycle transitions — login, silent refresh,
    /// forced expiry, explicit logout. Returns a
    /// [`tokio::sync::broadcast::Receiver`]. Lagged receivers will observe
    /// [`tokio::sync::broadcast::error::RecvError::Lagged`]; the channel
    /// capacity is 32.
    ///
    /// The broadcast channel outlives start/stop cycles — a receiver taken
    /// before `stop_silent_refresh` will continue to observe subsequent
    /// `LoggedIn` / `Refreshed` events after a fresh `start_silent_refresh`.
    pub fn session_events(&self) -> broadcast::Receiver<SessionEvent> {
        self.refresh.sender.subscribe()
    }

    /// Emit a [`SessionEvent::LoggedIn`] transition. Callers who have
    /// completed a login flow outside the SDK (or who want to seed the
    /// broadcast channel after calling [`crate::services::auth::AuthService::login`])
    /// can invoke this to notify subscribers.
    ///
    /// Does not mutate tokens — callers should already have invoked
    /// [`Self::set_access_token`] + [`Self::set_refresh_token`].
    pub fn emit_logged_in(&self, session: AuthSession) {
        self.refresh.emit(SessionEvent::LoggedIn(session));
    }

    /// Start the in-SDK silent refresh task. The task sleeps until
    /// `exp - refresh_margin` (decoded from the current access token),
    /// POSTs `/auth/refresh` with the cached refresh token, swaps the
    /// access token on success, and broadcasts a
    /// [`SessionEvent::Refreshed`]. On failure the task broadcasts
    /// [`SessionEvent::Expired`] and exits.
    ///
    /// Idempotent — a second call aborts the first task before spawning a
    /// replacement. Dropping the returned [`SilentRefreshHandle`] also
    /// cancels the task.
    ///
    /// Requires a tokio runtime on the current thread (Rust async SDKs
    /// typically run inside one already).
    pub fn start_silent_refresh(&self, refresh_margin: Duration) -> SilentRefreshHandle {
        let abort = spawn_refresh_loop(
            Arc::clone(&self.http),
            Arc::clone(&self.refresh),
            refresh_margin,
        );
        // Register the new task in the client-side slot (aborts the prior one).
        self.refresh.set_current(abort.clone());
        SilentRefreshHandle::new(abort)
    }

    /// Stop the silent-refresh task, if any. Idempotent. Does NOT broadcast
    /// a session event — use [`Self::logout`] for that.
    pub fn stop_silent_refresh(&self) {
        self.refresh.abort_current();
    }

    /// Log out: abort the silent-refresh task, clear the access + refresh
    /// tokens, and broadcast [`SessionEvent::LoggedOut`]. Does not call the
    /// server — callers should `await auth().logout()` (or equivalent)
    /// before invoking this when the server-side invalidation matters.
    pub fn logout(&self) {
        self.refresh.abort_current();
        self.http.clear_access_token();
        self.http.clear_refresh_token();
        self.refresh.emit(SessionEvent::LoggedOut);
    }

    // -----------------------------------------------------------------------
    // App-scoped permissions fast path — decodes `app_scopes_bitset` from
    // the current access token (cached per-token by the HTTP client).
    // -----------------------------------------------------------------------

    /// Returns true if the current access token is app-scoped (i.e. carries
    /// an `app_id` claim and non-empty `app_scopes_bitset`).
    ///
    /// Platform-shell tokens (no app context) always return `false`.
    pub fn is_app_scoped(&self) -> bool {
        let claims = match self.http.decoded_claims_cached() {
            Some(c) => c,
            None => return false,
        };
        let has_app_id = claims
            .get("app_id")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        if !has_app_id {
            return false;
        }
        match self.http.decoded_bitset_cached() {
            Some(bs) => !bs.is_empty(),
            None => false,
        }
    }

    /// Fast-path O(1) scope check against the bitset embedded in the current
    /// access token. Returns `false` for any out-of-range bit, for missing
    /// bitset, or when no token is set.
    pub fn has_scope_bit(&self, bit: usize) -> bool {
        let bytes = match self.http.decoded_bitset_cached() {
            Some(b) => b,
            None => return false,
        };
        let byte_idx = bit / 8;
        if byte_idx >= bytes.len() {
            return false;
        }
        (bytes[byte_idx] >> (bit % 8)) & 1 == 1
    }

    /// String-keyed scope check against the `app_scopes` array claim in the
    /// current access token. Complements [`Self::has_scope_bit`] for callers
    /// that have the canonical scope string (e.g. generated constants from
    /// [`crate::OlympusScopes`]) but not the catalog bit ID.
    ///
    /// Returns `false` when no access token is set, when the token carries no
    /// `app_scopes` claim (platform-shell tokens), or when the scope is not
    /// present in the granted set.
    ///
    /// See #3403 §1.2.
    pub fn has_scope(&self, scope: &str) -> bool {
        self.granted_scopes().contains(scope)
    }

    /// Returns `Ok(())` if the scope is granted, else
    /// [`OlympusError::ScopeRequired`]. This is a **client-side precheck** —
    /// it does not call the server. The server remains the source of truth
    /// (responds with [`OlympusError::ScopeDenied`] /
    /// [`OlympusError::ConsentRequired`] on actual requests).
    ///
    /// See #3403 §1.2.
    pub fn require_scope(&self, scope: &str) -> Result<()> {
        if self.has_scope(scope) {
            Ok(())
        } else {
            Err(OlympusError::ScopeRequired {
                scope: scope.to_string(),
            })
        }
    }

    /// All scopes granted to the current session, decoded from the
    /// `app_scopes` claim on the current access token.
    ///
    /// Returns an empty set when no token is set, when the token carries no
    /// `app_scopes` claim, or when the claim is not a JSON array of strings.
    /// The returned set is a fresh allocation; the SDK does not retain it.
    ///
    /// See #3403 §1.2.
    pub fn granted_scopes(&self) -> HashSet<String> {
        let claims = match self.http.decoded_claims_cached() {
            Some(c) => c,
            None => return HashSet::new(),
        };
        let arr = match claims.get("app_scopes").and_then(|v| v.as_array()) {
            Some(a) => a,
            None => return HashSet::new(),
        };
        arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect()
    }
}
