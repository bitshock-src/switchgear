use crate::common::context::global::GlobalContext;
use crate::common::context::server::CertificateLocation;
use crate::common::context::Protocol;
use crate::common::helpers::get_payee_from_context;
use crate::common::step_functions::*;
use crate::FEATURE_TEST_CONFIG_PATH;
use std::path::PathBuf;
use switchgear_testing::credentials::lightning::{RegTestLnNode, RegTestLnNodeType};

enum LnTrustRootsLocation {
    Credentials,
    Configuration,
    Native,
}

#[tokio::test]
async fn test_payer_requests_invoice_from_payee_cln_lightning_offer_using_http_creds() {
    test_payer_requests_invoice_from_payee_inner(
        Protocol::Http,
        RegTestLnNodeType::Cln,
        LnTrustRootsLocation::Credentials,
    )
    .await
    .expect("assert");
}

#[tokio::test]
async fn test_payer_requests_invoice_from_payee_cln_lightning_offer_using_https_creds() {
    test_payer_requests_invoice_from_payee_inner(
        Protocol::Https,
        RegTestLnNodeType::Cln,
        LnTrustRootsLocation::Credentials,
    )
    .await
    .expect("assert");
}

#[tokio::test]
async fn test_payer_requests_invoice_from_payee_cln_lightning_offer_using_https_config() {
    test_payer_requests_invoice_from_payee_inner(
        Protocol::Https,
        RegTestLnNodeType::Cln,
        LnTrustRootsLocation::Configuration,
    )
    .await
    .expect("assert");
}

#[tokio::test]
async fn test_payer_requests_invoice_from_payee_cln_lightning_offer_using_https_native() {
    test_payer_requests_invoice_from_payee_inner(
        Protocol::Https,
        RegTestLnNodeType::Cln,
        LnTrustRootsLocation::Native,
    )
    .await
    .expect("assert");
}

#[tokio::test]
async fn test_payer_requests_invoice_from_payee_lnd_lightning_offer_using_http_creds() {
    test_payer_requests_invoice_from_payee_inner(
        Protocol::Http,
        RegTestLnNodeType::Lnd,
        LnTrustRootsLocation::Credentials,
    )
    .await
    .expect("assert");
}

#[tokio::test]
async fn test_payer_requests_invoice_from_payee_lnd_lightning_offer_using_https_creds() {
    test_payer_requests_invoice_from_payee_inner(
        Protocol::Https,
        RegTestLnNodeType::Lnd,
        LnTrustRootsLocation::Credentials,
    )
    .await
    .expect("assert");
}

#[tokio::test]
async fn test_payer_requests_invoice_from_payee_lnd_lightning_offer_using_https_config() {
    test_payer_requests_invoice_from_payee_inner(
        Protocol::Https,
        RegTestLnNodeType::Lnd,
        LnTrustRootsLocation::Configuration,
    )
    .await
    .expect("assert");
}

#[tokio::test]
async fn test_payer_requests_invoice_from_payee_lnd_lightning_offer_using_https_native() {
    test_payer_requests_invoice_from_payee_inner(
        Protocol::Https,
        RegTestLnNodeType::Lnd,
        LnTrustRootsLocation::Native,
    )
    .await
    .expect("assert");
}

async fn test_payer_requests_invoice_from_payee_inner(
    protocol: Protocol,
    node_type: RegTestLnNodeType,
    ln_trusted_roots_location: LnTrustRootsLocation,
) -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let feature_test_config_path = manifest_dir.join(FEATURE_TEST_CONFIG_PATH);
    let mut ctx = match GlobalContext::create(&feature_test_config_path)? {
        Some(ctx) => ctx,
        None => return Ok(()),
    };

    let server1 = "server1";
    let config_path = match protocol {
        Protocol::Http => manifest_dir.join("config/memory-basic-no-tls.yaml"),
        Protocol::Https => manifest_dir.join("config/memory-basic.yaml"),
    };

    ctx.add_server(server1, config_path, protocol, protocol, protocol)?;
    ctx.activate_server(server1);

    // Given: Specific backend type
    step_given_the_payee_has_a_lightning_node_available(&mut ctx, node_type).await?;

    let payee = get_payee_from_context(&ctx, "single")?;
    let node_cert_path = match &payee.node {
        RegTestLnNode::Cln(n) => n.ca_cert_path.as_path(),
        RegTestLnNode::Lnd(n) => n.tls_cert_path.as_path(),
    };

    let include_ca = match ln_trusted_roots_location {
        LnTrustRootsLocation::Credentials => true,
        LnTrustRootsLocation::Configuration => {
            ctx.set_ln_trusted_roots_path(server1, Some(node_cert_path.to_path_buf()))
                .expect("assert");
            false
        }
        LnTrustRootsLocation::Native => {
            ctx.set_certificate_location(
                server1,
                CertificateLocation::NativePath(node_cert_path.to_string_lossy().to_string()),
            )
            .expect("assert");
            false
        }
    };

    // When: Start the server
    step_when_i_start_the_lnurl_server_with_the_configuration(&mut ctx).await?;

    // Then: Verify server starts successfully
    step_then_the_server_should_start_successfully(&mut ctx).await?;
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx).await?;

    // Background: Verify LNURL server is running
    step_and_all_services_should_be_listening_on_their_configured_ports(&mut ctx).await?;

    // When: Payee setup steps
    step_when_the_payee_creates_an_offer_for_their_lightning_node(&mut ctx, "single").await?;
    step_when_the_payee_registers_their_lightning_node_as_a_backend(&mut ctx, "single", include_ca)
        .await?;

    // When: Payer requests offer using specified protocol
    step_when_the_payer_requests_the_lnurl_offer_from_the_payee(&mut ctx, "single").await?;

    // Then: Verify offer properties
    step_then_the_payee_offer_should_contain_valid_sendable_amounts(&mut ctx, "single").await?;
    step_then_the_payee_offer_should_contain_valid_metadata(&mut ctx, "single").await?;
    step_then_the_payee_offer_should_provide_a_callback_url(&mut ctx, "single").await?;

    // When: Request invoice using specified protocol
    step_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url(
        &mut ctx, "single", &protocol,
    )
    .await?;

    // Then: Verify invoice
    step_then_the_payer_should_receive_a_valid_lightning_invoice(&mut ctx, "single").await?;
    step_then_the_invoice_amount_should_be_100000_millisatoshis(&mut ctx, "single").await?;
    step_then_the_invoice_description_hash_should_match_the_metadata_hash(&mut ctx, "single")
        .await?;

    ctx.stop_all_servers()?;

    Ok(())
}
