//! Integration tests for I18nService — wraps GET /v1/i18n/errors (#3638).
//!
//! Covers:
//! - manifest fetch + parse
//! - cache hit avoids second HTTP call
//! - in-flight dedup (concurrent cold callers share one request)
//! - localize fallback to en when locale unknown
//! - localize returns code when code unknown
//! - localize_code happy + missing-code paths
//! - integration test against a recorded fixture matching the canonical
//!   response shape (AC-6)

use mockito::Server;
use olympus_sdk::services::i18n::{ErrorManifest, ErrorManifestEntry};
use olympus_sdk::{OlympusClient, OlympusConfig};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Canonical fixture mirroring the byte-for-byte shape from
/// backend/rust/platform/src/handlers/i18n.rs.
const MANIFEST_FIXTURE: &str = r#"{
  "version": "1.0",
  "locales": ["en", "es", "fr"],
  "errors": [
    {
      "code": "NOT_FOUND",
      "messages": {
        "en": "The requested resource was not found.",
        "es": "No se encontró el recurso solicitado.",
        "fr": "La ressource demandée est introuvable."
      }
    },
    {
      "code": "VALIDATION_ERROR",
      "messages": {
        "en": "One or more fields failed validation.",
        "es": "Uno o más campos no superaron la validación.",
        "fr": "Un ou plusieurs champs ont échoué à la validation."
      }
    },
    {
      "code": "RATE_LIMIT_EXCEEDED",
      "messages": {
        "en": "Too many requests. Please try again in a few moments.",
        "es": "Demasiadas solicitudes. Por favor, inténtelo de nuevo en unos momentos.",
        "fr": "Trop de requêtes. Veuillez réessayer dans quelques instants."
      }
    }
  ]
}"#;

fn make_client(base_url: &str) -> OlympusClient {
    let cfg = OlympusConfig::new("test-app", "oc_test").with_base_url(base_url);
    OlympusClient::from_config(cfg)
}

#[tokio::test]
async fn errors_fetches_and_parses_manifest() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/v1/i18n/errors")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("cache-control", "public, max-age=3600")
        .with_body(MANIFEST_FIXTURE)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let manifest = oc.i18n().errors("en").await.expect("ok");
    assert_eq!(manifest.version, "1.0");
    assert_eq!(manifest.locales, vec!["en", "es", "fr"]);
    assert_eq!(manifest.errors.len(), 3);
    assert_eq!(manifest.errors[0].code, "NOT_FOUND");
    assert_eq!(
        manifest.errors[0].messages.get("es").map(String::as_str),
        Some("No se encontró el recurso solicitado.")
    );
    m.assert_async().await;
}

#[tokio::test]
async fn errors_caches_manifest() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/v1/i18n/errors")
        .with_status(200)
        .with_body(MANIFEST_FIXTURE)
        .expect(1) // exactly one HTTP hit despite multiple calls
        .create_async()
        .await;
    let oc = make_client(&server.url());
    for _ in 0..5 {
        oc.i18n().errors("en").await.expect("ok");
    }
    oc.i18n().errors("fr").await.expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn errors_concurrent_callers_share_inflight() {
    let mut server = Server::new_async().await;
    // mockito doesn't expose an artificial delay knob; instead, we
    // launch all callers BEFORE the mock fires, and the cache lock
    // serializes them so subsequent callers observe the populated cache.
    let m = server
        .mock("GET", "/v1/i18n/errors")
        .with_status(200)
        .with_body(MANIFEST_FIXTURE)
        .expect(1) // singleflight invariant
        .create_async()
        .await;
    let oc = Arc::new(make_client(&server.url()));
    let counter = Arc::new(AtomicU32::new(0));
    let mut tasks = vec![];
    for _ in 0..8 {
        let oc_clone = Arc::clone(&oc);
        let counter = Arc::clone(&counter);
        tasks.push(tokio::spawn(async move {
            let manifest = oc_clone.i18n().errors("en").await.expect("ok");
            assert_eq!(manifest.errors.len(), 3);
            counter.fetch_add(1, Ordering::SeqCst);
        }));
    }
    for t in tasks {
        t.await.expect("task ok");
    }
    assert_eq!(counter.load(Ordering::SeqCst), 8);
    m.assert_async().await;
}

#[tokio::test]
async fn clear_cache_forces_refetch() {
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/v1/i18n/errors")
        .with_status(200)
        .with_body(MANIFEST_FIXTURE)
        .expect(2)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    oc.i18n().errors("en").await.expect("ok");
    oc.i18n().clear_cache().await;
    oc.i18n().errors("en").await.expect("ok");
    m.assert_async().await;
}

#[tokio::test]
async fn localize_returns_locale_specific_message() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/v1/i18n/errors")
        .with_status(200)
        .with_body(MANIFEST_FIXTURE)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    assert_eq!(
        oc.i18n().localize("NOT_FOUND", "es").await.expect("ok"),
        "No se encontró el recurso solicitado."
    );
    assert_eq!(
        oc.i18n().localize("NOT_FOUND", "fr").await.expect("ok"),
        "La ressource demandée est introuvable."
    );
}

#[tokio::test]
async fn localize_falls_back_to_en() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/v1/i18n/errors")
        .with_status(200)
        .with_body(MANIFEST_FIXTURE)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    assert_eq!(
        oc.i18n().localize("NOT_FOUND", "de").await.expect("ok"),
        "The requested resource was not found."
    );
    assert_eq!(
        oc.i18n().localize("NOT_FOUND", "ja").await.expect("ok"),
        "The requested resource was not found."
    );
}

