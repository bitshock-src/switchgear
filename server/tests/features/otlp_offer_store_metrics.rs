//! `db.client.operation.duration` for the offer service's database store.
//!
//! Same pipeline as `otlp_metrics.rs`: the child exports OTLP metrics to the
//! containerised collector, which writes them to InfluxDB, and the assertion
//! is a SQL query. What differs is the driver — the `swgr` CLI against a
//! server whose stores are SQLite, so one scenario walks every call site in
//! `components/src/offer/db.rs`, including the two the foreign key rejects.

use crate::FEATURE_TEST_CONFIG_PATH;
use crate::common::context::Protocol;
use crate::common::context::cli::CliContext;
use crate::common::context::global::GlobalContext;
use crate::common::context::server::CertificateLocation;
use crate::common::step_functions::*;
use std::path::PathBuf;

/// Every `swgr.operation` in `offer/db.rs`, with the table each names.
const OPERATIONS: &[(&str, &str)] = &[
    ("get_offer", "offer_record"),
    ("get_offers", "offer_record"),
    ("post_offer", "offer_record"),
    ("put_offer_upsert", "offer_record"),
    ("put_offer_fetch", "offer_record"),
    ("delete_offer", "offer_record"),
    ("get_metadata", "offer_metadata"),
    ("get_all_metadata", "offer_metadata"),
    ("post_metadata", "offer_metadata"),
    ("put_metadata_upsert", "offer_metadata"),
    ("put_metadata_fetch", "offer_metadata"),
    ("delete_metadata", "offer_metadata"),
];

#[tokio::test]
async fn test_offer_store_operations_reach_influx() {
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

    // Posts the first metadata and leaves an offer JSON referencing it.
    step_given_a_valid_offer_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    let offer_id = extract_offer_id(&cli_ctx).await.expect("assert");
    let metadata_id = extract_metadata_id_from_offer(&cli_ctx)
        .await
        .expect("assert");

    step_when_i_run_swgr_offer_post(&mut ctx, &mut cli_ctx, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    step_when_i_run_swgr_offer_get(&mut ctx, &mut cli_ctx, &offer_id, CertificateLocation::Arg)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    step_when_i_run_swgr_offer_get_all(&mut ctx, &mut cli_ctx, None, None)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    step_when_i_run_swgr_offer_metadata_get(
        &mut ctx,
        &mut cli_ctx,
        &metadata_id,
        CertificateLocation::Arg,
    )
    .await
    .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    step_when_i_run_swgr_offer_metadata_get_all(&mut ctx, &mut cli_ctx, None, None)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    step_and_updated_offer_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_offer_put(&mut ctx, &mut cli_ctx, &offer_id)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    step_and_updated_offer_metadata_json_exists(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_offer_metadata_put(&mut ctx, &mut cli_ctx, &metadata_id)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    // A miss is still a successful round trip: `get_offer` maps the empty
    // result to `Ok(None)` and the metric records no `error.type`.
    step_when_i_run_swgr_offer_get_for_non_existent_offer(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_then_the_command_should_fail(&mut cli_ctx)
        .await
        .expect("assert");

    // The two foreign-key rejections, which are the only failures the offer
    // store's API surface can provoke. The second posts the run's second
    // metadata and a second offer along the way.
    step_when_i_run_swgr_offer_post_with_unknown_metadata_expecting_failure(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");
    step_when_i_run_swgr_offer_metadata_delete_referenced_expecting_failure(&mut ctx, &mut cli_ctx)
        .await
        .expect("assert");

    step_when_i_run_swgr_offer_delete(&mut ctx, &mut cli_ctx, &offer_id)
        .await
        .expect("assert");
    step_then_the_command_should_succeed(&mut cli_ctx)
        .await
        .expect("assert");

    step_when_i_run_swgr_offer_metadata_delete(&mut ctx, &mut cli_ctx, &metadata_id)
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
            OFFER_SERVICE,
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

    // Both foreign-key rejections carry the same `error.type`: it is the
    // class of failure, kept predictable and low-cardinality. What separates
    // them is `db.response.status_code` — SQLITE_CONSTRAINT_FOREIGNKEY on
    // the insert, SQLITE_CONSTRAINT_TRIGGER on the delete — which is the
    // domain-specific attribute the convention pairs with `error.type`.
    for (operation, status_code, error_type) in [
        ("post_offer", "787", "foreign_key_constraint"),
        ("delete_metadata", "1811", "statement"),
    ] {
        let row = step_then_influx_should_have_a_histogram_row(
            &influx,
            &identity,
            DB_OPERATION_DURATION,
            OFFER_SERVICE,
            &[
                ("swgr.operation", operation),
                ("db.response.status_code", status_code),
            ],
            MetricOutcome::Error(error_type),
        )
        .await
        .expect("assert");
        step_then_the_histogram_row_should_be_well_formed(&row).expect("assert");
    }

    step_then_no_ecs_log_record_should_carry_a_metric_field(&mut ctx).expect("assert");

    ctx.stop_all_servers().expect("assert");
}
