//! ECS-compliance snapshot tests.
//!
//! Captures the child server's `stderr` — where `tracing-ecs-formatter`
//! writes ECS JSON records verbatim — and asserts on the reduced/redacted
//! wire shape via `insta`. Unlike the `EcsRequestFilter`-based whitelists
//! in `service_logs.rs`, this catches drift the whitelists cannot see:
//! renamed keys, dropped keys, changed value types, or extra unexpected
//! fields on the wire.
//!
//! On a snapshot diff: run `cargo insta review`, audit that the change is
//! intentional (usually a `tracing-ecs-formatter` bump, an ECS-spec version
//! bump, or a call-site attribute-set change), then accept.

use crate::FEATURE_TEST_CONFIG_PATH;
use crate::common::context::Protocol;
use crate::common::context::global::GlobalContext;
use crate::common::context::server::CertificateLocation;
use crate::common::ecs_reducer::{EcsFilter, EcsReducer};
use crate::common::step_functions::*;
use std::path::PathBuf;

/// Happy-path ECS snapshot: `GET /offers/{partition}/{id}/invoice` returns
/// 200. Asserts the single INFO access-log line `RequestLogger` emits with
/// its full attribute set on the wire.
#[tokio::test]
async fn test_ecs_success_invoice() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");
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

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    let reduced = reduce_active_stderr(
        &mut ctx,
        &EcsFilter {
            service_names: &["swgr.lnurl"],
            levels: Some(&["INFO"]),
            url_path_prefixes: Some(&["/offers/default/"]),
            error_types: None,
        },
    );
    insta::assert_json_snapshot!("ecs_logs__success_invoice", reduced);

    ctx.stop_all_servers().expect("assert");
}

/// Error HTTP-request ECS snapshot: 404 from a non-existent partition
/// surfaces as a two-line pair — access-log INFO + error-log WARN. The
/// snapshot captures both so a regression that suppresses either surfaces
/// in the diff.
#[tokio::test]
async fn test_ecs_error_http_request() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");
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

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    let reduced = reduce_active_stderr(
        &mut ctx,
        &EcsFilter {
            service_names: &["swgr.lnurl"],
            levels: Some(&["INFO", "WARN"]),
            url_path_prefixes: Some(&["/offers/non-existent-partition/"]),
            error_types: Some(&["switchgear_service::lnurl::pay::error::LnUrlPayServiceError"]),
        },
    );
    insta::assert_json_snapshot!("ecs_logs__error_http_request", reduced);

    ctx.stop_all_servers().expect("assert");
}

/// Non-HTTP-shaped error ECS snapshot: dead offer-store URL trips the
/// upstream `CrudError::boxed_error` chain, producing a 502 with an ERROR
/// record carrying the full origin + stack-trace triple. Also asserts the
/// accompanying INFO access line and any WARN records the same request
/// emits — bidirectional coverage of both `CrudError` and
/// `LnUrlPayServiceError` emitters.
#[tokio::test]
async fn test_ecs_error_log() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");

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
    step_when_i_start_server_2_with_only_lnurl_service(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_the_lnurl_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");

    step_when_i_request_offer_expecting_failure(&mut ctx, "default")
        .await
        .expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    let reduced = reduce_active_stderr(
        &mut ctx,
        &EcsFilter {
            service_names: &["swgr.lnurl"],
            levels: Some(&["ERROR", "INFO", "WARN"]),
            url_path_prefixes: None,
            error_types: Some(&[
                "switchgear_service::axum::crud::error::CrudError",
                "switchgear_service::lnurl::pay::error::LnUrlPayServiceError",
            ]),
        },
    );
    insta::assert_json_snapshot!("ecs_logs__error_log", reduced);

    ctx.stop_all_servers().expect("assert");
}

fn reduce_active_stderr(ctx: &mut GlobalContext, filter: &EcsFilter<'_>) -> Vec<serde_json::Value> {
    let stderr = ctx
        .get_active_stderr_buffer()
        .expect("stderr buffer")
        .lock()
        .map(|l| l.clone())
        .expect("lock stderr buffer");
    EcsReducer::reduce(&stderr, filter)
}