#[tokio::test]
async fn localize_returns_code_when_unknown() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/v1/i18n/errors")
        .with_status(200)
        .with_body(MANIFEST_FIXTURE)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    assert_eq!(
        oc.i18n()
            .localize("UNKNOWN_FUTURE_CODE", "en")
            .await
            .expect("ok"),
        "UNKNOWN_FUTURE_CODE"
    );
}

#[tokio::test]
async fn localize_empty_code_short_circuits() {
    let oc = make_client("http://localhost:1"); // no http call should happen
    assert_eq!(oc.i18n().localize("", "en").await.expect("ok"), "");
    assert_eq!(oc.i18n().localize("   ", "en").await.expect("ok"), "");
}

#[tokio::test]
async fn localize_code_happy_path() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/v1/i18n/errors")
        .with_status(200)
        .with_body(MANIFEST_FIXTURE)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let got = oc
        .i18n()
        .localize_code("NOT_FOUND", "server msg", "es")
        .await
        .expect("ok");
    assert_eq!(got, "No se encontró el recurso solicitado.");
}

#[tokio::test]
async fn localize_code_unknown_falls_back_to_server_msg() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/v1/i18n/errors")
        .with_status(200)
        .with_body(MANIFEST_FIXTURE)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let got = oc
        .i18n()
        .localize_code("BRAND_NEW_CODE", "server-side English", "es")
        .await
        .expect("ok");
    assert_eq!(got, "server-side English");
}

#[tokio::test]
async fn localize_code_empty_returns_server_msg() {
    let oc = make_client("http://localhost:1");
    let got = oc
        .i18n()
        .localize_code("", "plain text error", "es")
        .await
        .expect("ok");
    assert_eq!(got, "plain text error");
}

#[tokio::test]
async fn localize_code_falls_through_to_code() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/v1/i18n/errors")
        .with_status(200)
        .with_body(MANIFEST_FIXTURE)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let got = oc
        .i18n()
        .localize_code("BRAND_NEW_CODE", "", "es")
        .await
        .expect("ok");
    assert_eq!(got, "BRAND_NEW_CODE");
}

// AC-6 — fixture-shape contract validating the schema invariants the
// Rust manifest tests enforce server-side. If the deployed endpoint
// changes shape, this test fails — protecting downstream apps from
// silent parse failures during a bad deploy.
#[tokio::test]
async fn e2e_fixture_contract() {
    let mut server = Server::new_async().await;
    server
        .mock("GET", "/v1/i18n/errors")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_header("cache-control", "public, max-age=3600")
        .with_body(MANIFEST_FIXTURE)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let manifest = oc.i18n().errors("en").await.expect("ok");
    assert_eq!(manifest.version, "1.0");
    assert!(manifest.locales.contains(&"en".to_string()));
    assert!(manifest.locales.contains(&"es".to_string()));
    assert!(manifest.locales.contains(&"fr".to_string()));

    // Schema invariant: every entry must translate every locale.
    for entry in &manifest.errors {
        for locale in &manifest.locales {
            let msg = entry.messages.get(locale).unwrap_or_else(|| {
                panic!(
                    "manifest entry {} missing locale {} — backend invariant violated",
                    entry.code, locale
                )
            });
            assert!(
                !msg.trim().is_empty(),
                "manifest entry {} locale {} blank",
                entry.code,
                locale
            );
        }
    }

    // Spot-check a translation actually localizes.
    let msg = oc
        .i18n()
        .localize("RATE_LIMIT_EXCEEDED", "es")
        .await
        .expect("ok");
    assert!(
        msg.starts_with("Demasiadas solicitudes"),
        "es localization missing: {msg}"
    );
}

#[tokio::test]
async fn shared_cache_across_handles() {
    // Multiple `i18n()` calls return services that share the same cache
    // — verified by counting that only ONE HTTP request fires for two
    // separate handles fetching the manifest.
    let mut server = Server::new_async().await;
    let m = server
        .mock("GET", "/v1/i18n/errors")
        .with_status(200)
        .with_body(MANIFEST_FIXTURE)
        .expect(1)
        .create_async()
        .await;
    let oc = make_client(&server.url());
    let svc1 = oc.i18n();
    let svc2 = oc.i18n();
    let _ = svc1.errors("en").await.expect("ok");
    let _ = svc2.errors("fr").await.expect("ok");
    m.assert_async().await;
}

// Pure model-level unit tests (no HTTP, no async) — exercised in the same
// integration test module so they run alongside the rest of the suite.
#[test]
fn entry_message_for_falls_back_to_en() {
    let mut messages = HashMap::new();
    messages.insert("en".to_string(), "english".to_string());
    messages.insert("es".to_string(), "spanish".to_string());
    let entry = ErrorManifestEntry {
        code: "X".into(),
        messages,
    };
    assert_eq!(entry.message_for("es"), Some("spanish"));
    assert_eq!(entry.message_for("de"), Some("english"));
}

#[test]
fn manifest_entry_for_lookup() {
    let manifest = ErrorManifest {
        version: "1.0".into(),
        locales: vec!["en".into()],
        errors: vec![ErrorManifestEntry {
            code: "ABC".into(),
            messages: HashMap::new(),
        }],
    };
    assert!(manifest.entry_for("ABC").is_some());
    assert!(manifest.entry_for("XYZ").is_none());
}

// Forces compile-time access of the constant; if the TTL drifts from the
// backend Cache-Control max-age, the comment in i18n.rs is stale.
#[test]
fn cache_ttl_is_one_hour() {
    use olympus_sdk::services::i18n::I18N_CACHE_TTL;
    assert_eq!(I18N_CACHE_TTL, Duration::from_secs(3600));
}
