use crate::common::context::global::GlobalContext;
use crate::common::context::Protocol;
use crate::common::step_functions::*;
use crate::FEATURE_TEST_CONFIG_PATH;
use std::path::PathBuf;
use switchgear_testing::credentials::lightning::RegTestLnNodeType;

use crate::common::context::server::CertificateLocation;

#[tokio::test]
async fn test_complete_http_remote_stores_workflow_with_distributed_services() {
    let certificate_locations = vec![CertificateLocation::Env, CertificateLocation::Native];

    for certificate_location in certificate_locations {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
        let mut ctx = match GlobalContext::create(&feature_test_config_path).expect("assert") {
            Some(ctx) => ctx,
            None => return,
        };
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

        let server2 = "server2";
        let config_path = manifest_dir.join("config/lnurl-standalone.yaml");
        ctx.add_server(
            server2,
            config_path,
            Protocol::Https,
            Protocol::Https,
            Protocol::Https,
        )
        .expect("assert");

        ctx.set_discovery_store_url(server1, server2)
            .expect("assert");
        ctx.set_discovery_store_authorization(server1, server2)
            .expect("assert");

        ctx.set_offer_store_url(server1, server2).expect("assert");
        ctx.set_offer_store_authorization(server1, server2)
            .expect("assert");

        // Set certificate location for server2
        ctx.set_certificate_location(server2, certificate_location)
            .expect("assert");

        ctx.activate_server(server1);

        // Background
        step_given_the_payee_has_a_lightning_node_available(&mut ctx, RegTestLnNodeType::Cln)
            .await
            .expect("assert");
        step_given_the_server_is_not_already_running(&mut ctx)
            .await
            .expect("assert");

        // Setup first server with offers and discovery services using memory stores
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

        // Create offer and register backend on server1 (offers and discovery services)
        step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "single")
            .await
            .expect("assert");
        step_when_the_payee_registers_their_lightning_node_as_a_backend(&mut ctx, "single", true)
            .await
            .expect("assert");

        ctx.activate_server(server2);

        // Request LNURL offer and invoice through server2 (which uses HTTP stores to access server1)
        step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single")
            .await
            .expect("assert");
        step_then_the_payee_offer_should_contain_valid_sendable_amounts(&mut ctx, "single")
            .await
            .expect("assert");
        step_then_the_payee_offer_should_contain_valid_metadata(&mut ctx, "single")
            .await
            .expect("assert");
        step_then_the_payee_offer_should_provide_a_callback_url(&mut ctx, "single")
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
        step_then_the_invoice_amount_should_be_100000_millisatoshis(&mut ctx, "single")
            .await
            .expect("assert");
        step_then_the_invoice_description_hash_should_match_the_metadata_hash(&mut ctx, "single")
            .await
            .expect("assert");

        // Stop servers and validate logs
        step_when_i_stop_all_servers(&mut ctx)
            .await
            .expect("assert");

        ctx.activate_server(server1);

        // Validate server1 logs (offers and discovery services)
        step_then_server_1_logs_should_contain_offer_creation_requests(&mut ctx)
            .await
            .expect("assert");
        step_and_server_1_logs_should_contain_backend_registration_requests(&mut ctx)
            .await
            .expect("assert");
        step_and_server_1_logs_should_contain_health_check_requests_for_offers_and_discovery_services(
        &mut ctx, ).await.expect("assert");
        step_and_server_1_logs_should_contain_http_requests_from_server_2_for_offers_and_discovery(
            &mut ctx,
        )
        .await
        .expect("assert");

        ctx.activate_server(server2);

        // Validate server2 logs (LNURL service with HTTP stores)
        step_and_server_2_logs_should_contain_offer_retrieval_requests_via_http_stores(&mut ctx)
            .await
            .expect("assert");
        step_and_server_2_logs_should_contain_invoice_generation_requests(&mut ctx)
            .await
            .expect("assert");
        step_and_server_2_logs_should_contain_health_check_requests_for_lnurl_service(&mut ctx)
            .await
            .expect("assert");

        ctx.stop_all_servers().expect("assert");
    }
}
