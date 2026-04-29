# Changelog

All notable changes to `olympus-sdk` (Rust) will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 1.0.0-rc1 (2026-04-28)

First release candidate. Fully includes ASP fanout (Wave 14c — #3781 #3788 #3804 #3805 #3806 #3808 #3810 #3817), shadow-metrics surfaces, and per-app token mint/refresh.

No breaking changes from 0.x.

## [Unreleased]

### Added — App-Scoped Permissions Wave 14c (Epic #3234)

Additive surface fan-out for the App-Scoped Permissions epic. All new methods
match the canonical request/response shapes shipped on `olympus-cloud-gcp`
develop in PRs #3781 #3788 #3804 #3806 #3808 #3810.

- `AuthService::mint_app_token` — `POST /auth/app-tokens/mint` (#3781).
  Mints an App JWT pair from a Firebase App Check attestation.
- `AuthService::refresh_app_token` — `POST /auth/app-tokens/refresh` (#3781).
  Single-use refresh-token rotation with replay detection (refresh-family).
- `AuthService::get_app_jwks` — `GET /.well-known/app-keys/{app_id}` (#3788).
  Public JWKS lookup (RFC 8037 OKP/Ed25519) for an Olympus app's signing keys.
- `PlatformService::onboard_app` — `POST /platform/apps/onboard` (#3810).
  End-to-end app provisioning: manifest validate, `developer_apps` upsert,
  fresh `osk_*` API key issuance, and signing-key seed event publish.
- `PlatformService::submit_consent` — `POST /platform/authorize/consent` (#3804).
  Browser-flow consent submission. SDK captures the 303 `Location` and
  surfaces `grant_id` / `state` / `error` without following the deep link.
- `PlatformService::submit_grant` — `POST /platform/authorize/grant` (#3808).
  PKCE-required OAuth `/grant` form. Returns the auth code parsed out of
  the redirect for handoff to `exchange_authorization_code`.
- `PlatformService::exchange_authorization_code` — `POST /platform/authorize/exchange`
  (#3808). Trades a PKCE auth code for a 5-minute `mint_ticket` envelope.
- `PlatformService::get_grants_graph` — `GET /platform/admin/grants/graph` (#3806).
  Read-only grant graph projection plus HMAC-SHA256 signature for compliance
  traversals (never consulted for enforcement per design §0.4 E.1).

### Added — internals

- `OlympusHttpClient::post_form_no_redirect` — form-urlencoded POST that
  captures the response status, `Location` header, and body without
  following 3xx redirects. Backs the browser-flow `/consent` and `/grant`
  endpoints.
- `FormResponse` — public struct exposing `(status, location, body)` from
  the no-redirect form post.
- `chrono` and `uuid` are now first-class SDK dependencies (used by
  `MintTicket`, `GrantedEdge`, `GrantGraphSnapshot`).

### Notes

- Issue #3817 (`scope_upgrade`) ships only the server-side detector wired
  into the `/authorize` render path — there is **no public HTTP endpoint
  to wrap**, so no SDK method was added for it. The brief listed 8
  endpoints; this fanout ships the 7 that actually exist.
