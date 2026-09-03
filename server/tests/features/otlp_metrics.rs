//! OTLP metrics end-to-end test.
//!
//! Unlike `otlp_spans.rs`, which snapshots the wire shape against an
//! in-process collector, this drives the real pipeline: the child server
//! exports OTLP metrics to the containerised OTel collector, which writes
//! them to InfluxDB under the `telegraf-prometheus-v1` schema (one
//! measurement per metric name; resource, scope and datapoint attributes
//! become tags). The assertion is a SQL query against InfluxDB.
//!
//! Runs are isolated by a random `test_identity` resource attribute set on
//! the children through `OTEL_RESOURCE_ATTRIBUTES`, so concurrent and
//! previous runs sharing the container never collide.
//!
//! The scenarios here cover the LNURL service's read path, one topology per
//! store backing: gRPC to a Lightning node, HTTP stores on both the success
//! and the connect-failure path, and database stores. Each assertion is made
//! in exactly one place — `rpc.client.call.duration` belongs to the two gRPC
//! scenarios, which exist to cover both node types' call sites, so the
//! HTTP- and database-store scenarios do not re-assert it. The database
//! stores' own call sites live in `otlp_offer_store_metrics.rs` and
//! `otlp_discovery_store_metrics.rs`.

use crate::FEATURE_TEST_CONFIG_PATH;
use crate::common::context::Protocol;
use crate::common::context::global::GlobalContext;
use crate::common::context::server::CertificateLocation;
use crate::common::step_functions::*;
use std::path::PathBuf;

/// A port with nothing listening, so a request to it fails at connect.
const DEAD_STORE_URL: &str = "https://127.0.0.1:1";

/// How server 2's discovery store is wired in the two-server topology.
#[derive(Clone, Copy)]
enum DiscoveryStore {
    /// Reachable, on server 1.
    OnServer1,
    /// Unreachable, so every backend refresh fails at connect.
    DeadPort,
}

/// The two-server topology from `http_remote_stores.rs`: server 1 serves the
/// offers and discovery services over memory stores, and server 2 runs only
/// the LNURL service, reaching them over HTTP. Both children export, so both
/// carry the scenario's `identity`.
///
/// Returns with both started, their services listening, a CLN payee
/// available, and server 1 active.
async fn start_remote_store_topology(
    identity: &str,
    discovery_store: DiscoveryStore,
) -> GlobalContext {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");

    let server1 = "server1";
    ctx.add_server(
        server1,
        manifest_dir.join("config/memory-basic.yaml"),
        Protocol::Https,
        Protocol::Https,
        Protocol::Https,
    )
    .expect("assert");

    let server2 = "server2";
    ctx.add_server(
        server2,
        manifest_dir.join("config/lnurl-standalone.yaml"),
        Protocol::Https,
        Protocol::Https,
        Protocol::Https,
    )
    .expect("assert");

    ctx.set_offer_store_url(server1, server2).expect("assert");
    ctx.set_offer_store_authorization(server1, server2)
        .expect("assert");
    ctx.set_discovery_store_authorization(server1, server2)
        .expect("assert");
    ctx.set_certificate_location(server2, CertificateLocation::Env)
        .expect("assert");

    for server in [server1, server2] {
        ctx.activate_server(server);
        step_given_the_active_server_is_stamped_with_test_identity(&mut ctx, identity)
            .await
            .expect("assert");
    }

    match discovery_store {
        DiscoveryStore::OnServer1 => {
            ctx.set_discovery_store_url(server1, server2)
                .expect("assert");
        }
        DiscoveryStore::DeadPort => {
            ctx.activate_server(server2);
            ctx.set_active_discovery_store_url_raw(Some(DEAD_STORE_URL.to_string()))
                .expect("assert");
        }
    }

    ctx.activate_server(server1);
    step_given_the_payee_has_a_lightning_node_available(&mut ctx, "cln")
        .await
        .expect("assert");
    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
        .await
        .expect("assert");
    step_when_i_start_server_1_with_offers_and_discovery_services(&mut ctx)
        .await
        .expect("assert");
    step_then_server_1_should_have_offers_and_discovery_services_listening(&mut ctx)
        .await
        .expect("assert");

    ctx.activate_server(server2);
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
        .await
        .expect("assert");
    step_when_i_start_server_2_with_only_lnurl_service(&mut ctx)
        .await
        .expect("assert");
    step_then_server_2_should_have_only_lnurl_service_listening(&mut ctx)
        .await
        .expect("assert");

    ctx.activate_server(server1);
    ctx
}

/// The wire path of the invoice RPC each node type serves.
fn invoice_rpc_method(node_type: &str) -> &'static str {
    match node_type {
        "cln" => "cln.Node/Invoice",
        _ => "lnrpc.Lightning/AddInvoice",
    }
}

