# Changelog

## Unreleased

### Added — `oc.i18n()` error manifest consumer (#3638 / monorepo PR #3626)

New `I18nService` wrapping `GET /v1/i18n/errors`, the centralised error
code → localized message manifest served by the Rust platform service.
Apps localize platform errors without bundling per-app translations:

```rust
use olympus_sdk::OlympusClient;

let client = OlympusClient::new("com.my-app", "oc_live_...");

// Fetch the manifest (cached for 1h after first call).
let manifest = client.i18n().errors("en").await?;

// Look up a single code in the user's locale, with `en` fallback.
let msg = client.i18n().localize("ORDER_NOT_FOUND", "es").await?;
```

Concurrent cold callers share a single `tokio::sync::Mutex`-guarded HTTP
request — we never issue two parallel requests for the same payload.
The cache state lives on the `OlympusClient` so every `client.i18n()`
call returns a service backed by the same cache. Fallback chain:
`locale` → `en` → raw code. Cache TTL matches the backend
`Cache-Control: max-age=3600`.

## 0.5.4 (2026-04-25)

### Added — `oc.platform().list_scope_registry` + `get_scope_registry_digest` (gcp#3517)

Wraps the rust-platform scope-catalog read API:

```rust
use olympus_sdk::services::platform::ListScopeRegistryParams;

let listing = oc
    .platform()
    .list_scope_registry(ListScopeRegistryParams {
        namespace: Some("voice".into()),
        owner_app_id: Some("orderecho-ai".into()),
        include_drafts: false,
    })
    .await?;
for row in &listing.scopes {
    println!("{} (bit {:?}) — {}", row.scope, row.bit_id, row.description);
}

let digest = oc.platform().get_scope_registry_digest().await?;
// digest.platform_catalog_digest matches scripts/seed_platform_scopes.py byte-for-byte
```

`ScopeRow` exposes the full 13-field surface; `bit_id: Option<i64>` is
nullable for rows not yet allocated a bit (workshop_status pre-`service_ok`).
`owner_app_id: Some("")` is forwarded as the explicit "platform-owned only"
filter (semantically distinct from `None`).

## 0.5.3 (2026-04-25)

### Added — `oc.pay.list_routing` (gcp#3312 pt2 → PR #3537)

