use crate::FEATURE_TEST_CONFIG_PATH;
use crate::common::context::global::GlobalContext;
use crate::common::context::server::CertificateLocation;
use crate::common::context::{Protocol, Service};
use crate::common::error_fixture::{
    EcsLevel, ExpectedErrorContext, step_and_expected_error_should_be_logged,
};
use crate::common::helpers::{
    EcsRequestFilter, count_ecs_requests, matches_ecs_request, parse_ecs_line,
    read_active_stderr_lines,
};
use crate::common::step_functions::*;
use rand::Rng;
use secp256k1::{PublicKey, Secp256k1, SecretKey};
use std::path::PathBuf;

#[tokio::test]
async fn test_service_health_check_logging() {
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

    // Background
    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    // Scenario
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

    // Stop server and capture logs
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    // Assert logs after server stopped
    step_and_the_server_logs_should_contain_health_check_requests_for_all_services(&mut ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

#[tokio::test]
async fn test_service_operation_request_logging() {
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

    // Background
    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    // Scenario
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

    // Stop server and capture logs
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    // Assert logs after server stopped
    step_and_the_server_logs_should_contain_backend_registration_requests(&mut ctx)
        .await
        .expect("assert");
    step_and_the_server_logs_should_contain_offer_retrieval_requests(&mut ctx)
        .await
        .expect("assert");
    step_and_the_server_logs_should_contain_invoice_generation_requests(&mut ctx)
        .await
        .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// End-to-end verification of the success path:
///
/// 1. A successful invoice request produces an ECS `INFO` access log via
///    `RequestLogger` (GET /offers/default/{uuid}/invoice?amount=... → 200).
/// 2. Jaeger has a trace containing the `invoice` handler span with the root
///    HTTP status 200 and service.name `swgr.lnurl`.
/// 3. None of the spans in that trace carry a span event named `"request"` —
///    the target-based filter on `OpenTelemetryLayer` must suppress the
///    `RequestLogger` summary event so it does not duplicate as an OTLP span
///    event on the enclosing request span.
#[tokio::test]
async fn test_success_invoice_ecs_and_traces_without_duplicate_span_event() {
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

    // ECS: RequestLogger summary line for the invoice call.
    step_and_the_server_logs_should_contain_invoice_generation_requests(&mut ctx)
        .await
        .expect("assert");

    // Extract trace.id / span.id from the RequestLogger's ECS invoice-request
    // access line so we can prove log↔trace correlation.
    let (ecs_trace_id, ecs_span_id) = extract_invoice_ecs_correlation(&mut ctx).expect("assert");
    step_and_jaeger_root_span_should_match_ecs_correlation(
        &mut ctx,
        "invoice",
        "swgr.lnurl",
        &ecs_trace_id,
        &ecs_span_id,
    )
    .await
    .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Locate the single ECS access-log line produced by `RequestLogger` for the
/// invoice endpoint and return its `trace.id` / `span.id` (populated by the ECS
/// formatter from the currently-active OTel span at emit time).
fn extract_invoice_ecs_correlation(ctx: &mut GlobalContext) -> anyhow::Result<(String, String)> {
    let stderr_lines = ctx
        .get_active_stderr_buffer()?
        .lock()
        .map_err(|_| anyhow::anyhow!("failed to lock stderr buffer"))?
        .clone();

    let filter = EcsRequestFilter {
        service: Some("lnurl"),
        method: Some("GET"),
        path_prefix: Some("/offers/default/"),
        query_prefix: Some("amount="),
        status: Some(200),
        level: Some("INFO"),
        ..Default::default()
    };

    let line = stderr_lines
        .iter()
        .find(|line| {
            if !matches_ecs_request(line, &filter) {
                return false;
            }
            parse_ecs_line(line)
                .and_then(|v| {
                    v.get("url.path")
                        .and_then(|x| x.as_str())
                        .map(str::to_string)
                })
                .is_some_and(|p| p.ends_with("/invoice"))
        })
        .ok_or_else(|| anyhow::anyhow!("invoice ECS access log line not found in stderr"))?;

    let v = parse_ecs_line(line)
        .ok_or_else(|| anyhow::anyhow!("failed to parse invoice ECS line as JSON"))?;
    let trace_id = v
        .get("trace.id")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow::anyhow!("trace.id missing from invoice ECS line"))?
        .to_string();
    let span_id = v
        .get("span.id")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow::anyhow!("span.id missing from invoice ECS line"))?
        .to_string();
    Ok((trace_id, span_id))
}

#[tokio::test]
async fn test_error_conditions_are_properly_logged() {
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

    // Background
    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");

    // Scenario
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

    let secp = Secp256k1::new();
    let mut rng = rand::thread_rng();

    let missing_backend_private_key =
        SecretKey::from_byte_array(rng.r#gen::<[u8; 32]>()).expect("assert");
    let missing_backend_public_key =
        PublicKey::from_secret_key(&secp, &missing_backend_private_key);

    step_when_i_try_to_get_a_missing_backend(&mut ctx, &missing_backend_public_key)
        .await
        .expect("assert");

    // Stop server and capture logs
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    // Assert logs after server stopped
    step_and_the_server_logs_should_contain_404_error_responses(&mut ctx)
        .await
        .expect("assert");
    step_and_the_server_logs_should_contain_invalid_offer_error_responses(&mut ctx)
        .await
        .expect("assert");
    step_and_the_server_logs_should_contain_invalid_backend_get_errors(
        &mut ctx,
        &missing_backend_public_key,
    )
    .await
    .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Provokes PingoraLnError::no_available_nodes: invoice endpoint runs, offer
/// is fetched, then the balancer has no backend to route to and terminates at
/// pingora/src/balance.rs:222. Captured span is PingoraLnBalancer::get_invoice;
/// status 502.
#[tokio::test]
async fn test_invoice_no_available_nodes_error_logged_and_traced() {
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

    // Create the offer without registering a backend — the invoice request will
    // reach the balancer and fail with "no available nodes".
    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single")
        .await
        .expect("assert");
    step_but_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url_expecting_failure(
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

    step_and_the_server_logs_should_contain_invoice_no_backend_error(&mut ctx)
        .await
        .expect("assert");
    ctx.stop_all_servers().expect("assert");
}

/// Provokes LnUrlPayServiceError::bad_request: invoice handler validates
/// `amount` against the offer's [min_sendable, max_sendable] bounds and returns
/// bad_request at service/src/lnurl/pay/handler.rs:97-100. Captured span is
/// the invoice
/// handler; status 400.
#[tokio::test]
async fn test_invoice_out_of_range_amount_error_logged_and_traced() {
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

    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single")
        .await
        .expect("assert");
    // amount=1 is below min_sendable=1000 → bad_request.
    step_when_the_payer_requests_an_invoice_with_amount_expecting_failure(
        &mut ctx,
        "single",
        &Protocol::Https,
        1,
    )
    .await
    .expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    step_and_the_server_logs_should_contain_invoice_bad_amount_error(&mut ctx)
        .await
        .expect("assert");
    ctx.stop_all_servers().expect("assert");
}

/// Provokes CrudError::conflict via DiscoveryCrudError: registering the same
/// backend twice makes the second POST return None from the store, which the
/// handler at service/src/discovery/handler.rs:101 converts to a `conflict`
/// capture at service/src/axum/crud/error.rs:111. Captured span is the
/// post_backend handler; status 409.
#[tokio::test]
async fn test_duplicate_backend_post_conflict_logged_and_traced() {
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
    step_and_the_discovery_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");

    // First POST succeeds (201 CREATED); second POST for the same public_key
    // triggers the CONFLICT terminal.
    step_when_the_payee_registers_their_lightning_node_as_a_backend(&mut ctx, "single", true)
        .await
        .expect("assert");
    step_when_the_payee_registers_their_lightning_node_as_a_backend(&mut ctx, "single", true)
        .await
        .expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    step_and_the_server_logs_should_contain_backend_conflict_error(&mut ctx)
        .await
        .expect("assert");
    step_and_ecs_line_should_correlate_with_otel_root_span(
        &mut ctx,
        "discovery",
        "POST",
        "/discovery",
        409,
        "post_backend",
        "swgr.discovery",
    )
    .await
    .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Provokes CrudError::unauthorized (report §1 line 138) via the newly
/// instrumented `BearerTokenAuthService` reject path. Raw HTTP POST to
/// /discovery without a bearer header short-circuits at the auth middleware;
/// the `auth` span opened by `unauthorized_in_span` becomes the captured
/// span, so the rejection is now visible in Jaeger.
#[tokio::test]
async fn test_unauthorized_backend_post_logged_and_traced() {
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
    step_and_the_discovery_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");

    step_when_i_post_backend_without_auth(&mut ctx)
        .await
        .expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    step_and_the_server_logs_should_contain_unauthorized_backend_error(&mut ctx)
        .await
        .expect("assert");
    ctx.stop_all_servers().expect("assert");
}

/// Provokes OfferCrudError::bad → CrudError::bad (report §3 → §1 line 98) via
/// `get_offers` when the requested `count` exceeds max_page_size.
/// Captured span is the `get_offers` handler; status 400. Also asserts
/// ECS↔Jaeger correlation on `service.name = "swgr.offer"` — the only
/// place in the suite where the offer-service resource `service.name`
/// value is checked at the wire level (`otlp_spans.rs` type-redacts it).
#[tokio::test]
async fn test_get_offers_over_limit_logged_and_traced_bad_request() {
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
    step_and_the_offers_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");

    // memory-basic.yaml sets max-page-size = 100; ask for 10_000.
    step_when_i_get_offers_with_over_limit_count(&mut ctx, "default", 10_000)
        .await
        .expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    step_and_the_server_logs_should_contain_offers_bad_request_error(&mut ctx)
        .await
        .expect("assert");
    step_and_ecs_line_should_correlate_with_otel_root_span(
        &mut ctx,
        "offer",
        "GET",
        "/offers/",
        400,
        "get_offers",
        "swgr.offer",
    )
    .await
    .expect("assert");
    ctx.stop_all_servers().expect("assert");
}

/// Provokes LnUrlPayServiceError::not_found from PartitionsService's newly
/// instrumented reject path (report §2 line 76). The `partitions` span opened
/// by the middleware is the captured span, so the reject is now attributable
/// in Jaeger — previously it produced only a log event with no OTLP span.
#[tokio::test]
async fn test_non_existent_partition_reject_logged_and_traced() {
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

    // ECS content: LnUrlPayServiceError::not_found emits a WARN-level
    // ECS line for the partitions-middleware rejection.
    step_and_expected_error_should_be_logged(
        &mut ctx,
        ExpectedErrorContext {
            service: Service::LnUrl,
            method: "GET",
            path_prefix: "/offers/non-existent-partition/",
            status: 404,
            level: EcsLevel::Warn,
        },
    )
    .await
    .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Provokes DefaultOfferStoreError from reqwest transport failure (report §5).
/// Server1 runs lnurl-standalone (HTTP offer + discovery stores). The offer
/// store base URL is redirected to a dead port so the HttpOfferStore's request
/// fails at connect. Captured span is the innermost HTTP-store span in the
/// offer chain (`get_offer`); the request logger tags it with swgr.lnurl.
#[tokio::test]
async fn test_lnurl_with_dead_offer_store_url_logged_and_traced() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");

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
    // Point the HTTP stores at dead ports. Discovery URL doesn't matter for
    // the offer path but the config requires it to be set (and the token file
    // must exist for the config to load), so we also wire the auth pointers
    // to the server's own on-disk token files.
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
    // lnurl-standalone config only defines the lnurl service, so start only that.
    step_when_i_start_server_2_with_only_lnurl_service(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_the_lnurl_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");

    // Any partition/id — the HTTP store's request never reaches a real server
    // to check whether the offer exists.
    step_when_i_request_offer_expecting_failure(&mut ctx, "default")
        .await
        .expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    step_and_the_server_logs_should_contain_offer_upstream_error(&mut ctx)
        .await
        .expect("assert");
    ctx.stop_all_servers().expect("assert");
}

/// Provokes PingoraLnError via the retry-give-up path (report §8, terminating
/// at line 89 no_available_nodes after retries). A backend is registered but
/// its grpc URL points at a dead port, so the balancer keeps failing until
/// backoff exhausts. Distinct from the "no backends registered" test because
/// this exercises the discovery/pool code path with an actual DiscoveryBackend.
#[tokio::test]
async fn test_invoice_with_unreachable_backend_logged_and_traced() {
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

    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "single")
        .await
        .expect("assert");
    // Register the payee's node with a dead grpc URL. Health checks never pass
    // → select_backend returns None → no_available_nodes.
    register_payee_node_as_unreachable_backend(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single")
        .await
        .expect("assert");
    step_but_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url_expecting_failure(
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

    step_and_the_server_logs_should_contain_invoice_no_backend_error(&mut ctx)
        .await
        .expect("assert");
    ctx.stop_all_servers().expect("assert");
}

/// Extractor rejection: `UuidParam::from_request_parts` rejects `not-a-uuid`
/// before any handler runs. The root span opened by `OtelAxumLayer` still
/// records the 404 and RequestLogger emits an ECS event with status=404.
#[tokio::test]
async fn test_extractor_uuid_rejection_logged_and_traced() {
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

    step_when_i_get_offer_with_invalid_uuid(&mut ctx, "default")
        .await
        .expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    step_and_expected_error_should_be_logged(
        &mut ctx,
        ExpectedErrorContext {
            service: Service::LnUrl,
            method: "GET",
            path_prefix: "/offers/default/",
            status: 404,
            level: EcsLevel::Warn,
        },
    )
    .await
    .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Extractor rejection: axum's `Json` extractor rejects a malformed body with
/// 400 before the handler runs. Bearer is valid so we pass auth first, then
/// fail body decode.
#[tokio::test]
async fn test_extractor_json_rejection_logged_and_traced() {
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
    step_and_the_discovery_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");

    step_when_i_post_backend_with_malformed_json(&mut ctx)
        .await
        .expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    step_and_expected_error_should_be_logged(
        &mut ctx,
        ExpectedErrorContext {
            service: Service::Discovery,
            method: "POST",
            path_prefix: "/discovery",
            status: 400,
            level: EcsLevel::Warn,
        },
    )
    .await
    .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Extractor rejection: axum's `Query` extractor rejects `amount=abc` with 400.
/// The uuid is well-formed so `UuidParam` passes; the `Query<Amount>` extractor
/// then fails to parse `abc` as `u64`.
#[tokio::test]
async fn test_extractor_query_rejection_logged_and_traced() {
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

    step_when_i_get_invoice_with_non_numeric_amount(&mut ctx, "default")
        .await
        .expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    step_and_expected_error_should_be_logged(
        &mut ctx,
        ExpectedErrorContext {
            service: Service::LnUrl,
            method: "GET",
            path_prefix: "/offers/default/",
            status: 400,
            level: EcsLevel::Warn,
        },
    )
    .await
    .expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// Locks in the per-service `event.category` wiring on error-emitter
/// records. `CrudError` (used by `swgr.discovery` and `swgr.offer`) emits
/// `event.category = ["api", "web"]` — the multi-value covers both the
/// admin-REST API framing (`api`) and the HTTP-server access-log framing
/// (`web`, which is the only category whose Expected event types list
/// includes `error`). `LnUrlPayServiceError` (used by `swgr.lnurl`) emits
/// `event.category = ["web"]` alone.
///
/// Trips each service's error emitter with a real failing request, then
/// asserts:
/// - The correct discriminating category is present in the array
///   (array-contains match on `event_category`).
/// - `event.kind = "event"`, `event.type = "error"`, `event.outcome =
///   "failure"` on every record.
/// - Negative cross-check: lnurl's records do NOT carry `api`; admin
///   services' records DO carry both `api` AND `web`.
///
/// RequestLogger records don't carry event.* by design (§ 5.3 of the
/// audit doc), so this test targets the error emitters, not access logs.
#[tokio::test]
async fn test_event_categorization_per_service_on_error_emitters() {
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
    step_and_the_discovery_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");
    step_and_the_offers_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");

    // lnurl: LnUrlPayServiceError::not_found → 404 WARN, event.category=["web"].
    step_when_i_request_an_offer_from_a_non_existent_partition(&mut ctx)
        .await
        .expect("assert");
    // discovery: CrudError::unauthorized → 401 WARN, event.category=["api","web"].
    step_when_i_post_backend_without_auth(&mut ctx)
        .await
        .expect("assert");
    // offer: CrudError::bad (over-limit page count) → 400 WARN, event.category=["api","web"].
    step_when_i_get_offers_with_over_limit_count(&mut ctx, "default", 10_000)
        .await
        .expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    let stderr_lines = read_active_stderr_lines(&ctx).expect("assert");

    // lnurl-emitter: category must contain "web" and must NOT contain "api".
    let lnurl_web = EcsRequestFilter {
        service: Some("lnurl"),
        status: Some(404),
        level: Some("WARN"),
        event_kind: Some("event"),
        event_category: Some("web"),
        event_type: Some("error"),
        event_outcome: Some("failure"),
        ..Default::default()
    };
    assert_eq!(
        count_ecs_requests(&stderr_lines, &lnurl_web),
        1,
        "expected exactly one lnurl WARN record with event.category containing web"
    );
    let lnurl_api = EcsRequestFilter {
        event_category: Some("api"),
        ..lnurl_web
    };
    assert_eq!(
        count_ecs_requests(&stderr_lines, &lnurl_api),
        0,
        "lnurl WARN record must not carry event.category=api"
    );

    // discovery-emitter: category must contain BOTH "api" AND "web".
    let discovery_api = EcsRequestFilter {
        service: Some("discovery"),
        method: Some("POST"),
        path_exact: Some("/discovery"),
        status: Some(401),
        level: Some("WARN"),
        event_kind: Some("event"),
        event_category: Some("api"),
        event_type: Some("error"),
        event_outcome: Some("failure"),
        ..Default::default()
    };
    assert_eq!(
        count_ecs_requests(&stderr_lines, &discovery_api),
        1,
        "expected exactly one discovery WARN record with event.category containing api"
    );
    let discovery_web = EcsRequestFilter {
        event_category: Some("web"),
        ..discovery_api
    };
    assert_eq!(
        count_ecs_requests(&stderr_lines, &discovery_web),
        1,
        "discovery WARN record must also carry event.category=web"
    );

    // offer-emitter: category must contain BOTH "api" AND "web".
    let offer_api = EcsRequestFilter {
        service: Some("offer"),
        method: Some("GET"),
        path_prefix: Some("/offers/"),
        status: Some(400),
        level: Some("WARN"),
        event_kind: Some("event"),
        event_category: Some("api"),
        event_type: Some("error"),
        event_outcome: Some("failure"),
        ..Default::default()
    };
    assert_eq!(
        count_ecs_requests(&stderr_lines, &offer_api),
        1,
        "expected exactly one offer WARN record with event.category containing api"
    );
    let offer_web = EcsRequestFilter {
        event_category: Some("web"),
        ..offer_api
    };
    assert_eq!(
        count_ecs_requests(&stderr_lines, &offer_web),
        1,
        "offer WARN record must also carry event.category=web"
    );

    ctx.stop_all_servers().expect("assert");
}

/// Locks in `event.outcome = "failure"` on the failure branches of both
/// error emitters (`CrudError` on `swgr.discovery`, `LnUrlPayServiceError`
/// on `swgr.lnurl`) in a single scenario: an unauthenticated POST
/// /discovery (401 via CrudError) and a GET to a non-existent partition on
/// the lnurl service (404 via LnUrlPayServiceError). Both emit a matched
/// pair of records (INFO access with `event.outcome = "failure"` +
/// WARN error with `event.outcome = "failure"`) — the existing
/// `count_ecs_requests` WARN branch already enforces the pairing.
#[tokio::test]
async fn test_event_outcome_failure_on_error_emitters() {
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
    step_and_the_discovery_service_should_be_listening_on_the_configured_port(&mut ctx)
        .await
        .expect("assert");

    // 401 via CrudError on the discovery service.
    step_when_i_post_backend_without_auth(&mut ctx)
        .await
        .expect("assert");
    // 404 via LnUrlPayServiceError on the lnurl service.
    step_when_i_request_an_offer_from_a_non_existent_partition(&mut ctx)
        .await
        .expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    let stderr_lines = read_active_stderr_lines(&ctx).expect("assert");

    // CrudError WARN 401 pair on swgr.discovery with event.category=api,
    // event.outcome=failure.
    let discovery_filter = EcsRequestFilter {
        service: Some("discovery"),
        method: Some("POST"),
        path_exact: Some("/discovery"),
        status: Some(401),
        level: Some("WARN"),
        event_kind: Some("event"),
        event_category: Some("api"),
        event_outcome: Some("failure"),
        ..Default::default()
    };
    assert_eq!(
        count_ecs_requests(&stderr_lines, &discovery_filter),
        1,
        "expected exactly one 401 pair on swgr.discovery with event.category=api, event.outcome=failure"
    );

    // LnUrlPayServiceError WARN 404 pair on swgr.lnurl with
    // event.category=web, event.outcome=failure.
    let lnurl_filter = EcsRequestFilter {
        service: Some("lnurl"),
        method: Some("GET"),
        path_prefix: Some("/offers/non-existent-partition/"),
        status: Some(404),
        level: Some("WARN"),
        event_kind: Some("event"),
        event_category: Some("web"),
        event_outcome: Some("failure"),
        ..Default::default()
    };
    assert_eq!(
        count_ecs_requests(&stderr_lines, &lnurl_filter),
        1,
        "expected exactly one 404 pair on swgr.lnurl with event.category=web, event.outcome=failure"
    );

    ctx.stop_all_servers().expect("assert");
}
