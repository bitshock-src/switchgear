//! OTLP-compliance snapshot tests.
//!
//! Wires a per-test in-process OTLP gRPC collector into swgr's tracing
//! pipeline, exercises a success and an error scenario, then asserts the
//! collector's on-wire span tree via `insta`. Unlike the Jaeger-backed
//! assertions in `service_logs.rs`, this asserts exactly what swgr put on
//! the OTLP wire — Jaeger's v1 query API mutates the shape (drops per-span
//! Resource, adds `otel.scope.*` tags, renames Event.name → `event`, …), so
//! only these tests catch OTLP-shape drift.
//!
//! On a snapshot diff: run `cargo insta review`, audit that the change is
//! intentional (usually a `tracing-opentelemetry` / `opentelemetry-otlp`
//! bump or an intentional emit-shape change), then accept.

use crate::FEATURE_TEST_CONFIG_PATH;
use crate::common::context::Protocol;
use crate::common::context::global::GlobalContext;
use crate::common::context::server::CertificateLocation;
use crate::common::otlp_collector::TestOtlpCollector;
use crate::common::step_functions::*;
use std::path::PathBuf;
use switchgear_testing::credentials::otel::OtelCollector;
use tempfile::TempDir;

/// Happy-path OTLP snapshot: `GET /offers/{partition}/{id}/invoice` returns
/// 200. Asserts the collector's exhaustive span tree (attrs + child spans +
/// events) for every root span the server emitted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_success_invoice_otlp_shape() {
    const ROOT_SPAN_WHITELIST: &[&str] = &["GET /offers/{partition}/{id}/invoice"];

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");

    let collector_dir = TempDir::new().expect("assert");
    let collector =
        TestOtlpCollector::spawn(collector_dir.path(), ROOT_SPAN_WHITELIST).expect("assert");

    let server1 = "server1";
    let config_path = manifest_dir.join("config/memory-basic.yaml");
    ctx.add_server(
        server1,
        config_path,
        Protocol::Https,
        Protocol::Https,
        Protocol::Https,
    )
    .expect("assert");
    ctx.activate_server(server1);

    // ca_cert_path stays pointed at the existing PKI root: swgr's OTLP
    // client only consults it for https:// endpoints, and our collector is
    // http:// (h2c). Path just needs to exist so `OTLP_TRUSTED_ROOTS` is a
    // readable file when the child reads it.
    let ca_cert_path = ctx.get_pki_root_certificate_path().to_path_buf();
    ctx.set_active_otel_collector(OtelCollector {
        grpc_endpoint: collector.endpoint().to_string(),
        jaeger_query_endpoint: String::new(),
        ca_cert_path,
        bearer_token_path: collector.bearer_token_path().to_path_buf(),
        client_cert_path: None,
        client_key_path: None,
    })
    .expect("assert");

    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
        .await
        .expect("assert");
    step_given_the_payee_has_a_lightning_node_available(&mut ctx, "cln")
        .await
        .expect("assert");
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_the_lnurl_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");
    step_and_the_discovery_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");
    step_and_the_offers_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");

    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_the_payee_registers_their_lightning_node_as_a_backend(&mut ctx, "single", true)
        .await
        .expect("assert");
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url(
        &mut ctx,
        "single",
        &Protocol::Https,
    )
    .await
    .expect("assert");

    // SIGTERM flushes the OTLP BatchExporter shutdown path, guaranteeing the
    // collector sees every buffered span before we ask for the log.
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    let log_path = collector.shutdown().await.expect("assert");
    ctx.stop_all_servers().expect("assert");

    let lines = read_log_lines(&log_path);
    insta::assert_json_snapshot!("otlp_spans__success_invoice", lines);
}

/// Error-path OTLP snapshot: two 404s from the LnUrl service exercise
/// `LnUrlPayServiceError::emit_event`. Kept small (one path, two calls) so
/// the snapshot stays legible; broader coverage lives in
/// `service_logs.rs::test_error_conditions_are_properly_logged` (via Jaeger).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_error_get_offer_404_otlp_shape() {
    // Both 404 scenarios (non-existent partition, non-existent offer id) hit
    // the same axum route; the two calls surface as two sibling root spans
    // with the same name.
    const ROOT_SPAN_WHITELIST: &[&str] = &["GET /offers/{partition}/{id}"];

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");

    let collector_dir = TempDir::new().expect("assert");
    let collector =
        TestOtlpCollector::spawn(collector_dir.path(), ROOT_SPAN_WHITELIST).expect("assert");

    let server1 = "server1";
    let config_path = manifest_dir.join("config/memory-basic.yaml");
    ctx.add_server(
        server1,
        config_path,
        Protocol::Https,
        Protocol::Https,
        Protocol::Https,
    )
    .expect("assert");
    ctx.activate_server(server1);

    // ca_cert_path stays pointed at the existing PKI root: swgr's OTLP
    // client only consults it for https:// endpoints, and our collector is
    // http:// (h2c). Path just needs to exist so `OTLP_TRUSTED_ROOTS` is a
    // readable file when the child reads it.
    let ca_cert_path = ctx.get_pki_root_certificate_path().to_path_buf();
    ctx.set_active_otel_collector(OtelCollector {
        grpc_endpoint: collector.endpoint().to_string(),
        jaeger_query_endpoint: String::new(),
        ca_cert_path,
        bearer_token_path: collector.bearer_token_path().to_path_buf(),
        client_cert_path: None,
        client_key_path: None,
    })
    .expect("assert");

    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
        .await
        .expect("assert");
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_the_lnurl_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");

    step_when_i_request_an_offer_from_a_non_existent_partition(&mut ctx)
        .await
        .expect("assert");
    step_when_i_request_an_invoice_for_a_non_existent_offer(&mut ctx)
        .await
        .expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    let log_path = collector.shutdown().await.expect("assert");
    ctx.stop_all_servers().expect("assert");

    let lines = read_log_lines(&log_path);
    insta::assert_json_snapshot!("otlp_spans__error_get_offer_404", lines);
}