`PayService::list_routing` wraps `GET /platform/pay/routing` (the list
endpoint that landed in olympus-cloud-gcp PR #3537). Lists every payment-
routing config for the caller's tenant with optional filters:

```rust
use olympus_sdk::services::pay::ListRoutingParams;

let result = oc
    .pay()
    .list_routing(ListRoutingParams {
        is_active: Some(true),
        processor: Some("square".into()),
        limit: Some(50),
    })
    .await?;
for cfg in &result.configs {
    println!("{} → {}", cfg.location_id, cfg.preferred_processor);
}
println!("returned {} configs", result.total_returned);
```

`RoutingConfigList` exposes `configs: Vec<RoutingConfig>` + `total_returned`
so callers can detect when `limit` capped the result. `is_active = None`
returns both active + inactive configs (matches the server default).
Backwards-compatible — existing `configure_routing` / `get_routing`
wrappers unchanged.

## Unreleased

### AppsApi — apps.install consent ceremony (#3413 §3)

Wires to the `/apps/*` routes shipped in olympus-cloud-gcp#3422. Cross-SDK
parity with sdk-dart#26.

- `OlympusClient::apps() -> AppsApi<'_>` — canonical `/apps/*` surface
  driving the four-state install ceremony (install → consent → approve/deny
  → steady state).
  - `AppsApi::install(AppInstallRequest) -> PendingInstall` — initiate the
    ceremony. Server creates a pending-install row with a 10-minute TTL.
    Idempotent on `(tenant_id, app_id, idempotency_key)` within the window.
  - `AppsApi::list_installed() -> Vec<AppInstall>` — every active app on
    the caller's tenant.
  - `AppsApi::uninstall(&str)` — tenant_admin + MFA; emits
    `platform.app.uninstalled` driving session revocation downstream.
  - `AppsApi::get_manifest(&str) -> AppManifest` — latest published row.
  - `AppsApi::get_pending_install(&str) -> PendingInstallDetail` —
    **anonymous**; the unguessable UUID IS the bearer. Eager-loads the
    manifest onto `PendingInstallDetail::manifest`.
  - `AppsApi::approve_pending_install(&str) -> AppInstall` — tenant_admin
    + MFA; returns the fresh install row.
  - `AppsApi::deny_pending_install(&str)` — tenant_admin (no MFA).
- New public types at the crate root: `AppsApi`, `AppInstall` (the canonical
  6-field `/apps/installed` row — distinct from the inline 3-field
  `TenantAppInstall` returned by `/tenant/create`), `AppInstallRequest`,
  `AppManifest`, `PendingInstall`, `PendingInstallDetail`.
- **Rename** (unreleased, no crates.io publish yet): the 3-field struct
  previously exported as `olympus_sdk::AppInstall` is now
  `olympus_sdk::TenantAppInstall` to make room for the canonical 6-field
  apps-ceremony shape and match the Dart SDK naming.

### TenantApi + IdentityApi invite wrappers (#3403 §4.2 + §4.4)

- `OlympusClient::tenant() -> TenantApi<'_>` — canonical `/tenant/*` surface
  for signup, current-tenant read/patch, retire/unretire, multi-tenant
  listing, and cross-tenant switch. Wires to PR #3410's
  `tenant_lifecycle` handler.
  - `TenantApi::create(TenantCreateRequest)` — idempotent self-service
    tenant provisioning (24h window on `idempotency_key`).
  - `TenantApi::current()` / `TenantApi::update(TenantUpdate)` — read and
    patch the tenant scoped by the current session.
  - `TenantApi::retire(&str)` / `TenantApi::retire_with_reason(&str,
    Option<&str>)` — MFA'd soft-delete with typed `confirmation_slug`
    and 30-day grace window.
  - `TenantApi::unretire()` — reverse a retire within the grace window.
  - `TenantApi::my_tenants()` — every tenant the signed-in user can
    access.
  - `TenantApi::switch_tenant(&str) -> ExchangedSession` — chains
    `POST /tenant/switch` → `POST /auth/switch-tenant` and rotates the
    HTTP client's access + refresh tokens to the freshly minted pair.
- `OlympusClient::identity_invites() -> IdentityApi<'_>` — canonical
  `/identity/invite*` + `/identity/remove_from_tenant` surface. Distinct
  from the pre-existing `OlympusClient::identity()` accessor which wraps
  the global Olympus-ID / age-verification service.
  - `IdentityApi::invite(InviteCreateRequest)` — mint a signed invite
    token. `InviteHandle::token` populated only on the create response.
  - `IdentityApi::accept(&str, &str)` — POST
    `/identity/invites/:token/accept` with the caller's Firebase ID
    token. Returns the minted session payload as `serde_json::Value`
    (full `TokenResponse` shape lives in the auth service).
  - `IdentityApi::list()` — every invite for the caller's tenant
    (pending + accepted + revoked + expired), capped 500.
  - `IdentityApi::revoke(&str)` — flip a pending invite to revoked.
  - `IdentityApi::remove_from_tenant(&str, Option<&str>)` — remove a
    user from the tenant while preserving their Firebase identity.
- New public types at the crate root: `Tenant`, `TenantCreateRequest`,
  `TenantFirstAdmin`, `TenantUpdate`, `TenantProvisionResult`,
  `TenantOption`, `TenantAppInstall` (renamed from the previous
  `AppInstall` — see `AppsApi` section above), `ExchangedSession`,
  `InviteCreateRequest`, `InviteHandle`, `InviteStatus`,
  `RemoveFromTenantResponse`.

### Silent token refresh + broadcast SessionEvents (#3403 §1.4 / #3412)

- `OlympusClient::start_silent_refresh(refresh_margin: Duration) -> SilentRefreshHandle`
  — spawns a `tokio` task that sleeps until `exp - refresh_margin` (decoded
  from the current access token's JWT), POSTs `/auth/refresh`, and swaps
  the access token on success. Idempotent — a second call aborts the
  prior task before spawning.
- `OlympusClient::stop_silent_refresh()` — silent cancellation of the
  current task. Emits no event.
- `OlympusClient::logout()` — aborts the silent-refresh task, clears the
  access + refresh tokens, and broadcasts `SessionEvent::LoggedOut`.
- `OlympusClient::session_events()` — returns a
  `tokio::sync::broadcast::Receiver<SessionEvent>` for observing session
  lifecycle transitions. Channel capacity 32; created once per client and
  reused across start/stop cycles.
- `OlympusClient::emit_logged_in(session)` — emit a `LoggedIn` transition
  after completing a login flow outside the SDK.
- `OlympusClient::set_refresh_token` / `clear_refresh_token` — manage the
  refresh token used by the silent-refresh task.
- New `SessionEvent` enum: `LoggedIn(AuthSession)`, `Refreshed(AuthSession)`,
  `Expired { reason }`, `LoggedOut`.
- New `AuthSession` struct — minimal view of `/auth/login` + `/auth/refresh`
  response bodies (`access_token`, `refresh_token`, `expires_at`,
  `token_type`, `user_id`, `tenant_id`).
- New `SilentRefreshHandle` — returned from `start_silent_refresh`; aborts
  the task on `Drop`.

### App-scoped permissions — string-keyed scope helpers (#3403 §1.2)

- `OlympusClient::has_scope(&str) -> bool` — string-keyed complement to the
  existing bitset fast-path `has_scope_bit(usize)`.
- `OlympusClient::require_scope(&str) -> Result<()>` — client-side precheck
  returning `OlympusError::ScopeRequired { scope }` on miss. Distinct from
  server-side `ScopeDenied` / `ConsentRequired` which are still returned by
  the HTTP layer on 403 responses.
- `OlympusClient::granted_scopes() -> HashSet<String>` — decoded from the
  `app_scopes` JWT claim (array of canonical scope strings, per §7.1).
- New `OlympusError::ScopeRequired { scope }` variant.
- Generated constants under `olympus_sdk::constants` (re-exported at the
  crate root as `OlympusScopes` and `OlympusRoles`), produced from
  `docs/platform/{scopes,roles}.yaml` via
  `scripts/generate_sdk_scope_constants.py`. Do not hand-edit the generated
  files.

## 0.5.0 (2026-04-19)

### Wave 2 of the SDK 1.0 Campaign (OlympusCloud/olympus-cloud-gcp#3216)

Dart-parity port of voice + identity + smart-home + sms + voice-orders.
All Wave 1 signatures preserved; new methods are additive.

**New services:**

- `client.identity()` — global, cross-tenant Olympus ID + age verification
  (`OlympusIdentity`, `IdentityLink` typed). Routes:
  `/platform/identities`, `/platform/identities/links`, `/identity/scan-id`,
  `/identity/status/{phone}`, `/identity/{verify,set}-passphrase`,
  `/identity/create-upload-session`.
- `client.smart_home()` — consumer smart-home: platforms, devices, rooms,
  scenes, automations. Routes: `/smart-home/*`.
- `client.sms()` — SMS send + delivery via the CPaaS abstraction
  (Telnyx-primary / Twilio-fallback). Routes: `/voice/sms/*`,
  `/cpaas/messages/*`.

**Voice service expansion (~40 new methods):**

- Agent CRUD: `list_configs`, `get_config`, `create_config`,
  `update_config`, `delete_config`, `create_agent`, `update_agent`,
  `clone_agent`, `preview_agent_voice`, `list_gemini_voices`,
  `list_agents`/`get_agent`/`delete_agent` aliases.
- Voice pool, schedule, provisioning wizard + status.
- Persona library: `list_personas`, `get_persona`,
  `apply_persona_to_agent`.
- Templates: `list_agent_templates`, `instantiate_agent_template`,
  `publish_agent_as_template`, `list_templates`.
- Background ambiance: `list_ambiance_library`, `upload_ambiance_bed`
  (base64-encoded), `update_agent_ambiance`,
  `update_agent_voice_overrides`.
- Workflow templates: `list/create/get/delete_workflow_template`,
  `create_workflow_instance`.
- Voicemail: `list_voicemails`, `update_voicemail`,
  `get_voicemail_audio_url`.
- Conversations + messages: `list_conversations`, `get_conversation`,
  `list_messages`.
- Analytics, campaigns, phone numbers (incl. port lifecycle).
- Marketplace voices + packs: `list_voices`, `get_my_voices`,
  `list_packs`, `get_pack`, `install_pack`.
- Calls: `end_call`. Speaker: `get_speaker_profile`, `enroll_speaker`,
  `add_words`. Profiles: `list/get/create/update_profile`.
- Edge voice pipeline: `process_audio` (base64-encoded),
  `pipeline_health`, `get_voice_web_socket_url(session_id)` →
  `wss://…/ws/voice` URL helper.
- Caller profiles (#2868): `get/list/upsert/delete_caller_profile`,
  `record_caller_order`.
- Escalation + business hours (#2870): `get/update_escalation_config`,
  `get/update_business_hours`.
- Agent testing (#170): `test_agent`.

**Voice orders:** added `create_raw(order: Value)` for dart parity
alongside the existing typed `create(...)`.

**Client surface:**

- New typed `OlympusClient` accessors: `consent()`, `governance()`,
  `identity()`, `smart_home()`, `sms()`.
- New token + scope helpers on `OlympusClient`: `set_access_token`,
  `clear_access_token`, `set_app_token`, `clear_app_token`,
  `on_catalog_stale`, `is_app_scoped`, `has_scope_bit(bit)`. Closes
  the gap that left `tests/app_scoped_permissions.rs` referencing
  unimplemented methods on `main`.
- New `OlympusHttpClient::config()` accessor (read-only) —
  required by `VoiceService::get_voice_web_socket_url`.

**Tests:** 95/95 passing across 8 test binaries
(8 lib + 11 app-scoped + 8 identity + 9 smart-home + 7 sms +
7 voice-orders + 43 voice + 2 doc).

**Convention notes (deviations called out for reviewers):**

- List methods return `Result<Value>` (full envelope), not
  `Result<Vec<Value>>`. Matches the pre-existing convention used by
  every other service in this crate (commerce, agent_workflows, …).
- Most endpoint responses outside V2-005 are returned as
  `serde_json::Value` (mirrors dart's `Map<String, dynamic>`); only
  `OlympusIdentity` + `IdentityLink` carry typed shapes.
- `IdentityService::scan_id` faithfully reproduces dart's
  `List<int>`-as-JSON-array shape (dart comments call this out as
  multipart-but-not-actually).

## 0.4.0 (2026-04-18)

### Wave 1 of the SDK 1.0 Campaign (OlympusCloud/olympus-cloud-gcp#3216, Wave #3217)

**New services:**

- `client.voice()` — Voice AI with V2-005 cascade resolver (#3162).
- `client.connect()` — marketing-funnel + pre-conversion lead capture
  (#3108).

**New methods:**

- `client.voice().get_effective_config(agent_id).await` →
  `VoiceEffectiveConfig`. Backing endpoint
  `GET /api/v1/voice-agents/configs/{id}/effective-config`.
- `client.voice().get_pipeline(agent_id).await` → `VoicePipeline`.
  Canonical subset for runtimes / provisioners.
- `client.connect().create_lead(&req).await` → `CreateLeadResponse`.
  Unauthenticated; idempotent on email over 1h.

**New types:** `VoiceEffectiveConfig`, `VoicePipeline`,
`VoiceDefaultsCascade`, `VoiceDefaultsRung`, `UTM`, `CreateLeadRequest`,
`CreateLeadResponse`. All derive `Serialize` + `Deserialize` (serde).

**Deferred from Wave 1:**

- `client.auth().create_service_token(...)` — endpoint #2848 exists in
  Rust auth but is not routed through the Go gateway. Tracked in platform
  issue OlympusCloud/olympus-cloud-gcp#3220. Wave 1.5.
- Identity / training coverage — Wave 2 per campaign doc §2.

**Tests:** First tests ever in this crate. `cargo test --lib` → 8/8
passing. Fixtures are real captures from dev.api.olympuscloud.ai —
same as olympus-sdk-dart#8, olympus-sdk-typescript#1, olympus-sdk-go#1,
olympus-sdk-python#1.
