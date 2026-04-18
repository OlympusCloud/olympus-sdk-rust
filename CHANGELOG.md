# Changelog

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