/// 5xx path OTLP snapshot: `GET /offers/{partition}/{id}` against an HTTP
/// offer store pointed at a dead port → reqwest transport failure →
/// HTTP 502.
///
/// Purpose: prove the axum-tracing-opentelemetry root-span
/// status-wiring survives our topology-only OTLP surface. Per OTel HTTP
/// semconv, 5xx responses MUST set `Span.status.code = ERROR` on the
/// SERVER root span (and only 5xx — 4xx are client faults and MUST
/// leave status UNSET, per
/// https://opentelemetry.io/docs/specs/semconv/http/http-spans/#status).
///
/// The wiring is done by
/// `tracing_opentelemetry_instrumentation_sdk::http::http_server::update_span_from_response`
/// invoked by `OtelAxumLayer`'s response handler, which records
/// `otel.status_code = "ERROR"` on the root span via the span handle
/// the layer holds. Our snapshot's `status` metadata field should show
/// `STATUS_CODE_ERROR` on the root span specifically.
///
/// Chosen over the invoice/no-backend 502 scenario because that path
/// runs through the pingora balancer retry loop, producing N sibling
/// `select_backend` spans (N depends on wall-clock backoff timing).
/// This dead-store scenario fails on the first (and only) reqwest
/// connect attempt — no retries, clean single-request tree.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_dead_offer_store_502_otlp_shape() {
    const ROOT_SPAN_WHITELIST: &[&str] = &["GET /offers/{partition}/{id}"];

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");

    let collector_dir = TempDir::new().expect("assert");
    let collector =
        TestOtlpCollector::spawn(collector_dir.path(), ROOT_SPAN_WHITELIST).expect("assert");

    // Standalone-lnurl config so the offer store is HTTP (not memory).
    let server1 = "server1";
    let config_path = manifest_dir.join("config/lnurl-standalone.yaml");
    ctx.add_server(
        server1,
        config_path,
        Protocol::Https,
        Protocol::Https,
        Protocol::Https,
    )
    .expect("assert");
    ctx.activate_server(server1);

    let ca_cert_path = ctx.get_pki_root_certificate_path().to_path_buf();
    ctx.set_active_otel_collector(OtelCollector {
        grpc_endpoint: collector.endpoint().to_string(),
        jaeger_query_endpoint: String::new(),
        ca_cert_path,
        bearer_token_path: collector.bearer_token_path().to_path_buf(),
        client_cert_path: None,
        client_key_path: None,
    })
    .expect("assert");

    // Point the HTTP stores at dead ports. Discovery URL doesn't matter
    // for the offer path but the config requires it to be set (and the
    // token file must exist for the config to load), so wire the auth
    // pointers to the server's own on-disk token files.
    ctx.set_active_offer_store_url_raw(Some("https://127.0.0.1:1".to_string()))
        .expect("assert");
    ctx.set_active_discovery_store_url_raw(Some("https://127.0.0.1:1".to_string()))
        .expect("assert");
    ctx.set_offer_store_authorization(server1, server1)
        .expect("assert");
    ctx.set_discovery_store_authorization(server1, server1)
        .expect("assert");
    ctx.set_certificate_location(server1, CertificateLocation::Env)
        .expect("assert");

    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
        .await
        .expect("assert");
    // lnurl-standalone config only defines the lnurl service.
    step_when_i_start_server_2_with_only_lnurl_service(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_the_lnurl_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");

    // Any partition/id — the HTTP store's request never reaches a real
    // server to check whether the offer exists.
    step_when_i_request_offer_expecting_failure(&mut ctx, "default")
        .await
        .expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    let log_path = collector.shutdown().await.expect("assert");
    ctx.stop_all_servers().expect("assert");

    let lines = read_log_lines(&log_path);
    insta::assert_json_snapshot!("otlp_spans__dead_offer_store_502", lines);
}

fn read_log_lines(path: &std::path::Path) -> Vec<serde_json::Value> {
    let contents = std::fs::read_to_string(path).expect("read otlp log");
    contents
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("otlp log line is valid JSON"))
        .collect()
}