#[tokio::test]
async fn test_grpc_invoice_request_reaches_influx_cln() {
    test_grpc_invoice_request_reaches_influx_inner("cln").await;
}

#[tokio::test]
async fn test_grpc_invoice_request_reaches_influx_lnd() {
    test_grpc_invoice_request_reaches_influx_inner("lnd").await;
}

/// The sole home of the `rpc.client.call.duration` assertion: one run per
/// node type, because `rpc.method` is the wire path of that node's own
/// service and the two call sites are separate code.
async fn test_grpc_invoice_request_reaches_influx_inner(node_type: &str) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");

    let server1 = "server1";
    ctx.add_server(
        server1,
        manifest_dir.join("config/memory-basic.yaml"),
        Protocol::Https,
        Protocol::Https,
        Protocol::Https,
    )
    .expect("assert");
    ctx.activate_server(server1);

    let identity = new_test_identity();
    step_given_the_active_server_is_stamped_with_test_identity(&mut ctx, &identity)
        .await
        .expect("assert");

    step_given_the_server_is_not_already_running(&mut ctx)
        .await
        .expect("assert");
    step_given_the_lnurl_server_is_ready_to_start(&mut ctx)
        .await
        .expect("assert");
    step_given_the_payee_has_a_lightning_node_available(&mut ctx, node_type)
        .await
        .expect("assert");
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_start_successfully(&mut ctx)
        .await
        .expect("assert");
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
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

    // SIGTERM drains the meter provider: the `PeriodicReader`'s own interval is
    // 60s, so shutdown is what puts this run's datapoint on the wire.
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    let influx = influx_client().expect("assert");
    let row = step_then_influx_should_have_a_histogram_row(
        &influx,
        &identity,
        RPC_CALL_DURATION,
        LNURL_SERVICE,
        &[("rpc.method", invoice_rpc_method(node_type))],
        MetricOutcome::Success,
    )
    .await
    .expect("assert");
    step_then_the_histogram_row_should_be_well_formed(&row).expect("assert");

    step_then_no_ecs_log_record_should_carry_a_metric_field(&mut ctx).expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// The LNURL service reading through HTTP stores: both reads its request
/// path makes, on the success path.
///
/// The invoice itself is asserted by the step that requests it, so the gRPC
/// metric is left to the two scenarios above.
#[tokio::test]
async fn test_http_store_reads_reach_influx() {
    let identity = new_test_identity();
    let mut ctx = start_remote_store_topology(&identity, DiscoveryStore::OnServer1).await;

    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "single")
        .await
        .expect("assert");
    step_when_the_payee_registers_their_lightning_node_as_a_backend(&mut ctx, "single", true)
        .await
        .expect("assert");

    ctx.activate_server("server2");
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
    step_then_the_payer_should_receive_a_valid_lightning_invoice(&mut ctx, "single")
        .await
        .expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_all_servers_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    let influx = influx_client().expect("assert");
    // `/discovery` comes from the balancer's in-request retry, which
    // refreshes its backends before each attempt.
    for template in ["/offers/{partition}/{id}", "/discovery"] {
        let row = step_then_influx_should_have_a_histogram_row(
            &influx,
            &identity,
            HTTP_REQUEST_DURATION,
            LNURL_SERVICE,
            &[("http.request.method", "GET"), ("url.template", template)],
            MetricOutcome::Success,
        )
        .await
        .expect("assert");
        step_then_the_histogram_row_should_be_well_formed(&row).expect("assert");
    }

    step_then_no_ecs_log_record_should_carry_a_metric_field(&mut ctx).expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// The LNURL service's offer store pointed at a dead port.