fn key_orders_active_stderr(ctx: &mut GlobalContext, filter: &EcsFilter<'_>) -> Vec<Vec<String>> {
    let stderr = ctx
        .get_active_stderr_buffer()
        .expect("stderr buffer")
        .lock()
        .map(|l| l.clone())
        .expect("lock stderr buffer");
    EcsReducer::key_orders(&stderr, filter)
}

/// ECS-logging spec: the first three keys on every ECS log line must be
/// `@timestamp`, `log.level`, `message`, and `ecs.version` is the fourth key
/// in the minimum-viable-log MVP set. `message` may be absent — in that
/// case the record is expected to have `@timestamp`, `log.level`,
/// `ecs.version` as the first three keys.
///
/// See https://github.com/elastic/ecs-logging/blob/main/spec/README.md.
///
/// Every ECS record swgr emits carries a message (the tracing event's
/// formatted message string), so the four-key prefix applies universally
/// here.
fn assert_ecs_logging_spec_order(records: &[Vec<String>], context: &str) {
    assert!(
        !records.is_empty(),
        "{context}: expected at least one ECS record matching the filter"
    );
    for (i, keys) in records.iter().enumerate() {
        assert!(
            keys.len() >= 4,
            "{context}: record {i} has only {} keys ({keys:?}), expected at least the MVP four",
            keys.len(),
        );
        assert_eq!(
            &keys[..4],
            &[
                "@timestamp".to_string(),
                "log.level".to_string(),
                "message".to_string(),
                "ecs.version".to_string(),
            ],
            "{context}: record {i} violates ECS-logging spec field order (got {keys:?})",
        );
    }
}

/// Wire-order guarantee for RequestLogger's INFO access lines. Uses the
/// same success-invoice scenario as `test_ecs_success_invoice`, but
/// asserts the ECS-logging spec's field-ordering requirement
/// (@timestamp, log.level, message, ecs.version as the first four keys)
/// via `EcsReducer::key_orders` instead of the type-redacted snapshot.
#[tokio::test]
async fn test_ecs_wire_order_info_access() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");
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

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    let orders = key_orders_active_stderr(
        &mut ctx,
        &EcsFilter {
            service_names: &["swgr.lnurl"],
            levels: Some(&["INFO"]),
            url_path_prefixes: Some(&["/offers/default/"]),
            error_types: None,
        },
    );
    assert_ecs_logging_spec_order(&orders, "RequestLogger INFO access");

    ctx.stop_all_servers().expect("assert");
}

/// Wire-order guarantee for the WARN error emitter
/// (`LnUrlPayServiceError::emit_event` 4xx branch,
/// `service/src/lnurl/pay/error.rs:94-102`). Same 404-partition scenario
/// as `test_ecs_error_http_request`, asserted at the field-order level.
#[tokio::test]
async fn test_ecs_wire_order_warn_error() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");
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

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    let orders = key_orders_active_stderr(
        &mut ctx,
        &EcsFilter {
            service_names: &["swgr.lnurl"],
            levels: Some(&["WARN"]),
            url_path_prefixes: None,
            error_types: Some(&["switchgear_service::lnurl::pay::error::LnUrlPayServiceError"]),
        },
    );
    assert_ecs_logging_spec_order(&orders, "LnUrlPayServiceError WARN");

    ctx.stop_all_servers().expect("assert");
}

/// Wire-order guarantee for the ERROR error emitter (5xx branch of the
/// error emitters, `service/src/axum/crud/error.rs:87-116` /
/// `service/src/lnurl/pay/error.rs:83-92`). Same dead-offer-store
/// scenario as `test_ecs_error_log`, asserted at the field-order level.
#[tokio::test]
async fn test_ecs_wire_order_error_upstream() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");

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
    step_when_i_start_server_2_with_only_lnurl_service(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_the_lnurl_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");

    step_when_i_request_offer_expecting_failure(&mut ctx, "default")
        .await
        .expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    let orders = key_orders_active_stderr(
        &mut ctx,
        &EcsFilter {
            service_names: &["swgr.lnurl"],
            levels: Some(&["ERROR"]),
            url_path_prefixes: None,
            error_types: Some(&[
                "switchgear_service::axum::crud::error::CrudError",
                "switchgear_service::lnurl::pay::error::LnUrlPayServiceError",
            ]),
        },
    );
    assert_ecs_logging_spec_order(&orders, "CrudError / LnUrlPayServiceError ERROR");

    ctx.stop_all_servers().expect("assert");
}
