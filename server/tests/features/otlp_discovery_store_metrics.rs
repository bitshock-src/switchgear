//! `db.client.operation.duration` for the discovery service's database
//! store.
//!
//! The mirror of `otlp_offer_store_metrics.rs`: the `swgr` CLI drives a
//! server whose stores are SQLite, and one scenario walks every call site in
//! `components/src/discovery/db.rs`.
//!
//! No `error.type` row here. The discovery schema has no foreign keys, so no
//! call the API surface can make will fail the transaction; the `error.type`
//! mapping itself is unit-tested in `components/src/metrics.rs`.

use crate::FEATURE_TEST_CONFIG_PATH;
use crate::common::context::Protocol;
use crate::common::context::cli::CliContext;
use crate::common::context::global::GlobalContext;
use crate::common::context::server::CertificateLocation;
use crate::common::step_functions::*;
use std::path::PathBuf;

/// Every `swgr.operation` in `discovery/db.rs`, with the table each names.
/// The four writes are transactions spanning both tables; each names the one
/// the call is about, and the etag bump is incidental.
const OPERATIONS: &[(&str, &str)] = &[
    ("get", "discovery_backend"),
    ("get_all_etag", "discovery_backend_etag"),
    ("get_all_backends", "discovery_backend"),
    ("post", "discovery_backend"),
    ("put", "discovery_backend"),
    ("patch", "discovery_backend"),
    ("delete", "discovery_backend"),
];

#[tokio::test]
async fn test_discovery_store_operations_reach_influx() {
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

    let mut cli_ctx = CliContext::create().expect("assert");
    step_given_the_swgr_cli_is_available(&mut cli_ctx)
        .await
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
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx)
        .await
        .expect("assert");

    step_given_a_valid_backend_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    let public_key = extract_backend_public_key(&cli_ctx)
        .await
        .expect("assert")
        .to_string();

    step_when_i_run_swgr_discovery_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    step_when_i_run_swgr_discovery_get(
        &mut ctx,
        &mut cli_ctx,
        &public_key,
        CertificateLocation::Arg,
    )
    .await
    .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    step_when_i_run_swgr_discovery_ls(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    step_when_i_run_swgr_discovery_get_all(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    step_and_updated_backend_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_discovery_put(&mut ctx, &mut cli_ctx, &public_key)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    step_and_backend_patch_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_discovery_patch(&mut ctx, &mut cli_ctx, &public_key)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    step_when_i_run_swgr_discovery_disable(&mut ctx, &mut cli_ctx, &public_key)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    step_when_i_run_swgr_discovery_enable(&mut ctx, &mut cli_ctx, &public_key)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    // A miss is still a successful round trip: `get` maps the empty result to
    // `Ok(None)` and the metric records no `error.type`.
    step_when_i_run_swgr_discovery_get_for_non_existent_backend(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");

    step_when_i_run_swgr_discovery_delete(&mut ctx, &mut cli_ctx, &public_key)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    // SIGTERM drains the meter provider: the `PeriodicReader`'s own interval
    // is 60s, so shutdown is what puts this run's datapoints on the wire.
    step_when_i_send_a_sigterm_signal_to_the_server_process(&mut ctx)
        .await
        .expect("assert");
    step_then_the_server_should_exit_with_code_0(&mut ctx)
        .await
        .expect("assert");

    let influx = influx_client().expect("assert");

    for (operation, collection) in OPERATIONS {
        let row = step_then_influx_should_have_a_histogram_row(
            &influx,
            &identity,
            DB_OPERATION_DURATION,
            DISCOVERY_SERVICE,
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