///
/// Topology from `otlp_spans.rs::test_dead_offer_store_502_otlp_shape`. The
/// offer read fails at reqwest's connect, so the call site records
/// `error.type = connect` and no `http.response.status_code` at all.
///
/// The discovery store's connect failure is not reachable from this
/// topology: every request that would refresh the balancer's backends goes
/// through the invoice handler, which fetches the offer first and so never
/// gets past the dead offer store. It is covered by
/// `test_dead_discovery_store_records_connect_errors` below.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_dead_offer_store_records_connect_errors() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");

    let server1 = "server1";
    ctx.add_server(
        server1,
        manifest_dir.join("config/lnurl-standalone.yaml"),
        Protocol::Https,
        Protocol::Https,
        Protocol::Https,
    )
    .expect("assert");
    ctx.activate_server(server1);

    // The auth pointers and certificate location still have to resolve for
    // the config to load.
    ctx.set_active_offer_store_url_raw(Some(DEAD_STORE_URL.to_string()))
        .expect("assert");
    ctx.set_active_discovery_store_url_raw(Some(DEAD_STORE_URL.to_string()))
        .expect("assert");
    ctx.set_offer_store_authorization(server1, server1)
        .expect("assert");
    ctx.set_discovery_store_authorization(server1, server1)
        .expect("assert");
    ctx.set_certificate_location(server1, CertificateLocation::Env)
        .expect("assert");

    let identity = new_test_identity();
    step_given_the_active_server_is_stamped_with_test_identity(&mut ctx, &identity)
        .await
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

    // Any partition/id — the store's request never reaches a real server.
    step_when_i_request_offer_expecting_failure(&mut ctx, "default")
        .await
        .expect("assert");

    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    let influx = influx_client().expect("assert");
    let row = step_then_influx_should_have_a_histogram_row(
        &influx,
        &identity,
        HTTP_REQUEST_DURATION,
        LNURL_SERVICE,
        &[
            ("http.request.method", "GET"),
            ("url.template", "/offers/{partition}/{id}"),
        ],
        MetricOutcome::Error("connect"),
    )
    .await
    .expect("assert");
    step_then_the_histogram_row_should_be_well_formed(&row).expect("assert");

    step_then_no_ecs_log_record_should_carry_a_metric_field(&mut ctx).expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// A live offer store and a dead discovery store.
///
/// The LNURL offer request succeeds against server 1, so the invoice request
/// gets past the offer fetch and reaches the balancer. With no backends
/// readable it retries, and each retry refreshes the discovery store
/// in-request — which is where `GET /discovery` records its connect failure.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_dead_discovery_store_records_connect_errors() {
    let identity = new_test_identity();
    let mut ctx = start_remote_store_topology(&identity, DiscoveryStore::DeadPort).await;

    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "single")
        .await
        .expect("assert");

    ctx.activate_server("server2");
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
    step_then_all_servers_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    let influx = influx_client().expect("assert");
    let row = step_then_influx_should_have_a_histogram_row(
        &influx,
        &identity,
        HTTP_REQUEST_DURATION,
        LNURL_SERVICE,
        &[
            ("http.request.method", "GET"),
            ("url.template", "/discovery"),
        ],
        MetricOutcome::Error("connect"),
    )
    .await
    .expect("assert");
    step_then_the_histogram_row_should_be_well_formed(&row).expect("assert");

    step_then_no_ecs_log_record_should_carry_a_metric_field(&mut ctx).expect("assert");

    ctx.stop_all_servers().expect("assert");
}

/// The same LNURL flow over database stores.
///
/// `config/persistence.yaml` runs all three services in one process against
/// one pair of SQLite files, which is the only place `service.name`
/// attribution is put under real pressure: the setup steps and the LNURL
/// request path share a store, and each row must still be attributed to the
/// dispatch that made the call. The per-service call-site inventories belong
/// to `otlp_offer_store_metrics.rs` and `otlp_discovery_store_metrics.rs`,
/// so only one write per service is checked here.
#[tokio::test]
async fn test_db_store_reads_reach_influx() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = GlobalContext::create(&feature_test_config_path).expect("assert");

    let server1 = "server1";
    ctx.add_server(
        server1,
        manifest_dir.join("config/persistence.yaml"),
        Protocol::Https,
        Protocol::Https,
        Protocol::Https,
    )
    .expect("assert");
    ctx.activate_server(server1);

    let identity = new_test_identity();
    step_given_the_active_server_is_stamped_with_test_identity(&mut ctx, &identity)
        .await
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
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
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

    let influx = influx_client().expect("assert");

    // The LNURL service's own reads. `get_all_*` come from the balancer's
    // in-request retry, which refreshes its backends before each attempt.
    for (service, operation, collection) in [
        (LNURL_SERVICE, "get_offer", "offer_record"),
        (LNURL_SERVICE, "get_all_etag", "discovery_backend_etag"),
        (LNURL_SERVICE, "get_all_backends", "discovery_backend"),
        // One write per other service, on the same store and the same
        // identity: the row is proof the dispatch, not the store, decides
        // `service.name`.
        (OFFER_SERVICE, "post_offer", "offer_record"),
        (DISCOVERY_SERVICE, "post", "discovery_backend"),
    ] {
        let row = step_then_influx_should_have_a_histogram_row(
            &influx,
            &identity,
            DB_OPERATION_DURATION,
            service,
            &[
                ("swgr.operation", operation),
                ("db.collection.name", collection),
                ("db.system.name", "sqlite"),
            ],
            MetricOutcome::Success,
        )
        .await
        .expect("assert");
        step_then_the_histogram_row_should_be_well_formed(&row).expect("assert");
    }

    step_then_no_ecs_log_record_should_carry_a_metric_field(&mut ctx).expect("assert");

    ctx.stop_all_servers().expect("assert");
}
