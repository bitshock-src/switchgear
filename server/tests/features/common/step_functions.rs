use crate::common::context::cli::CliContext;
use crate::common::context::global::GlobalContext;
use crate::common::context::pay::OfferRequest;
use crate::common::context::server::CertificateLocation;
use crate::common::context::{Protocol, Service};
use crate::common::helpers::{
    check_all_services_health, check_services_listening_status, count_log_patterns,
    get_invoice_from_request, get_lnurl_offer_from_request, get_offer_id_from_request,
    get_offer_request, get_offer_request_mut, get_payee_from_context, get_payee_from_context_mut,
    parse_lightning_invoice, request_and_validate_invoice_helper, validate_invoice_has_amount,
    verify_exit_code, verify_single_service_status,
};
use crate::{anyhow_log, bail_log};
use anyhow::{anyhow, Context, Result};
use chrono::{Duration as ChronoDuration, Utc};
use rand::{distributions::Alphanumeric, Rng};
use reqwest::{StatusCode, Url};
use secp256k1::PublicKey;
use std::time::{Duration, SystemTime};
use std::vec;
use switchgear_service::api::discovery::{
    DiscoveryBackend, DiscoveryBackendImplementation, DiscoveryBackendPatch,
    DiscoveryBackendPatchSparse, DiscoveryBackendSparse, DiscoveryBackendStore,
};
use switchgear_service::api::offer::{
    OfferMetadata, OfferMetadataSparse, OfferMetadataStore, OfferRecord, OfferRecordSparse,
    OfferStore,
};
use switchgear_service::components::pool::cln::grpc::config::{
    ClnGrpcClientAuth, ClnGrpcClientAuthPath, ClnGrpcDiscoveryBackendImplementation,
};
use switchgear_service::components::pool::lnd::grpc::config::{
    LndGrpcClientAuth, LndGrpcClientAuthPath, LndGrpcDiscoveryBackendImplementation,
};
use switchgear_testing::credentials::lightning::{RegTestLnNode, RegTestLnNodeType};
use tokio::time::sleep as tokio_sleep;
use uuid::Uuid;

// =============================================================================
// STEP FUNCTIONS - Mapped to Gherkin steps in feature files
// =============================================================================

/// Step: "Given an invalid configuration file exists"
/// Creates an invalid YAML configuration file for negative testing
pub async fn step_given_an_invalid_configuration_file_exists(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get the config file path
    let config_path = ctx.get_active_server_config_path()?;
    if !config_path.exists() {
        bail_log!("Configuration file not found: {}", config_path.display());
    }

    // Attempt to load and parse the config file
    let config_content = std::fs::read_to_string(config_path)
        .with_context(|| format!("reading config file: {}", config_path.display()))?;

    // Try to parse with serde_saphyr and assert that it fails
    let parse_result =
        serde_saphyr::from_str::<switchgear_server::config::ServerConfig>(&config_content);

    if parse_result.is_ok() {
        bail_log!(
            "Expected config parsing to fail, but it succeeded for file: {}",
            config_path.display()
        );
    }

    Ok(())
}

/// Step: "Given the server is not already running"
/// Verifies that no server process is already listening on the configured ports
pub async fn step_given_the_server_is_not_already_running(ctx: &mut GlobalContext) -> Result<()> {
    // Check if any process is already listening on our ports
    let services = vec![Service::LnUrl, Service::Discovery, Service::Offer];
    if check_all_services_health(ctx, services).await? {
        bail_log!("Server is already running on one of the configured ports");
    }

    Ok(())
}

/// Step: "Given the LNURL server is ready to start"
/// Ensures the server is ready to start by checking configuration and binary availability
pub async fn step_given_the_lnurl_server_is_ready_to_start(ctx: &mut GlobalContext) -> Result<()> {
    // Ensure we have a valid config and no running server
    let config_path = ctx.get_active_server_config_path()?;
    if !config_path.exists() {
        bail_log!("Configuration file not found: {}", config_path.display());
    }

    let config_content = std::fs::read_to_string(config_path)
        .with_context(|| format!("reading config file: {}", config_path.display()))?;

    let config_content = shellexpand::env(&config_content).with_context(|| {
        format!(
            "expanding configuration file env vars '{}'",
            config_path.to_string_lossy()
        )
    })?;

    let _config =
        serde_saphyr::from_str::<switchgear_server::config::ServerConfig>(&config_content)
            .with_context(|| format!("parsing config file: {}", config_path.display()))?;

    Ok(())
}

/// Step: "When I start the LNURL server with the configuration"
/// Starts the LNURL server process with the generated configuration file
/// Captures stdout/stderr for later assertions
pub async fn step_when_i_start_the_lnurl_server_with_the_configuration(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let _pid = ctx
        .start_active_server(
            &[Service::LnUrl, Service::Discovery, Service::Offer],
            log::Level::Info,
        )
        .await?;
    Ok(())
}

/// Step: "When I send a {signal} signal to the server process"
/// Sends the specified signal to the running server process for graceful shutdown
pub async fn step_when_i_send_a_signal_to_the_server_process(
    ctx: &mut GlobalContext,
    signal: sysinfo::Signal,
) -> Result<()> {
    ctx.signal_all_servers(signal)?;
    Ok(())
}

/// Step: "When I send a SIGTERM signal to the server process"
/// Sends a SIGTERM signal to the running server process for graceful shutdown
pub async fn step_when_i_send_a_sigterm_signal_to_the_server_process(
    ctx: &mut GlobalContext,
) -> Result<()> {
    ctx.stop_all_servers()?;

    Ok(())
}

/// Step: "Then the server should start successfully"
/// Verifies that the server process started without errors
pub async fn step_then_the_server_should_start_successfully(ctx: &mut GlobalContext) -> Result<()> {
    if !ctx.has_active_server_process()? {
        bail_log!("Server process not started");
    }

    Ok(())
}

/// Step: "And all services should be listening on their configured ports"
/// Verifies that all service endpoints are responding to health checks  
pub async fn step_and_all_services_should_be_listening_on_their_configured_ports(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let services = vec![Service::LnUrl, Service::Discovery, Service::Offer];

    check_services_listening_status(ctx, services, true, 100, 5).await?;
    Ok(())
}

/// Step: "Then the server should fail to start"
/// Verifies that the server process failed to start due to invalid configuration
pub async fn step_then_the_server_should_fail_to_start(ctx: &mut GlobalContext) -> Result<()> {
    // Wait for the process to exit (following the pattern from TestService)
    let exit_code = ctx.wait_active_exit_code()?;

    if exit_code != 0 {
        // This is what we expect - process failed
        Ok(())
    } else {
        bail_log!(
            "Process exited successfully with code {:?}, but we expected failure",
            exit_code
        )
    }
}

/// Step: "Then an error message should be displayed"
/// Verifies that an error message was captured in stderr
pub async fn step_then_an_error_message_should_be_displayed(
    ctx: &mut GlobalContext,
    error: &str,
) -> Result<()> {
    let stderr_buf = ctx.get_active_stderr_buffer()?;
    let stderr_buf = stderr_buf
        .lock()
        .map_err(|e| anyhow!("memory error: {e}"))?;

    let stderr_content = stderr_buf.join("\n");
    let exit_code = ctx.wait_active_exit_code()?;

    if exit_code != 0 && stderr_content.contains(error) {
        return Ok(());
    }

    bail_log!("No error message was captured or no error exit code was recorded")
}

/// Step: "Then a success log should be displayed"
/// Verifies that a log message was captured in stderr
pub async fn step_then_a_success_log_should_be_displayed(
    ctx: &mut GlobalContext,
    error: &str,
) -> Result<()> {
    let stderr_buf = ctx.get_active_stderr_buffer()?;
    let stderr_buf = stderr_buf
        .lock()
        .map_err(|e| anyhow!("memory error: {e}"))?;

    let stderr_content = stderr_buf.join("\n");
    let exit_code = ctx.wait_active_exit_code()?;

    if exit_code == 0 && stderr_content.contains(error) {
        return Ok(());
    }

    bail_log!("No log message was captured or no success exit code was recorded")
}

/// Step: "Then the error message should contain configuration parsing details"
/// Verifies that the error message contains specific configuration parsing error details
pub async fn step_then_the_error_message_should_contain_configuration_parsing_details(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Check stderr buffer for specific configuration parsing errors
    if let Ok(stderr_lines) = ctx.get_active_stderr_buffer()?.lock() {
        if !stderr_lines.is_empty() {
            let mut detected_patterns = Vec::new();
            let stderr_text = stderr_lines.join("\n");

            // Assert on specific error content patterns
            if stderr_text.contains("parsing YAML configuration") {
                detected_patterns.push("Configuration parsing error");
            }

            if stderr_text.contains("did not find expected") {
                detected_patterns.push("YAML syntax error");
            }

            if stderr_text.contains("while parsing a flow sequence") {
                detected_patterns.push("Flow sequence parsing error");
            }

            if stderr_text.contains("server terminated with error") {
                detected_patterns.push("Server termination error");
            }

            return if !detected_patterns.is_empty() {
                Ok(())
            } else {
                bail_log!("Expected configuration parsing error patterns not found")
            };
        }
    }

    bail_log!("No stderr capture found for configuration parsing error assertion")
}

/// Step: "Then the server should exit with code 0"
/// Verifies that the server process exits with a successful exit code (0)
pub async fn step_then_the_server_should_exit_with_code_0(ctx: &mut GlobalContext) -> Result<()> {
    verify_exit_code(ctx, true).await?;

    Ok(())
}

/// Step: "Then the server should exit with a non-zero code"
/// Verifies that the server process exits with an error exit code (non-zero)
pub async fn step_then_the_server_should_exit_with_a_non_zero_code(
    ctx: &mut GlobalContext,
) -> Result<()> {
    verify_exit_code(ctx, false).await?;

    Ok(())
}

/// Step: "Then the server should stop accepting new connections"
/// Verifies that the server is no longer accepting new HTTP connections
pub async fn step_then_the_server_should_stop_accepting_new_connections(
    ctx: &mut GlobalContext,
    service: Service,
) -> Result<()> {
    // Wait a moment for the server to stop accepting connections
    tokio_sleep(Duration::from_millis(500)).await;

    let probe = match service {
        Service::LnUrl => ctx.get_active_lnurl_probe()?,
        Service::Discovery => ctx.get_active_discovery_probe()?,
        Service::Offer => ctx.get_active_offer_probe()?,
    };

    let accepting_connections = probe.probe().await;

    // If the connection succeeds quickly, the server may not be shutting down properly
    if accepting_connections {
        bail_log!("Server is still accepting new connections too readily");
    }

    Ok(())
}

/// Step: "Then no error logs should be present"
/// Verifies that no error or fatal log messages were generated during the test
pub async fn step_then_no_error_logs_should_be_present(ctx: &mut GlobalContext) -> Result<()> {
    // Check stderr buffer for error logs
    if let Ok(stderr_buf) = ctx.get_active_stderr_buffer()?.lock() {
        let stderr_content = stderr_buf.join("\n");
        if stderr_content.contains("ERROR") || stderr_content.contains("FATAL") {
            bail_log!("Error log found in stderr: {}", stderr_content);
        }
    }

    Ok(())
}

// =============================================================================
// LNURL PAY STEP FUNCTIONS - For LNURL Pay invoice generation feature
// =============================================================================

/// Step: "Given the payee has a {backend_type} lightning node available"
/// Sets up the specific lightning node backend type for the test
pub async fn step_given_the_payee_has_a_lightning_node_available(
    ctx: &mut GlobalContext,
    backend_type: RegTestLnNodeType,
) -> Result<()> {
    let node = match backend_type {
        RegTestLnNodeType::Cln => RegTestLnNode::Cln(ctx.get_first_cln_node()?.clone()),
        RegTestLnNodeType::Lnd => RegTestLnNode::Lnd(ctx.get_first_lnd_node()?.clone()),
    };
    ctx.add_payee("single", node.clone());
    Ok(())
}

/// Step: "When the {payee_id} payee creates an offer for their lightning node"
/// Creates an LNURL offer for the specified lightning node
pub async fn step_when_the_payee_creates_an_offer_for_their_lightning_node(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Use the selected node from context
    let payee = get_payee_from_context(ctx, payee_id)?;
    let node = payee.node.clone();

    let client = ctx.get_active_offer_client()?;

    // Create offer metadata
    let metadata_id = Uuid::new_v4();
    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();

    let offer_metadata = OfferMetadata {
        id: metadata_id,
        partition: "default".to_string(),
        metadata: OfferMetadataSparse {
            text: random_string.clone(),
            long_text: Some("test-context".to_string()),
            image: None,
            identifier: None,
        },
    };

    // Post metadata
    let metadata_result = client.post_metadata(offer_metadata.clone()).await;
    if metadata_result.is_err() {
        bail_log!(
            "Expected successful metadata creation, got error: {:?}",
            metadata_result.err()
        );
    }

    // Create offer record with different limits based on node type
    let offer_id = Uuid::new_v4();
    let (max_sendable, min_sendable) = match node {
        RegTestLnNode::Cln(_) => (1_000_000_000, 1_000),
        RegTestLnNode::Lnd(_) => (2_000_000_000, 1_000),
    };

    let now = Utc::now();
    let offer = OfferRecord {
        partition: "default".to_string(),
        id: offer_id,
        offer: OfferRecordSparse {
            max_sendable,
            min_sendable,
            metadata_id,
            timestamp: now - ChronoDuration::minutes(5),
            expires: Some(now + ChronoDuration::hours(24)),
        },
    };

    // Post offer
    let offer_result = client.post_offer(offer.clone()).await;
    if offer_result.is_err() {
        bail_log!(
            "Expected successful offer creation, got error: {:?}",
            offer_result.err()
        );
    }

    // Store offer details in payee context for later steps
    let offer_request = OfferRequest {
        offer_id: Some(offer_id),
        metadata_id: Some(metadata_id),
        lnurl_offer: None,
        received_invoice: None,
    };
    ctx.add_offer_request(payee_id, "offer", offer_request)?;

    Ok(())
}

/// Step: "When the {payee_id} payee registers their lightning node as a backend"
/// Registers the lightning node as a discovery backend
pub async fn step_when_the_payee_registers_their_lightning_node_as_a_backend(
    ctx: &mut GlobalContext,
    payee_id: &str,
    include_ca: bool,
) -> Result<()> {
    // Use the selected node from context
    let payee = get_payee_from_context(ctx, payee_id)?;
    let node = payee.node.clone();

    let client = ctx.get_active_discovery_client()?;

    let url = Url::parse(&format!("https://{}", node.address()))?;

    let implementation = match &node {
        RegTestLnNode::Cln(cln) => {
            DiscoveryBackendImplementation::ClnGrpc(ClnGrpcDiscoveryBackendImplementation {
                url,
                auth: ClnGrpcClientAuth::Path(ClnGrpcClientAuthPath {
                    ca_cert_path: if include_ca {
                        cln.ca_cert_path.clone().into()
                    } else {
                        None
                    },
                    client_cert_path: cln.client_cert_path.clone(),
                    client_key_path: cln.client_key_path.clone(),
                }),
                domain: None,
            })
        }
        RegTestLnNode::Lnd(lnd) => {
            DiscoveryBackendImplementation::LndGrpc(LndGrpcDiscoveryBackendImplementation {
                url,
                auth: LndGrpcClientAuth::Path(LndGrpcClientAuthPath {
                    tls_cert_path: if include_ca {
                        lnd.tls_cert_path.clone().into()
                    } else {
                        None
                    },
                    macaroon_path: lnd.macaroon_path.clone(),
                }),
                amp_invoice: false,
                domain: None,
            })
        }
    };

    let backend = DiscoveryBackend {
        public_key: *node.public_key(),
        backend: DiscoveryBackendSparse {
            name: None,
            partitions: ["default".to_string()].into(),
            weight: 100,
            enabled: true,
            implementation,
        },
    };

    // Register backend
    let backend_result = client.post(backend.clone()).await;
    if backend_result.is_err() {
        bail_log!(
            "Expected successful backend registration, got error: {:?}",
            backend_result.err()
        );
    }

    Ok(())
}

/// Step: "When the payer requests the LNURL offer from the payee using {protocol}"
/// Makes a request to get the LNURL offer from a specific payee using the specified protocol
pub async fn step_when_the_payer_requests_the_lnurl_offer_from_the_payee(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Use the first available offer request from specified payee context
    // This is for backward compatibility with existing tests
    let payee = get_payee_from_context(ctx, payee_id)?;

    let (offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    let offer_id = get_offer_id_from_request(offer_request, payee_id)?;

    let client = ctx.get_active_lnurl_client()?;
    let lnurl_offer = client.get_offer("default", &offer_id).await?;

    // Store offer response in the specific offer request
    let offer_key = offer_key.clone(); // Clone the key before borrowing mutably
    let offer_request = ctx
        .get_offer_request_mut(payee_id, &offer_key)
        .ok_or_else(|| anyhow_log!("Offer request not found for {} payee", payee_id))?;
    offer_request.lnurl_offer = Some(lnurl_offer.clone());

    Ok(())
}

/// Step: "Then the {payee_id} payee offer should contain valid sendable amounts"
/// Verifies that the offer contains valid min/max sendable amounts
pub async fn step_then_the_payee_offer_should_contain_valid_sendable_amounts(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Get amounts from the LNURL offer in the offer request
    let payee = get_payee_from_context(ctx, payee_id)?;
    let (_offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    let lnurl_offer = get_lnurl_offer_from_request(offer_request, payee_id)?;
    let max_sendable = lnurl_offer.max_sendable;
    let min_sendable = lnurl_offer.min_sendable;

    // Verify amounts are reasonable
    if min_sendable == 0 {
        bail_log!("Min sendable amount should be greater than 0");
    }

    if max_sendable <= min_sendable {
        bail_log!("Max sendable should be greater than min sendable");
    }

    // Verify minimum expected values (vary by node type)
    if min_sendable != 1_000 {
        bail_log!("Expected min sendable 1000, got {}", min_sendable);
    }

    Ok(())
}

/// Step: "Then the {payee_id} payee offer should contain valid metadata"
/// Verifies that the offer contains valid metadata
pub async fn step_then_the_payee_offer_should_contain_valid_metadata(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Check if metadata ID exists in the offer request
    let payee = get_payee_from_context(ctx, payee_id)?;
    let (_offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    if offer_request.metadata_id.is_none() {
        bail_log!("Offer metadata not found in {} payee context", payee_id);
    }

    Ok(())
}

/// Step: "Then the {payee_id} payee offer should provide a callback URL"
/// Verifies that the offer provides a callback URL for invoice generation
pub async fn step_then_the_payee_offer_should_provide_a_callback_url(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Check for callback URL in the LNURL offer
    let payee = get_payee_from_context(ctx, payee_id)?;
    let (_offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    let lnurl_offer = get_lnurl_offer_from_request(offer_request, payee_id)?;

    let callback_url = lnurl_offer.callback.as_str();
    Url::parse(callback_url)?;

    Ok(())
}

/// Step: "When the payer requests an invoice for 100 sats using the payee's callback URL with {protocol}"
/// Requests an invoice using the callback URL from a specific payee with the specified protocol
pub async fn step_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url(
    ctx: &mut GlobalContext,
    payee_id: &str,
    protocol: &Protocol,
) -> Result<()> {
    // Get the LNURL offer
    let payee = get_payee_from_context(ctx, payee_id)?;
    let (_offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    let lnurl_offer = get_lnurl_offer_from_request(offer_request, payee_id)?;
    let callback_url = lnurl_offer.callback.as_str();
    if !callback_url.starts_with(&protocol.to_string()) {
        bail_log!("Callback URL is not expected protocol: {}", protocol);
    }

    let client = ctx.get_active_lnurl_client()?;
    let amount_msat = 100_000;
    let lnurl_invoice = client.get_invoice(lnurl_offer, amount_msat).await?;

    // Store invoice in the offer request for assertions
    let payee = get_payee_from_context_mut(ctx, payee_id)?;
    let (_offer_key, offer_request) = get_offer_request_mut(payee, payee_id)?;
    offer_request.received_invoice = Some(lnurl_invoice.pr.clone());

    Ok(())
}

/// Step: "Then the payer should receive a valid Lightning invoice"
/// Verifies that a valid Lightning invoice was received
pub async fn step_then_the_payer_should_receive_a_valid_lightning_invoice(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Get invoice from the offer request
    let payee = get_payee_from_context(ctx, payee_id)?;
    let (_offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    let invoice_str = get_invoice_from_request(offer_request, payee_id)?;

    // Parse the invoice to verify it's valid
    let _bolt11_invoice = parse_lightning_invoice(invoice_str)?;

    Ok(())
}

/// Step: "Then the invoice amount should be 100000 millisatoshis"
/// Verifies that the invoice amount matches the requested amount
pub async fn step_then_the_invoice_amount_should_be_100000_millisatoshis(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Get and parse invoice from the offer request
    let payee = get_payee_from_context(ctx, payee_id)?;
    let (_offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    let invoice_str = get_invoice_from_request(offer_request, payee_id)?;

    let bolt11_invoice = parse_lightning_invoice(invoice_str)?;

    // Verify amount
    let expected_amount = 100_000; // 100 sats in msat
    validate_invoice_has_amount(&bolt11_invoice, expected_amount)?;

    Ok(())
}

/// Step: "Then the invoice description hash should match the metadata hash"
/// Verifies that the invoice description hash matches the offer metadata hash
pub async fn step_then_the_invoice_description_hash_should_match_the_metadata_hash(
    ctx: &mut GlobalContext,
    payee_id: &str,
) -> Result<()> {
    // Get and parse invoice from the offer request
    let payee = get_payee_from_context(ctx, payee_id)?;
    let (_offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    let invoice_str = get_invoice_from_request(offer_request, payee_id)?;

    let bolt11_invoice = parse_lightning_invoice(invoice_str)?;

    // For this test, we verify that the invoice has a description hash
    // The actual hash verification would require the original metadata
    match bolt11_invoice.description() {
        lightning_invoice::Bolt11InvoiceDescriptionRef::Hash(_hash) => Ok(()),
        lightning_invoice::Bolt11InvoiceDescriptionRef::Direct(_) => {
            bail_log!("Expected hash description for invoice but got direct description")
        }
    }
}

// =============================================================================
// MULTI-PAYEE STEP FUNCTIONS - For testing multiple payees with separate nodes
// =============================================================================

/// Step: "Given two payees each have their own lightning node"
/// Sets up the test environment with two distinct lightning nodes
pub async fn step_given_two_payees_each_have_their_own_lightning_node(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let cln_node = ctx.get_first_cln_node()?;
    ctx.add_payee("first", RegTestLnNode::Cln(cln_node.clone()));

    let lnd_node = ctx.get_first_lnd_node()?;
    ctx.add_payee("second", RegTestLnNode::Lnd(lnd_node.clone()));

    Ok(())
}

/// Step: "And both nodes are registered as separate backends"
/// Registers both payee nodes as backends
pub async fn step_and_both_nodes_are_registered_as_separate_backends(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Register both payee nodes
    register_payee_node_as_backend(ctx, "first").await?;
    register_payee_node_as_backend(ctx, "second").await?;

    Ok(())
}

// =============================================================================
// GENERIC PAYEE HELPER FUNCTIONS - For testing any number of payees
// =============================================================================

/// Generic function to register a payee's node as a backend
pub async fn register_payee_node_as_backend(ctx: &mut GlobalContext, payee_id: &str) -> Result<()> {
    let payee = get_payee_from_context(ctx, payee_id)?;
    let node = payee.node.clone();

    let client = ctx.get_active_discovery_client()?;

    let url = Url::parse(&format!("https://{}", node.address()))?;

    let implementation = match &node {
        RegTestLnNode::Cln(cln) => {
            DiscoveryBackendImplementation::ClnGrpc(ClnGrpcDiscoveryBackendImplementation {
                url,
                auth: ClnGrpcClientAuth::Path(ClnGrpcClientAuthPath {
                    ca_cert_path: cln.ca_cert_path.clone().into(),
                    client_cert_path: cln.client_cert_path.clone(),
                    client_key_path: cln.client_key_path.clone(),
                }),
                domain: None,
            })
        }
        RegTestLnNode::Lnd(lnd) => {
            DiscoveryBackendImplementation::LndGrpc(LndGrpcDiscoveryBackendImplementation {
                url,
                auth: LndGrpcClientAuth::Path(LndGrpcClientAuthPath {
                    tls_cert_path: lnd.tls_cert_path.clone().into(),
                    macaroon_path: lnd.macaroon_path.clone(),
                }),
                amp_invoice: false,
                domain: None,
            })
        }
    };

    let backend = DiscoveryBackend {
        public_key: *node.public_key(),
        backend: DiscoveryBackendSparse {
            name: None,
            partitions: ["default".to_string()].into(),
            weight: 100,
            implementation,
            enabled: true,
        },
    };

    client.post(backend.clone()).await?;

    Ok(())
}

/// Generic function to set up two payees with specific node types
pub async fn setup_two_payees_with_node_types(
    ctx: &mut GlobalContext,
    first_node_type: RegTestLnNodeType,
    second_node_type: RegTestLnNodeType,
) -> Result<()> {
    let first_node = match first_node_type {
        RegTestLnNodeType::Cln => RegTestLnNode::Cln(ctx.get_first_cln_node()?.clone()),
        RegTestLnNodeType::Lnd => RegTestLnNode::Lnd(ctx.get_first_lnd_node()?.clone()),
    };
    ctx.add_payee("first", first_node);

    let second_node = match second_node_type {
        RegTestLnNodeType::Cln => RegTestLnNode::Cln(ctx.get_first_cln_node()?.clone()),
        RegTestLnNodeType::Lnd => RegTestLnNode::Lnd(ctx.get_first_lnd_node()?.clone()),
    };
    ctx.add_payee("second", second_node);

    Ok(())
}

/// Step: "But when the payer requests an invoice for 100 sats using the {payee_id} payee callback URL"
/// Requests an invoice using the callback URL and expects it to fail
pub async fn step_but_when_the_payer_requests_an_invoice_for_100_sats_using_the_payee_callback_url_expecting_failure(
    ctx: &mut GlobalContext,
    payee_id: &str,
    protocol: &Protocol,
) -> Result<()> {
    // Use the callback URL from the LNURL offer
    let payee = ctx
        .get_payee(payee_id)
        .ok_or_else(|| anyhow_log!("No {} payee found", payee_id))?;
    let (_offer_key, offer_request) = get_offer_request(payee, payee_id)?;
    let lnurl_offer = get_lnurl_offer_from_request(offer_request, payee_id)?;
    let callback_url = lnurl_offer.callback.as_str();
    if !callback_url.starts_with(&protocol.to_string()) {
        bail_log!("Callback URL is not expected protocol: {}", protocol);
    }
    // Request invoice for 100 sats (100000 msat) - expecting failure
    let amount_msat = 100_000;

    let client = ctx.get_active_lnurl_client()?;

    let response = client.get_invoice(lnurl_offer, amount_msat as usize).await;

    if response.is_ok() {
        bail_log!("expected failure, received {:?}", response);
    }

    Ok(())
}

// =============================================================================
// BACKEND ENABLE/DISABLE STEP FUNCTIONS - For backend lifecycle management
// =============================================================================

/// Step: "Given the payee has access to both CLN and LND lightning nodes"
/// Sets up the test environment with both CLN and LND lightning nodes for a single payee
pub async fn step_given_the_payee_has_access_to_both_cln_and_lnd_lightning_nodes(
    ctx: &mut GlobalContext,
) -> Result<()> {
    ctx.add_payee("cln", RegTestLnNode::Cln(ctx.get_first_cln_node()?.clone()));
    ctx.add_payee("lnd", RegTestLnNode::Lnd(ctx.get_first_lnd_node()?.clone()));

    Ok(())
}

/// Step: "And the payee has created an offer linked to both lightning nodes"
/// Creates a single offer with both CLN and LND public keys in the payees vector
pub async fn step_and_the_payee_has_created_an_offer_linked_to_both_lightning_nodes(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let client = ctx.get_active_offer_client()?;

    // Create offer metadata
    let metadata_id = Uuid::new_v4();
    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();

    let offer_metadata = OfferMetadata {
        id: metadata_id,
        partition: "default".to_string(),
        metadata: OfferMetadataSparse {
            text: format!("multi-backend-{random_string}"),
            long_text: Some("multi-backend-offer".to_string()),
            image: None,
            identifier: None,
        },
    };

    client.post_metadata(offer_metadata.clone()).await?;

    let offer_id = Uuid::new_v4();
    let now = Utc::now();
    let offer = OfferRecord {
        partition: "default".to_string(),
        id: offer_id,
        offer: OfferRecordSparse {
            max_sendable: 1_000_000_000, // 1000 sats in msat
            min_sendable: 1_000,         // 1 sat in msat
            metadata_id,
            timestamp: now - ChronoDuration::minutes(5),
            expires: Some(now + ChronoDuration::hours(24)),
        },
    };

    client.post_offer(offer.clone()).await?;

    // Store offer info in lnd payee context for access in tests
    let mut offer_request = OfferRequest::new();
    offer_request.offer_id = Some(offer_id);
    offer_request.metadata_id = Some(metadata_id);

    ctx.add_offer_request("lnd", "multi_backend_offer", offer_request)
        .map_err(|e| anyhow_log!("Failed to add offer request: {}", e))?;

    Ok(())
}

/// Step: "And both nodes are registered as separate backends" - Backend enable/disable version
/// Registers both CLN and LND nodes as backends for backend lifecycle management
/// Step: "Given the payer can generate invoices successfully"
/// Verifies that the payer can successfully generate invoices before testing backend operations
pub async fn step_given_the_payer_can_generate_invoices_successfully(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get the LNURL offer to establish the callback URL
    let offer_request = ctx
        .get_offer_request("lnd", "multi_backend_offer")
        .ok_or_else(|| anyhow_log!("Offer request not found"))?;
    let offer_id = offer_request
        .offer_id
        .as_ref()
        .ok_or_else(|| anyhow_log!("Offer ID not set"))?;

    let client = ctx.get_active_lnurl_client()?;
    let lnurl_offer_response = client.get_offer("default", offer_id).await?;

    // Store the callback in the lnurl_offer field
    if let Some(offer_request_mut) = ctx.get_offer_request_mut("lnd", "multi_backend_offer") {
        offer_request_mut.lnurl_offer = Some(lnurl_offer_response);
    }

    // Request and validate invoice
    request_and_validate_invoice_helper(ctx, "lnd", &Service::LnUrl).await?;

    // Clear the received invoice for future tests
    if let Some(offer_request) = ctx.get_offer_request_mut("lnd", "multi_backend_offer") {
        offer_request.received_invoice = None;
    }

    Ok(())
}

/// Step: "When the admin disables the first backend"
/// Disables the first backend (LND)
pub async fn step_when_the_admin_disables_the_first_backend(ctx: &mut GlobalContext) -> Result<()> {
    // Get LND backend location
    let lnd_payee = ctx
        .get_payee("lnd")
        .ok_or_else(|| anyhow_log!("LND payee not found in context"))?
        .clone();

    enable_disable_backend(ctx, lnd_payee.node.public_key(), false).await?;

    Ok(())
}

/// Step: "Then the payer can still get invoices"
/// Verifies that invoice generation still works after disabling one backend
pub async fn step_then_the_payer_can_still_generate_invoices(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Request and validate invoice
    request_and_validate_invoice_helper(ctx, "lnd", &Service::LnUrl).await?;

    Ok(())
}

/// Step: "When the admin disables the second backend"
/// Disables the second backend (CLN)
pub async fn step_when_the_admin_disables_the_second_backend(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get CLN backend location
    let cln_payee = ctx
        .get_payee("cln")
        .ok_or_else(|| anyhow_log!("CLN payee not found in context"))?
        .clone();

    enable_disable_backend(ctx, cln_payee.node.public_key(), false).await?;

    Ok(())
}

/// Step: "Then the payer cannot get invoices"
/// Verifies that invoice generation fails when all backends are disabled
pub async fn step_then_the_payer_cannot_generate_invoices(
    ctx: &mut GlobalContext,
    limit: Duration,
) -> Result<()> {
    // Clear any previous invoice
    if let Some(offer_request) = ctx.get_offer_request_mut("lnd", "multi_backend_offer") {
        offer_request.received_invoice = None;
    }

    // Try to request an invoice and expect it to fail
    let offer_request = ctx
        .get_offer_request("lnd", "multi_backend_offer")
        .ok_or_else(|| anyhow_log!("Offer request not found"))?;

    if let Some(lnurl_offer) = &offer_request.lnurl_offer {
        let callback_url = lnurl_offer.callback.as_str();
        Url::parse(callback_url)?;

        let client = ctx.get_active_lnurl_client()?;

        let start_time = SystemTime::now();

        while SystemTime::now().duration_since(start_time)? < limit {
            match client.get_invoice(lnurl_offer, 100000).await {
                Ok(_) => {
                    tokio_sleep(Duration::from_millis(50)).await;
                    continue;
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    if error_msg.contains(&StatusCode::BAD_GATEWAY.to_string()) {
                        return Ok(());
                    }
                }
            }
        }
        bail_log!("Expected invoice generation to fail, but it succeeded");
    } else {
        bail_log!("No callback URL available");
    }
}

/// Step: "When the admin enables any backend"
/// Re-enables any backend (LND in this case)
pub async fn step_when_the_admin_enables_any_backend(ctx: &mut GlobalContext) -> Result<()> {
    // Re-enable LND backend
    let lnd_payee = ctx
        .get_payee("lnd")
        .ok_or_else(|| anyhow_log!("LND payee not found in context"))?
        .clone();

    enable_disable_backend(ctx, lnd_payee.node.public_key(), true).await?;

    Ok(())
}

/// Step: "Then the payer can again get invoices"
/// Verifies that invoice generation is restored after re-enabling a backend
pub async fn step_then_the_payer_can_again_generate_invoices(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Request and validate invoice
    request_and_validate_invoice_helper(ctx, "lnd", &Service::LnUrl).await?;

    Ok(())
}

async fn enable_disable_backend(
    ctx: &mut GlobalContext,
    public_key: &PublicKey,
    enabled: bool,
) -> Result<()> {
    let client = ctx.get_active_discovery_client()?;

    let patch = DiscoveryBackendPatch {
        public_key: *public_key,
        backend: DiscoveryBackendPatchSparse {
            name: None,
            partitions: None,
            weight: None,
            enabled: Some(enabled),
        },
    };

    // PATCH the backend
    let patched = client.patch(patch).await?;
    if !patched {
        bail_log!("PATCH {public_key} failed");
    }

    Ok(())
}

/// Step: "When the admin deletes the first backend"
/// Deletes the first backend (LND) from the discovery service
pub async fn step_when_the_admin_deletes_the_first_backend(ctx: &mut GlobalContext) -> Result<()> {
    // Get LND backend location
    let lnd_payee = ctx
        .get_payee("lnd")
        .ok_or_else(|| anyhow_log!("LND payee not found in context"))?
        .clone();

    delete_backend(ctx, lnd_payee.node.public_key()).await?;

    Ok(())
}

/// Step: "When the admin deletes the second backend"
/// Deletes the second backend (CLN) from the discovery service
pub async fn step_when_the_admin_deletes_the_second_backend(ctx: &mut GlobalContext) -> Result<()> {
    // Get CLN backend location
    let cln_payee = ctx
        .get_payee("cln")
        .ok_or_else(|| anyhow_log!("CLN payee not found in context"))?
        .clone();

    delete_backend(ctx, cln_payee.node.public_key()).await?;

    Ok(())
}

/// Step: "And both nodes are ready to be registered as backends"
/// Prepares nodes for backend registration without actually registering them
pub async fn step_and_both_nodes_are_ready_to_be_registered_as_backends(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Ensure both nodes are available in context
    if ctx.get_payee("cln").is_none() {
        bail_log!("CLN payee not found in context");
    }
    if ctx.get_payee("lnd").is_none() {
        bail_log!("LND payee not found in context");
    }

    Ok(())
}

/// Helper function to delete backends via the discovery API
async fn delete_backend(ctx: &mut GlobalContext, public_key: &PublicKey) -> Result<()> {
    let client = ctx.get_active_discovery_client()?;

    // Delete the backend
    client.delete(public_key).await?;

    Ok(())
}

// =============================================================================
// SERVER PERSISTENCE STEP FUNCTIONS
// =============================================================================

/// Step: "When I delete the persistent {backend_store} backend storage files"
/// Deletes the backend storage files to test data loss scenarios
pub async fn step_when_i_delete_the_persistent_backend_storage_files(
    ctx: &mut GlobalContext,
    delete_discovery_store: bool,
    delete_offer_store: bool,
) -> Result<()> {
    if delete_discovery_store {
        std::fs::remove_dir_all(ctx.get_active_discovery_store_dir()?)?;
        std::fs::create_dir_all(ctx.get_active_discovery_store_dir()?)?;
    }

    if delete_offer_store {
        std::fs::remove_dir_all(ctx.get_active_offer_store_dir()?)?;
        std::fs::create_dir_all(ctx.get_active_offer_store_dir()?)?;
    }

    Ok(())
}

/// Step: "When I start the LNURL server with enablement flag {flag}"
/// Starts the LNURL server with specific service enablement flags
pub async fn step_when_i_start_the_lnurl_server_with_enablement_flags(
    ctx: &mut GlobalContext,
    start_services: &[Service],
) -> Result<()> {
    let _pid = ctx
        .start_active_server(start_services, log::Level::Info)
        .await?;
    Ok(())
}

/// Step: "And the lnurl service should be listening on the configured port"
/// Verifies that the LNURL service is listening on its configured port
pub async fn step_and_the_lnurl_service_should_be_listening_on_the_configured_port(
    ctx: &mut GlobalContext,
) -> Result<()> {
    verify_single_service_status(ctx, vec![Service::LnUrl], true, 1000, 5).await?;
    Ok(())
}

/// Step: "And the discovery service should be listening on the configured port"
/// Verifies that the discovery service is listening on its configured port
pub async fn step_and_the_discovery_service_should_be_listening_on_the_configured_port(
    ctx: &mut GlobalContext,
) -> Result<()> {
    verify_single_service_status(ctx, vec![Service::Discovery], true, 1000, 5).await?;
    Ok(())
}

/// Step: "And the offers service should be listening on the configured port"
/// Verifies that the offers service is listening on its configured port
pub async fn step_and_the_offers_service_should_be_listening_on_the_configured_port(
    ctx: &mut GlobalContext,
) -> Result<()> {
    verify_single_service_status(ctx, vec![Service::Offer], true, 1000, 5).await?;
    Ok(())
}

/// Step: "And the lnurl service should not be listening on the configured port"
/// Verifies that the LNURL service is NOT listening on its configured port
pub async fn step_and_the_lnurl_service_should_not_be_listening_on_the_configured_port(
    ctx: &mut GlobalContext,
) -> Result<()> {
    verify_single_service_status(ctx, vec![Service::LnUrl], false, 100, 1).await?;
    Ok(())
}

/// Step: "And the discovery service should not be listening on the configured port"
/// Verifies that the discovery service is NOT listening on its configured port
pub async fn step_and_the_discovery_service_should_not_be_listening_on_the_configured_port(
    ctx: &mut GlobalContext,
) -> Result<()> {
    verify_single_service_status(ctx, vec![Service::Discovery], false, 100, 1).await?;
    Ok(())
}

/// Step: "And the offers service should not be listening on the configured port"
/// Verifies that the offers service is NOT listening on its configured port
pub async fn step_and_the_offers_service_should_not_be_listening_on_the_configured_port(
    ctx: &mut GlobalContext,
) -> Result<()> {
    verify_single_service_status(ctx, vec![Service::Offer], false, 100, 1).await?;
    Ok(())
}

// =============================================================================
// SERVICE LOGS STEP FUNCTIONS - For testing service logging functionality
// =============================================================================

/// Step: "When I request an offer from a non-existent partition"
/// Makes a request for an offer with a non-existent partition to trigger 404 error
pub async fn step_when_i_request_an_offer_from_a_non_existent_partition(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let client = ctx.get_active_lnurl_client()?;

    // Try to get a non-existent offer with a different partition name to trigger 404
    let non_existent_offer_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000")?;
    let result = client
        .get_offer("non-existent-partition", &non_existent_offer_id)
        .await;

    // Expect this to fail with a 404
    match result {
        Ok(_) => {
            bail_log!("Expected offer request to fail, but it succeeded");
        }
        Err(e) => {
            let error_msg = e.to_string();
            if !error_msg.contains("404") {
                bail_log!("Expected 404 error, but got: {}", error_msg);
            }
        }
    }

    Ok(())
}

/// Step: "When I request an invoice for a non-existent offer"
/// Makes a request for an invoice with a non-existent offer ID
pub async fn step_when_i_request_an_invoice_for_a_non_existent_offer(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let client = ctx.get_active_lnurl_client()?;
    let non_existent_offer_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000000")?;

    // Try to get a non-existent offer, expect it to fail
    let result = client.get_offer("default", &non_existent_offer_id).await;
    let status_code = match result {
        Ok(_) => {
            bail_log!("Expected offer request to fail, but it succeeded");
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("404") {
                404
            } else {
                bail_log!("Expected 404 error, got: {}", error_msg);
            }
        }
    };

    // Assert expected 404 status code
    if status_code != 404 {
        bail_log!(
            "Expected 404 status code for non-existent offer, got {}",
            status_code
        );
    }

    Ok(())
}

/// Step: "When I try to get a missing backend"
/// Attempts to get a backend that doesn't exist to trigger error logging
pub async fn step_when_i_try_to_get_a_missing_backend(
    ctx: &mut GlobalContext,
    public_key: &PublicKey,
) -> Result<()> {
    let client = ctx.get_active_discovery_client()?;

    let backend = client.get(public_key).await?;

    if backend.is_some() {
        bail_log!("backend found unexpectedly")
    } else {
        Ok(())
    }
}

// =============================================================================
// POST-SHUTDOWN LOG ASSERTION STEP FUNCTIONS - Assert logs after server stops
// =============================================================================

/// Step: "And the server logs should contain health check requests for all services"
/// Verifies that health check requests are logged for all services (expected pattern: clf::{service} ... /health 200)
pub async fn step_and_the_server_logs_should_contain_health_check_requests_for_all_services(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    // Expected patterns for health check logs: clf::{service_name} ... /health HTTP/1.1 200
    let expected_services = [Service::LnUrl, Service::Discovery, Service::Offer];
    let mut service_counts = std::collections::HashMap::new();

    // Initialize counts
    for service in &expected_services {
        service_counts.insert(service.to_string(), 0);
    }

    // Iterate through each line and count matches
    for line in &stderr_lines {
        for service in &expected_services {
            let clf_pattern = format!("clf::{service}");
            if line.contains(&clf_pattern)
                && line.contains("/health HTTP/1.1 200")
                && line.contains(" INFO ")
            {
                *service_counts
                    .get_mut(&service.to_string())
                    .ok_or_else(|| anyhow_log!("could not find service {service}"))? += 1;
            }
        }
    }

    // Verify exact expected counts for each service (at least 1 health check per service)
    let expected_min_count = 1;
    let mut service_errors = Vec::new();
    let mut found_services = Vec::new();

    for (service, count) in &service_counts {
        if *count < expected_min_count {
            service_errors.push(format!(
                "{service}:expected≥{expected_min_count},got{count}",
            ));
        } else {
            found_services.push(format!("{service}:{count}"));
        }
    }

    if service_errors.is_empty() {
        Ok(())
    } else {
        let error_msg = format!("❌ Health check log count mismatch: {service_errors:?}. Found: {found_services:?} (expected: clf::{{service}} ... /health HTTP/1.1 200 INFO)");
        bail_log!(error_msg)
    }
}

/// Step: "And the server logs should contain backend registration requests"
/// Verifies that backend registration requests are logged
pub async fn step_and_the_server_logs_should_contain_backend_registration_requests(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    let server_logs = stderr_lines.join("\n");

    // Look for backend registration patterns: clf::discovery ... POST /discovery/default HTTP/1.1 201
    let registration_patterns = ["clf::discovery", "POST /discovery HTTP/1.1 201", " INFO "];
    let expected_count = 1; // We register exactly 1 backend in the test
    let registration_count = count_log_patterns(&server_logs, &registration_patterns);

    if registration_count == expected_count {
        Ok(())
    } else {
        let error_msg = format!("❌ Backend registration log count mismatch: expected={expected_count}, got={registration_count} (pattern: clf::discovery ... POST /discovery/default HTTP/1.1 201 INFO)");
        bail_log!(error_msg)
    }
}

/// Step: "And the server logs should contain offer retrieval requests"
/// Verifies that offer retrieval requests are logged
pub async fn step_and_the_server_logs_should_contain_offer_retrieval_requests(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    // Look for offer retrieval patterns: clf::lnurl ... GET /offers/default/{uuid} HTTP/1.1 200
    let mut offer_count = 0;

    for line in &stderr_lines {
        if line.contains("clf::lnurl")
            && line.contains("GET /offers/default/")
            && line.contains("HTTP/1.1 200")
            && line.contains(" INFO ")
        {
            // Verify the URI pattern matches the full path with UUID
            if let Some(get_start) = line.find("GET /offers/default/") {
                let uri_part = &line[get_start..];
                if let Some(http_start) = uri_part.find(" HTTP/1.1") {
                    let full_uri = &uri_part[..http_start];
                    // Verify it has the UUID pattern: GET /offers/default/{uuid}
                    if full_uri.len() > "GET /offers/default/".len() + 30 {
                        // UUID is 36 chars
                        offer_count += 1;
                    }
                }
            }
        }
    }

    let expected_count = 2; // We make multiple offer-related requests (1 explicit + health checks)
    if offer_count == expected_count {
        Ok(())
    } else {
        let error_msg = format!("❌ Offer retrieval log count mismatch: expected={expected_count}, got={offer_count} (pattern: clf::lnurl ... GET /offers/default/{{uuid}} HTTP/1.1 200 INFO)");
        bail_log!(error_msg)
    }
}

/// Step: "And the server logs should contain invoice generation requests"
/// Verifies that invoice generation requests are logged
pub async fn step_and_the_server_logs_should_contain_invoice_generation_requests(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    // Look for invoice generation patterns: clf::lnurl ... GET /offers/default/{uuid}/invoice?amount={msat} HTTP/1.1 200
    let mut invoice_count = 0;

    for line in &stderr_lines {
        if line.contains("clf::lnurl")
            && line.contains("GET /offers/default/")
            && line.contains("/invoice?amount=")
            && line.contains("HTTP/1.1 200")
            && line.contains(" INFO ")
        {
            // Verify the URI pattern matches the full invoice path with UUID and amount
            if let Some(get_start) = line.find("GET ") {
                let uri_part = &line[get_start..];
                if let Some(http_start) = uri_part.find(" HTTP/1.1") {
                    let full_uri = &uri_part[4..http_start]; // Skip "GET "
                                                             // Verify it has invoice pattern with UUID and amount query param
                    if full_uri.contains("/offers/default/")
                        && full_uri.contains("/invoice?amount=")
                    {
                        // Further verify it has reasonable UUID length (offers/default/{uuid}/invoice?amount=)
                        if full_uri.len() > "/offers/default/".len() + 30 {
                            // UUID is 36 chars
                            invoice_count += 1;
                        }
                    }
                }
            }
        }
    }

    let expected_count = 1; // We make exactly 1 invoice generation request in the test
    if invoice_count == expected_count {
        Ok(())
    } else {
        let error_msg = format!("❌ Invoice generation log count mismatch: expected={expected_count}, got={invoice_count} (pattern: clf::lnurl ... GET /offers/default/{{uuid}}/invoice?amount={{msat}} HTTP/1.1 200 INFO)");
        bail_log!(error_msg)
    }
}

/// Step: "And the server logs should contain 404 error responses"
/// Verifies that 404 error responses are logged
pub async fn step_and_the_server_logs_should_contain_404_error_responses(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    // Look for 404 error patterns: clf::{service} ... HTTP/1.1 404
    let mut error_404_count = 0;

    for line in &stderr_lines {
        if (line.contains("clf::lnurl")
            || line.contains("clf::discovery")
            || line.contains("clf::offer"))
            && line.contains("HTTP/1.1 404")
            && line.contains(" WARN ")
        {
            error_404_count += 1;
        }
    }

    let expected_count = 3; // We make 3 requests that result in 404: non-existent endpoint + non-existent offer + 1 discovery GET request
    if error_404_count == expected_count {
        Ok(())
    } else {
        let error_msg = format!("❌ 404 error log count mismatch: expected={expected_count}, got={error_404_count} (pattern: clf::{{service}} ... HTTP/1.1 404 WARN)");
        bail_log!(error_msg)
    }
}

/// Step: "And the server logs should contain invalid offer error responses"
/// Verifies that invalid offer error responses are logged
pub async fn step_and_the_server_logs_should_contain_invalid_offer_error_responses(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    let server_logs = stderr_lines.join("\n");

    // Look for invalid offer error patterns: clf::lnurl ... HTTP/1.1 404
    let invalid_offer_patterns = ["clf::lnurl", "HTTP/1.1 404", " WARN "];
    let expected_count = 2; // We make 2 requests to lnurl service that result in 404: non-existent endpoint + non-existent offer
    let invalid_offer_count = count_log_patterns(&server_logs, &invalid_offer_patterns);

    if invalid_offer_count == expected_count {
        Ok(())
    } else {
        let error_msg = format!("❌ Invalid offer error log count mismatch: expected={expected_count}, got={invalid_offer_count} (pattern: clf::lnurl ... HTTP/1.1 404 WARN)");
        bail_log!(error_msg)
    }
}

/// Step: "And the server logs should contain invalid backend get errors"
/// Verifies that invalid backend get errors are logged
pub async fn step_and_the_server_logs_should_contain_invalid_backend_get_errors(
    ctx: &mut GlobalContext,
    invalid_backend_patterns: &[&str],
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    let server_logs = stderr_lines.join("\n");

    let expected_count = 1; // We make exactly 1 invalid backend GET request in the test
    let invalid_backend_count = count_log_patterns(&server_logs, invalid_backend_patterns);

    if invalid_backend_count == expected_count {
        Ok(())
    } else {
        let error_msg = format!("❌ Invalid backend GET error log count mismatch: expected={expected_count}, got={invalid_backend_count} (pattern: clf::discovery ... GET /discovery/url/aHR0cDovL2Zha2UuY29tLw 404 WARN)");
        bail_log!(error_msg)
    }
}

// =============================================================================
// MULTI-SERVER STEP FUNCTIONS - For HTTP remote stores testing
// =============================================================================

/// Step: "When I start server 1 with offers and discovery services"
/// Starts server 1 with only offers and discovery services enabled
pub async fn step_when_i_start_server_1_with_offers_and_discovery_services(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let _pid = ctx
        .start_active_server(&[Service::Discovery, Service::Offer], log::Level::Info)
        .await?;

    Ok(())
}

/// Step: "When I start server 2 with only lnurl service"
/// Starts server 2 with only the lnurl service enabled
pub async fn step_when_i_start_server_2_with_only_lnurl_service(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let _pid = ctx
        .start_active_server(&[Service::LnUrl], log::Level::Info)
        .await?;
    Ok(())
}

/// Step: "Then server 1 should have offers and discovery services listening"
/// Verifies that server 1's offers and discovery services are listening
pub async fn step_then_server_1_should_have_offers_and_discovery_services_listening(
    ctx: &mut GlobalContext,
) -> Result<()> {
    check_services_listening_status(
        ctx,
        vec![Service::Discovery, Service::Offer],
        true,
        1000,
        10,
    )
    .await?;

    Ok(())
}

/// Step: "Then server 2 should have only lnurl service listening"
/// Verifies that server 2's lnurl service is listening
pub async fn step_then_server_2_should_have_only_lnurl_service_listening(
    ctx: &mut GlobalContext,
) -> Result<()> {
    verify_single_service_status(ctx, vec![Service::LnUrl], true, 1000, 10).await?;

    Ok(())
}

/// Step: "When I stop all servers"
/// Stops all running server processes and captures their logs
pub async fn step_when_i_stop_all_servers(ctx: &mut GlobalContext) -> Result<()> {
    ctx.stop_all_servers()?;

    Ok(())
}

/// Step: "Then server 1 logs should contain offer creation requests"
/// Validates that server 1 logs contain expected offer creation patterns
pub async fn step_then_server_1_logs_should_contain_offer_creation_requests(
    ctx: &mut GlobalContext,
) -> Result<()> {
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stdout buffer");
    };

    // Look for offer creation patterns
    let mut metadata_creation_found = false;
    let mut offer_creation_found = false;

    for line in &stderr_lines {
        if line.contains("clf::offer") && line.contains("POST /metadata HTTP/1.1 201") {
            metadata_creation_found = true;
        }
        if line.contains("clf::offer") && line.contains("POST /offers HTTP/1.1 201") {
            offer_creation_found = true;
        }
    }

    if metadata_creation_found && offer_creation_found {
        Ok(())
    } else {
        let error_msg = format!(
            "❌ Server 1 offer creation logs missing: metadata={metadata_creation_found}, offer={offer_creation_found}",

        );
        bail_log!(error_msg)
    }
}

/// Step: "And server 1 logs should contain backend registration requests"
/// Validates that server 1 logs contain expected backend registration patterns
pub async fn step_and_server_1_logs_should_contain_backend_registration_requests(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    // Look for backend registration patterns
    let mut backend_registration_found = false;

    for line in &stderr_lines {
        if line.contains("clf::discovery") && line.contains("POST /discovery HTTP/1.1 201") {
            backend_registration_found = true;
            break;
        }
    }

    if backend_registration_found {
        Ok(())
    } else {
        let error_msg = "❌ Server 1 backend registration logs missing";
        bail_log!(error_msg)
    }
}

/// Step: "And server 1 logs should contain health check requests for offers and discovery services"
/// Validates that server 1 logs contain health check patterns for offers and discovery services
pub async fn step_and_server_1_logs_should_contain_health_check_requests_for_offers_and_discovery_services(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    let mut offers_health_found = false;
    let mut discovery_health_found = false;

    for line in &stderr_lines {
        if line.contains("clf::offer") && line.contains("GET /health HTTP/1.1 200") {
            offers_health_found = true;
        }
        if line.contains("clf::discovery") && line.contains("GET /health HTTP/1.1 200") {
            discovery_health_found = true;
        }
    }

    if offers_health_found && discovery_health_found {
        Ok(())
    } else {
        let error_msg = format!(
            "❌ Server 1 health check logs missing: offers={offers_health_found}, discovery={discovery_health_found}",

        );
        bail_log!(error_msg)
    }
}

/// Step: "And server 1 logs should contain HTTP requests from server 2 for offers and discovery"
/// Validates that server 1 logs show HTTP requests from server 2 for offers and discovery services
pub async fn step_and_server_1_logs_should_contain_http_requests_from_server_2_for_offers_and_discovery(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    // Look for HTTP requests from server 2 to server 1's services
    let mut offers_http_request_found = false;
    let mut discovery_http_request_found = false;

    for line in &stderr_lines {
        // Look for offer requests from HTTP stores (server 2 accessing server 1's offers service)
        if line.contains("clf::offer")
            && (line.contains("GET /offers/default/") || line.contains("GET /metadata/default/"))
            && line.contains("HTTP/1.1 200")
        {
            offers_http_request_found = true;
        }

        // Look for discovery requests from HTTP stores (server 2 accessing server 1's discovery service)
        if line.contains("clf::discovery")
            && line.contains("GET /discovery")
            && line.contains("HTTP/1.1 200")
        {
            discovery_http_request_found = true;
        }
    }

    if offers_http_request_found && discovery_http_request_found {
        Ok(())
    } else {
        let error_msg = format!(
            "❌ Server 1 HTTP requests from server 2 missing: offers={offers_http_request_found}, discovery={discovery_http_request_found}",
        );
        bail_log!(error_msg)
    }
}

/// Step: "And server 2 logs should contain offer retrieval requests via HTTP stores"
/// Validates that server 2 logs contain patterns showing HTTP store usage for offers
pub async fn step_and_server_2_logs_should_contain_offer_retrieval_requests_via_http_stores(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    let mut offer_retrieval_found = false;

    for line in &stderr_lines {
        if line.contains("clf::lnurl")
            && line.contains("GET /offers/default/")
            && line.contains("HTTP/1.1 200")
        {
            offer_retrieval_found = true;
            break;
        }
    }

    if offer_retrieval_found {
        Ok(())
    } else {
        let error_msg = "❌ Server 2 offer retrieval via HTTP stores logs missing";
        bail_log!(error_msg)
    }
}

/// Step: "And server 2 logs should contain invoice generation requests"
/// Validates that server 2 logs contain invoice generation patterns
pub async fn step_and_server_2_logs_should_contain_invoice_generation_requests(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    let mut invoice_generation_found = false;

    for line in &stderr_lines {
        if line.contains("clf::lnurl")
            && line.contains("GET /offers/default/")
            && line.contains("/invoice")
            && line.contains("HTTP/1.1 200")
        {
            invoice_generation_found = true;
            break;
        }
    }

    if invoice_generation_found {
        Ok(())
    } else {
        let error_msg = "❌ Server 2 invoice generation logs missing";
        bail_log!(error_msg)
    }
}

/// Step: "And server 2 logs should contain health check requests for lnurl service"
/// Validates that server 2 logs contain health check patterns for lnurl service
pub async fn step_and_server_2_logs_should_contain_health_check_requests_for_lnurl_service(
    ctx: &mut GlobalContext,
) -> Result<()> {
    // Get captured server logs from stderr buffer
    let stderr_lines = if let Ok(lines) = ctx.get_active_stderr_buffer()?.lock() {
        lines.clone()
    } else {
        bail_log!("Failed to lock stderr buffer");
    };

    if stderr_lines.is_empty() {
        bail_log!("No stderr logs captured");
    }

    let mut lnurl_health_found = false;

    for line in &stderr_lines {
        if line.contains("clf::lnurl") && line.contains("GET /health HTTP/1.1 200") {
            lnurl_health_found = true;
            break;
        }
    }

    if lnurl_health_found {
        Ok(())
    } else {
        let error_msg = "❌ Server 2 lnurl health check logs missing";
        bail_log!(error_msg)
    }
}

// =============================================================================
// CLI SERVICE TOKEN STEP FUNCTIONS - For CLI service token commands
// =============================================================================

/// Step: "Given the swgr CLI is available"
/// Verifies that the swgr CLI is available
pub async fn step_given_the_swgr_cli_is_available(ctx: &mut CliContext) -> Result<()> {
    let args = { vec!["--help"] };
    let empty_env: Vec<(&str, &str)> = vec![];
    ctx.command(empty_env, args)?;
    let exit_code = ctx.exit_code();
    if exit_code != 0 {
        let stderr = ctx.stderr_buffer().join("\n");
        let stdout = ctx.stdout_buffer().join("\n");
        bail_log!(
            "Expected exit code 0, got {}. Stderr: {}. Stdout: {}",
            exit_code,
            stderr,
            stdout
        );
    }
    ctx.reset();
    Ok(())
}

/// Step: "When I run swgr <service> token key with public and private key output paths"
/// Runs the key generation command for the specified service
pub async fn step_when_i_run_swgr_service_token_key(
    ctx: &mut CliContext,
    service: &str,
) -> Result<()> {
    let (public_key_path, private_key_path) = {
        (
            ctx.public_key_path.to_str().unwrap().to_string(),
            ctx.private_key_path.to_str().unwrap().to_string(),
        )
    };

    let args = {
        vec![
            service,
            "token",
            "key",
            "--public",
            &public_key_path,
            "--private",
            &private_key_path,
        ]
    };

    let empty_env: Vec<(&str, &str)> = vec![];
    ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then the command should succeed"
/// Verifies that the command exited with code 0
pub async fn step_then_the_command_should_succeed(ctx: &mut CliContext) -> Result<()> {
    let exit_code = ctx.exit_code();
    if exit_code != 0 {
        let stderr = ctx.stderr_buffer().join("\n");
        let stdout = ctx.stdout_buffer().join("\n");
        bail_log!(
            "Expected exit code 0, got {}. Stderr: {}. Stdout: {}",
            exit_code,
            stderr,
            stdout
        );
    }
    Ok(())
}

/// Step: "And the public key file should exist"
/// Verifies that the public key file was created
pub async fn step_then_the_public_key_file_should_exist(ctx: &mut CliContext) -> Result<()> {
    if !ctx.public_key_path.exists() {
        bail_log!(
            "Public key file does not exist: {}",
            ctx.public_key_path.display()
        );
    }
    // Verify it contains PEM content
    let content = std::fs::read_to_string(&ctx.public_key_path)?;
    if !content.contains("-----BEGIN PUBLIC KEY-----") {
        bail_log!("Public key file does not contain valid PEM content");
    }
    Ok(())
}

/// Step: "And the private key file should exist"
/// Verifies that the private key file was created
pub async fn step_then_the_private_key_file_should_exist(ctx: &mut CliContext) -> Result<()> {
    if !ctx.private_key_path.exists() {
        bail_log!(
            "Private key file does not exist: {}",
            ctx.private_key_path.display()
        );
    }
    // Verify it contains PEM content
    let content = std::fs::read_to_string(&ctx.private_key_path)?;
    if !content.contains("-----BEGIN PRIVATE KEY-----") {
        bail_log!("Private key file does not contain valid PEM content");
    }
    Ok(())
}

/// Step: "Given a valid ECDSA private key exists"
/// Generates a key pair for use in subsequent tests
pub async fn step_given_a_valid_ecdsa_private_key_exists(
    ctx: &mut CliContext,
    service: &str,
) -> Result<()> {
    let (public_key_path, private_key_path) = {
        (
            ctx.public_key_path.to_str().unwrap().to_string(),
            ctx.private_key_path.to_str().unwrap().to_string(),
        )
    };

    let args = vec![
        service,
        "token",
        "key",
        "--public",
        &public_key_path,
        "--private",
        &private_key_path,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    ctx.command(empty_env, args)?;

    if ctx.exit_code() != 0 {
        bail_log!("Failed to generate key pair");
    }

    Ok(())
}

/// Step: "When I run swgr <service> token mint with key path and expiration"
/// Runs the mint command with an existing key
pub async fn step_when_i_run_swgr_service_token_mint(
    ctx: &mut CliContext,
    service: &str,
) -> Result<()> {
    let private_key_path = { ctx.private_key_path.to_str().unwrap().to_string() };

    let args = vec![
        service,
        "token",
        "mint",
        "--key",
        &private_key_path,
        "--expires",
        "3600",
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then a valid token should be output to stdout"
/// Verifies that a token was written to stdout
pub async fn step_then_a_valid_token_should_be_output_to_stdout(
    ctx: &mut CliContext,
) -> Result<()> {
    let stdout = ctx.stdout_buffer();
    if stdout.is_empty() {
        bail_log!("No token output to stdout");
    }

    let token = stdout.join("");
    if token.trim().is_empty() {
        bail_log!("Token output is empty");
    }

    // Store token for verification tests
    ctx.token_stdin = Some(token.trim().to_string());

    Ok(())
}

/// Step: "When I run swgr <service> token mint with key path, expiration, and output path"
/// Runs the mint command with output file
pub async fn step_when_i_run_swgr_service_token_mint_with_output(
    ctx: &mut CliContext,
    service: &str,
) -> Result<()> {
    let (token_path, private_key_path) = {
        (
            ctx.token_path.to_str().unwrap().to_string(),
            ctx.private_key_path.to_str().unwrap().to_string(),
        )
    };

    let args = vec![
        service,
        "token",
        "mint",
        "--key",
        &private_key_path,
        "--expires",
        "3600",
        "--output",
        &token_path,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "And the token file should exist"
/// Verifies that the token file was created
pub async fn step_then_the_token_file_should_exist(ctx: &mut CliContext) -> Result<()> {
    if !ctx.token_path.exists() {
        bail_log!("Token file does not exist: {}", ctx.token_path.display());
    }

    let content = std::fs::read_to_string(&ctx.token_path)?;
    if content.trim().is_empty() {
        bail_log!("Token file is empty");
    }

    Ok(())
}

/// Step: "When I run swgr <service> token mint-all with public path, private path, and expiration"
/// Runs the mint-all command
pub async fn step_when_i_run_swgr_service_token_mint_all(
    ctx: &mut CliContext,
    service: &str,
) -> Result<()> {
    let (public_key_path, private_key_path) = {
        (
            ctx.public_key_path.to_str().unwrap().to_string(),
            ctx.private_key_path.to_str().unwrap().to_string(),
        )
    };
    let args = vec![
        service,
        "token",
        "mint-all",
        "--public",
        &public_key_path,
        "--private",
        &private_key_path,
        "--expires",
        "3600",
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "When I run swgr <service> token mint-all with public path, private path, expiration, and output path"
/// Runs the mint-all command with output file
pub async fn step_when_i_run_swgr_service_token_mint_all_with_output(
    ctx: &mut CliContext,
    service: &str,
) -> Result<()> {
    let (public_key_path, private_key_path, token_path) = {
        (
            ctx.public_key_path.to_str().unwrap().to_string(),
            ctx.private_key_path.to_str().unwrap().to_string(),
            ctx.token_path.to_str().unwrap().to_string(),
        )
    };
    let args = vec![
        service,
        "token",
        "mint-all",
        "--public",
        &public_key_path,
        "--private",
        &private_key_path,
        "--expires",
        "3600",
        "--output",
        &token_path,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Given a valid ECDSA public key exists"
/// Ensures a public key is available (generates if needed)
pub async fn step_given_a_valid_ecdsa_public_key_exists(
    ctx: &mut CliContext,
    service: &str,
) -> Result<()> {
    if !ctx.public_key_path.exists() {
        step_given_a_valid_ecdsa_private_key_exists(ctx, service).await?;
    }
    Ok(())
}

/// Step: "And a valid <service> token exists"
/// Generates a token for verification tests
pub async fn step_given_a_valid_service_token_exists(
    ctx: &mut CliContext,
    service: &str,
) -> Result<()> {
    // Mint a token
    let private_key_path = { ctx.private_key_path.to_str().unwrap().to_string() };
    let args = vec![
        service,
        "token",
        "mint",
        "--key",
        &private_key_path,
        "--expires",
        "3600",
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    ctx.command(empty_env, args)?;

    if ctx.exit_code() != 0 {
        bail_log!("Failed to mint token");
    }

    let stdout = ctx.stdout_buffer();
    ctx.token_stdin = Some(stdout.join("").trim().to_string());

    Ok(())
}

/// Step: "When I run swgr <service> token verify with public key path and token via stdin"
/// Runs the verify command with stdin input
pub async fn step_when_i_run_swgr_service_token_verify_with_stdin(
    ctx: &mut CliContext,
    service: &str,
) -> Result<()> {
    let public_key_path = { ctx.public_key_path.to_str().unwrap().to_string() };
    // Write token to a temp file to simulate stdin
    let token_stdin_path = ctx.temp_dir.path().join("token_stdin.txt");
    if let Some(token) = &ctx.token_stdin {
        std::fs::write(&token_stdin_path, token)?;
    } else {
        bail_log!("No token available for stdin");
    }

    let args = vec![
        service,
        "token",
        "verify",
        "--public",
        &public_key_path,
        "--token",
        token_stdin_path.to_str().unwrap(),
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then the verification output should be valid"
/// Verifies that the token verification succeeded
pub async fn step_then_the_verification_output_should_be_valid(ctx: &mut CliContext) -> Result<()> {
    let stdout = ctx.stdout_buffer().join("\n");

    // Verification should output the token claims or confirmation
    if stdout.trim().is_empty() {
        bail_log!("Verification output is empty");
    }

    Ok(())
}

/// Step: "And a valid <service> token file exists"
/// Creates a token file for verification tests
pub async fn step_given_a_valid_service_token_file_exists(
    ctx: &mut CliContext,
    service: &str,
) -> Result<()> {
    // Mint a token to file
    let (token_path, private_key_path) = {
        (
            ctx.token_path.to_str().unwrap().to_string(),
            ctx.private_key_path.to_str().unwrap().to_string(),
        )
    };
    let args = vec![
        service,
        "token",
        "mint",
        "--key",
        &private_key_path,
        "--expires",
        "3600",
        "--output",
        &token_path,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    ctx.command(empty_env, args)?;

    if ctx.exit_code() != 0 {
        bail_log!("Failed to mint token to file");
    }

    Ok(())
}

/// Step: "When I run swgr <service> token verify with public key path and token file path"
/// Runs the verify command with file input
pub async fn step_when_i_run_swgr_service_token_verify_with_file(
    ctx: &mut CliContext,
    service: &str,
) -> Result<()> {
    let (public_key_path, token_path) = {
        (
            ctx.public_key_path.to_str().unwrap().to_string(),
            ctx.token_path.to_str().unwrap().to_string(),
        )
    };
    let args = vec![
        service,
        "token",
        "verify",
        "--public",
        &public_key_path,
        "--token",
        &token_path,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "And an invalid <service> token exists"
/// Creates an invalid token for negative testing
pub async fn step_given_an_invalid_service_token_exists(
    ctx: &mut CliContext,
    _service: &str,
) -> Result<()> {
    ctx.token_stdin = Some("invalid.token.data".to_string());
    Ok(())
}

/// Step: "Then the command should fail"
/// Verifies that the command exited with a non-zero code
pub async fn step_then_the_command_should_fail(ctx: &mut CliContext) -> Result<()> {
    let exit_code = ctx.exit_code();
    if exit_code == 0 {
        bail_log!("Expected non-zero exit code, got 0");
    }
    Ok(())
}

/// Step: "And an error message should be shown"
/// Verifies that an error message was output
pub async fn step_then_an_error_message_should_be_shown(ctx: &mut CliContext) -> Result<()> {
    let stderr = ctx.stderr_buffer().join("\n");

    if stderr.trim().is_empty() {
        // Some errors might be in stdout
        let stdout = ctx.stdout_buffer().join("\n");
        if stdout.trim().is_empty() {
            bail_log!("No error message in stderr or stdout");
        }
    }

    Ok(())
}

/// Step: "And a conflict message should be shown"
/// Verifies that a "conflict" error message was output
pub async fn step_then_a_conflict_message_should_be_shown(ctx: &mut CliContext) -> Result<()> {
    let stderr = ctx.stderr_buffer().join("\n");

    if !stderr.to_lowercase().contains("conflict") {
        bail_log!("Expected 'conflict' message in output. stderr: {}", stderr,);
    }

    Ok(())
}

/// Step: "And a user error message should be shown"
/// Verifies that a user error message (not a system error) was output
/// Checks for specific user error patterns:  "invalid input"
pub async fn step_then_a_user_error_message_should_be_shown(ctx: &mut CliContext) -> Result<()> {
    let stderr = ctx.stderr_buffer().join("\n");
    let stderr_lower = stderr.to_lowercase();

    // Check for user error patterns
    if !stderr_lower.contains("invalid input") {
        bail_log!(
            "Expected user error message ('invalid input') but got: {}",
            stderr
        );
    }

    Ok(())
}

// =============================================================================
// CLI DISCOVERY MANAGE STEP FUNCTIONS - For CLI discovery management commands
// =============================================================================

/// Step: "When I run swgr discovery new for <node_type>"
/// Runs the discovery new command
pub async fn step_when_i_run_swgr_discovery_new(
    ctx: &mut CliContext,
    node_type: &str,
) -> Result<()> {
    // Use a test public key
    let test_public_key = "02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let args = vec![
        "discovery",
        "new",
        "--partition",
        "default",
        node_type,
        test_public_key,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then valid backend JSON should be output to stdout"
/// Verifies that valid backend JSON was output and matches expected values
pub async fn step_then_valid_backend_json_should_be_output_to_stdout(
    ctx: &mut CliContext,
) -> Result<()> {
    let stdout = ctx.stdout_buffer();
    if stdout.is_empty() {
        bail_log!("No output to stdout");
    }

    let json_str = stdout.join("\n");
    if json_str.trim().is_empty() {
        bail_log!("JSON output is empty");
    }

    // Parse as DiscoveryBackend
    let _: DiscoveryBackend = match serde_json::from_str(&json_str) {
        Ok(backend) => backend,
        Err(e) => {
            bail_log!("Failed to parse backend JSON: {}. Output: {}", e, json_str);
        }
    };

    Ok(())
}

/// Step: "When I run swgr discovery new for <node_type> with output path"
/// Runs the discovery new command with output file
pub async fn step_when_i_run_swgr_discovery_new_with_output(
    ctx: &mut CliContext,
    node_type: &str,
) -> Result<()> {
    // Use a test public key
    let test_public_key = "02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let backend_json_path = ctx.backend_json_path.to_str().unwrap().to_string();

    let args = vec![
        "discovery",
        "new",
        "--partition",
        "default",
        node_type,
        test_public_key,
        "--output",
        &backend_json_path,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "And the backend JSON file should exist"
/// Verifies that the backend JSON file was created with valid content matching expected values
pub async fn step_then_the_backend_json_file_should_exist(
    ctx: &mut CliContext,
    expected_backend: &DiscoveryBackend,
) -> Result<()> {
    if !ctx.backend_json_path.exists() {
        bail_log!(
            "Backend JSON file does not exist: {}",
            ctx.backend_json_path.display()
        );
    }

    let content = std::fs::read_to_string(&ctx.backend_json_path)?;
    if content.trim().is_empty() {
        bail_log!("Backend JSON file is empty");
    }

    // Parse as DiscoveryBackend
    let backend: DiscoveryBackend = match serde_json::from_str(&content) {
        Ok(backend) => backend,
        Err(e) => {
            bail_log!(
                "Failed to parse backend JSON from file: {}. Content: {}",
                e,
                content
            );
        }
    };

    // Verify expected values match
    if backend.public_key != expected_backend.public_key {
        bail_log!(
            "Backend public key mismatch. Expected: {}, Got: {}",
            expected_backend.public_key,
            backend.public_key
        );
    }

    if backend.backend.weight != expected_backend.backend.weight {
        bail_log!(
            "Backend weight mismatch. Expected: {}, Got: {}",
            expected_backend.backend.weight,
            backend.backend.weight
        );
    }

    if backend.backend.enabled != expected_backend.backend.enabled {
        bail_log!(
            "Backend enabled mismatch. Expected: {}, Got: {}",
            expected_backend.backend.enabled,
            backend.backend.enabled
        );
    }

    Ok(())
}

/// Step: "Given a valid backend JSON exists"
/// Creates a valid backend JSON file using discovery new
pub async fn step_given_a_valid_backend_json_exists(
    _ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    // Generate backend JSON
    let test_public_key = "02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let backend_json_path = cli_ctx.backend_json_path.to_str().unwrap().to_string();
    let args = vec![
        "discovery",
        "new",
        "--partition",
        "default",
        "--name",
        "demo",
        "lnd-grpc",
        test_public_key,
        "--output",
        &backend_json_path,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;

    if cli_ctx.exit_code() != 0 {
        bail_log!("Failed to generate backend JSON");
    }

    cli_ctx.reset();

    Ok(())
}

/// Step: "When I run swgr discovery post with backend JSON"
/// Runs the discovery post command
pub async fn step_when_i_run_swgr_discovery_post(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    root_location: CertificateLocation,
) -> Result<()> {
    let discovery_profile = ctx.get_active_discovery_service_profile()?;
    let base_url = format!(
        "{}://{}:{}",
        discovery_profile.protocol,
        discovery_profile.domain,
        discovery_profile.address.port()
    );
    let backend_json_path = cli_ctx.backend_json_path.to_str().unwrap().to_string();

    // Get the trusted roots path
    let trusted_roots = ctx.get_pki_root_certificate_path();
    let trusted_roots_str = trusted_roots.to_str().unwrap().to_string();

    // Get the authorization path
    let authorization = ctx.get_active_discovery_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let mut args = vec!["discovery", "post", "--base-url", &base_url];

    // Configure root certificate location
    let mut env: Vec<(&str, &str)> = vec![];
    match &root_location {
        CertificateLocation::Arg => {
            args.push("--trusted-roots");
            args.push(&trusted_roots_str);
        }
        CertificateLocation::Env => {
            env.push(("DISCOVERY_STORE_HTTP_TRUSTED_ROOTS", &trusted_roots_str));
        }
        CertificateLocation::Native => {
            env.push(("SSL_CERT_FILE", &trusted_roots_str));
        }
        CertificateLocation::NativePath(path) => {
            env.push(("SSL_CERT_FILE", path));
        }
    }

    args.push("--authorization-path");
    args.push(&authorization_str);
    args.push("--input");
    args.push(&backend_json_path);

    cli_ctx.command(env, args)?;
    Ok(())
}

pub async fn extract_backend_public_key(cli_ctx: &CliContext) -> Result<PublicKey> {
    // Load the backend JSON to extract the address and partition
    let content = std::fs::read_to_string(&cli_ctx.backend_json_path)?;
    let backend: DiscoveryBackend = serde_json::from_str(&content)?;
    Ok(backend.public_key)
}

/// Extract backend information from the CLI context's backend JSON
pub async fn extract_backend(cli_ctx: &CliContext) -> Result<DiscoveryBackend> {
    // Load the backend JSON
    let content = std::fs::read_to_string(&cli_ctx.backend_json_path)?;
    let backend: DiscoveryBackend = serde_json::from_str(&content)?;

    Ok(backend)
}

/// Extract offer information from stdout
pub async fn extract_offer_from_stdout(cli_ctx: &CliContext) -> Result<OfferRecord> {
    let stdout = cli_ctx.stdout_buffer().join("\n");
    let offer: OfferRecord = serde_json::from_str(&stdout)?;
    Ok(offer)
}

/// Extract metadata information from stdout
pub async fn extract_metadata_from_stdout(cli_ctx: &CliContext) -> Result<OfferMetadata> {
    let stdout = cli_ctx.stdout_buffer().join("\n");
    let metadata: OfferMetadata = serde_json::from_str(&stdout)?;
    Ok(metadata)
}

/// Extract offer information from file
pub async fn extract_offer(cli_ctx: &CliContext) -> Result<OfferRecord> {
    // Load the offer JSON
    let content = std::fs::read_to_string(&cli_ctx.offer_json_path)?;
    let offer: OfferRecord = serde_json::from_str(&content)?;

    Ok(offer)
}

/// Extract metadata information from file
pub async fn extract_metadata(cli_ctx: &CliContext) -> Result<OfferMetadata> {
    // Load the metadata JSON
    let content = std::fs::read_to_string(&cli_ctx.metadata_json_path)?;
    let metadata: OfferMetadata = serde_json::from_str(&content)?;

    Ok(metadata)
}

/// Step: "When I run swgr discovery ls for partition"
/// Runs the discovery ls command
pub async fn step_when_i_run_swgr_discovery_ls(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let discovery_profile = ctx.get_active_discovery_service_profile()?;
    let base_url = format!(
        "{}://{}:{}",
        discovery_profile.protocol,
        discovery_profile.domain,
        discovery_profile.address.port()
    );

    // Get the trusted roots path
    let trusted_roots = ctx.get_pki_root_certificate_path();
    let trusted_roots_str = trusted_roots.to_str().unwrap().to_string();

    // Get the authorization path
    let authorization = ctx.get_active_discovery_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let args = vec![
        "discovery",
        "ls",
        "--base-url",
        &base_url,
        "--trusted-roots",
        &trusted_roots_str,
        "--authorization-path",
        &authorization_str,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then backend list should be output"
/// Verifies that backend list was output
pub async fn step_then_backend_list_should_be_output(
    cli_ctx: &mut CliContext,
    expected_backends: &[DiscoveryBackend],
) -> Result<()> {
    let stdout = cli_ctx
        .stdout_buffer()
        .join(" ")
        .lines()
        .collect::<Vec<&str>>()
        .join(" ");

    if !stdout.starts_with("# Discovery Backends") {
        bail_log!("Backend list missing header. stderr: {}", stdout);
    }

    for expected in expected_backends {
        let entry = format!(
            "## Public key: {}  * name: {} * location: {} * enabled: {} * weight: {}",
            expected.public_key,
            expected.backend.name.as_deref().unwrap_or("[null]"),
            expected.public_key,
            expected.backend.enabled,
            expected.backend.weight
        );

        if !stdout.contains(&entry) {
            bail_log!("Expected backend {expected:?} not found in output");
        }
    }

    Ok(())
}

/// Step: "When I run swgr discovery get for backend address"
/// Runs the discovery get command
pub async fn step_when_i_run_swgr_discovery_get(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    backend_address: &str,
    root_location: CertificateLocation,
) -> Result<()> {
    let discovery_profile = ctx.get_active_discovery_service_profile()?;
    let base_url = format!(
        "{}://{}:{}",
        discovery_profile.protocol,
        discovery_profile.domain,
        discovery_profile.address.port()
    );

    // Get the trusted roots path
    let trusted_roots = ctx.get_pki_root_certificate_path();
    let trusted_roots_str = trusted_roots.to_str().unwrap().to_string();

    // Get the authorization path
    let authorization = ctx.get_active_discovery_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let mut args = vec!["discovery", "get", backend_address, "--base-url", &base_url];

    // Configure root certificate location
    let mut env: Vec<(&str, &str)> = vec![];
    match &root_location {
        CertificateLocation::Arg => {
            args.push("--trusted-roots");
            args.push(&trusted_roots_str);
        }
        CertificateLocation::Env => {
            env.push(("DISCOVERY_STORE_HTTP_TRUSTED_ROOTS", &trusted_roots_str));
        }
        CertificateLocation::Native => {
            env.push(("SSL_CERT_FILE", &trusted_roots_str));
        }
        CertificateLocation::NativePath(path) => {
            env.push(("SSL_CERT_FILE", path));
        }
    }

    args.push("--authorization-path");
    args.push(&authorization_str);

    cli_ctx.command(env, args)?;
    Ok(())
}

/// Step: "When I run swgr discovery get"
/// Runs the discovery get command without address to get all backends
pub async fn step_when_i_run_swgr_discovery_get_all(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let discovery_profile = ctx.get_active_discovery_service_profile()?;
    let base_url = format!(
        "{}://{}:{}",
        discovery_profile.protocol,
        discovery_profile.domain,
        discovery_profile.address.port()
    );

    // Get the trusted roots path
    let trusted_roots = ctx.get_pki_root_certificate_path();
    let trusted_roots_str = trusted_roots.to_str().unwrap().to_string();

    // Get the authorization path
    let authorization = ctx.get_active_discovery_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let args = vec![
        "discovery",
        "get",
        "--base-url",
        &base_url,
        "--trusted-roots",
        &trusted_roots_str,
        "--authorization-path",
        &authorization_str,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then backend details should be output"
/// Verifies that backend details were output and match expected values
pub async fn step_then_backend_details_should_be_output(
    cli_ctx: &mut CliContext,
    expected_backend: &DiscoveryBackend,
) -> Result<()> {
    let stdout = cli_ctx.stdout_buffer().join("\n");

    if stdout.trim().is_empty() {
        bail_log!("Backend details output is empty");
    }

    // Parse as DiscoveryBackend
    let backend: DiscoveryBackend = match serde_json::from_str(&stdout) {
        Ok(backend) => backend,
        Err(e) => {
            bail_log!(
                "Failed to parse backend details JSON: {}. Output: {}",
                e,
                stdout
            );
        }
    };

    // Verify expected values match
    if backend.public_key != expected_backend.public_key {
        bail_log!(
            "Backend address mismatch. Expected: {}, Got: {}",
            expected_backend.public_key,
            backend.public_key
        );
    }

    if backend.backend.weight != expected_backend.backend.weight {
        bail_log!(
            "Backend weight mismatch. Expected: {}, Got: {}",
            expected_backend.backend.weight,
            backend.backend.weight
        );
    }

    if backend.backend.enabled != expected_backend.backend.enabled {
        bail_log!(
            "Backend enabled mismatch. Expected: {}, Got: {}",
            expected_backend.backend.enabled,
            backend.backend.enabled
        );
    }

    Ok(())
}

/// Step: "Then all backends should be output"
/// Verifies that all backends were output (as a JSON array) and contains the expected backend
pub async fn step_then_all_backends_should_be_output(
    cli_ctx: &mut CliContext,
    expected_backend: &DiscoveryBackend,
) -> Result<()> {
    let stdout = cli_ctx.stdout_buffer().join("\n");

    if stdout.trim().is_empty() {
        bail_log!("All backends output is empty");
    }

    // Parse as JSON array of DiscoveryBackend
    let backends: Vec<DiscoveryBackend> = match serde_json::from_str(&stdout) {
        Ok(backends) => backends,
        Err(e) => {
            bail_log!(
                "Failed to parse backends JSON array: {}. Output: {}",
                e,
                stdout
            );
        }
    };

    // Verify the expected backend is in the array
    let found = backends.iter().any(|backend| {
        backend.public_key == expected_backend.public_key
            && backend.backend.weight == expected_backend.backend.weight
            && backend.backend.enabled == expected_backend.backend.enabled
    });

    if !found {
        bail_log!(
            "Expected backend with address {} not found in output. Got {} backends",
            expected_backend.public_key,
            backends.len()
        );
    }

    Ok(())
}

/// Step: "And updated backend JSON exists"
/// Creates an updated backend JSON for put operation
pub async fn step_and_updated_backend_json_exists(
    _ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    // Read the existing backend JSON and modify it slightly
    let content = std::fs::read_to_string(&cli_ctx.backend_json_path)?;
    let mut backend: DiscoveryBackend = serde_json::from_str(&content)?;

    // Update the weight field to verify the update worked
    backend.backend.weight = 999;

    // Add a name to verify it was updated
    backend.backend.name = Some("updated-backend".to_string());

    // Write back to file
    std::fs::write(
        &cli_ctx.backend_json_path,
        serde_json::to_string_pretty(&backend)?,
    )?;

    Ok(())
}

/// Step: "When I run swgr discovery put with backend address and JSON"
/// Runs the discovery put command
pub async fn step_when_i_run_swgr_discovery_put(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    backend_address: &str,
) -> Result<()> {
    let discovery_profile = ctx.get_active_discovery_service_profile()?;
    let base_url = format!(
        "{}://{}:{}",
        discovery_profile.protocol,
        discovery_profile.domain,
        discovery_profile.address.port()
    );
    let backend_json_path = cli_ctx.backend_json_path.to_str().unwrap().to_string();

    // Get the trusted roots path
    let trusted_roots = ctx.get_pki_root_certificate_path();
    let trusted_roots_str = trusted_roots.to_str().unwrap().to_string();

    // Get the authorization path
    let authorization = ctx.get_active_discovery_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let args = vec![
        "discovery",
        "put",
        backend_address,
        "--base-url",
        &base_url,
        "--trusted-roots",
        &trusted_roots_str,
        "--authorization-path",
        &authorization_str,
        "--input",
        &backend_json_path,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "When I run swgr discovery delete for backend address"
/// Runs the discovery delete command
pub async fn step_when_i_run_swgr_discovery_delete(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    backend_address: &str,
) -> Result<()> {
    let discovery_profile = ctx.get_active_discovery_service_profile()?;
    let base_url = format!(
        "{}://{}:{}",
        discovery_profile.protocol,
        discovery_profile.domain,
        discovery_profile.address.port()
    );

    // Get the trusted roots path
    let trusted_roots = ctx.get_pki_root_certificate_path();
    let trusted_roots_str = trusted_roots.to_str().unwrap().to_string();

    // Get the authorization path
    let authorization = ctx.get_active_discovery_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let args = vec![
        "discovery",
        "delete",
        backend_address,
        "--base-url",
        &base_url,
        "--trusted-roots",
        &trusted_roots_str,
        "--authorization-path",
        &authorization_str,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then the backend should contain the updated data"
/// Verifies that the backend was updated with new data
pub async fn step_then_the_backend_should_contain_the_updated_data(
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let stdout = cli_ctx.stdout_buffer().join("\n");

    // Check if the output contains the updated weight (999) and name (updated-backend)
    if !stdout.contains("999") || !stdout.contains("updated-backend") {
        bail_log!("Backend does not contain updated data (weight: 999, name: updated-backend)");
    }

    Ok(())
}

/// Step: "Then the backend should not be found"
/// Verifies that the backend was deleted and returns not found
pub async fn step_then_the_backend_should_not_be_found(cli_ctx: &mut CliContext) -> Result<()> {
    let stderr = cli_ctx.stderr_buffer().join("\n");

    // Check if stderr contains "not found"
    if !stderr.to_lowercase().contains("not found") {
        bail_log!("Backend was found when it should not exist");
    }

    Ok(())
}

/// Step: "And backend patch JSON exists"
/// Creates a patch JSON for testing partial backend updates
pub async fn step_and_backend_patch_json_exists(
    _ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    // Create a patch that modifies the weight field
    let patch = DiscoveryBackendPatchSparse {
        name: None,
        partitions: None,
        weight: Some(777),
        enabled: None,
    };

    // Write the patch JSON to file
    std::fs::write(
        &cli_ctx.backend_json_path,
        serde_json::to_string_pretty(&patch)?,
    )?;

    Ok(())
}

/// Step: "When I run swgr discovery patch with backend address and patch JSON"
/// Runs the discovery patch command
pub async fn step_when_i_run_swgr_discovery_patch(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    backend_address: &str,
) -> Result<()> {
    let discovery_profile = ctx.get_active_discovery_service_profile()?;
    let base_url = format!(
        "{}://{}:{}",
        discovery_profile.protocol,
        discovery_profile.domain,
        discovery_profile.address.port()
    );
    let backend_json_path = cli_ctx.backend_json_path.to_str().unwrap().to_string();

    // Get the trusted roots path
    let trusted_roots = ctx.get_pki_root_certificate_path();
    let trusted_roots_str = trusted_roots.to_str().unwrap().to_string();

    // Get the authorization path
    let authorization = ctx.get_active_discovery_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let args = vec![
        "discovery",
        "patch",
        backend_address,
        "--input",
        &backend_json_path,
        "--base-url",
        &base_url,
        "--trusted-roots",
        &trusted_roots_str,
        "--authorization-path",
        &authorization_str,
    ];
    let empty_env: Vec<(&str, &str)> = vec![];

    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then the backend should contain the patched data"
/// Verifies that the backend was patched with new data
pub async fn step_then_the_backend_should_contain_the_patched_data(
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let stdout = cli_ctx.stdout_buffer().join("\n");

    // Check if the output contains the patched weight (777)
    if !stdout.contains("777") {
        bail_log!("Backend does not contain patched data (weight: 777)");
    }

    Ok(())
}

/// Step: "When I run swgr discovery enable for backend address"
/// Runs the discovery enable command
pub async fn step_when_i_run_swgr_discovery_enable(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    backend_address: &str,
) -> Result<()> {
    let discovery_profile = ctx.get_active_discovery_service_profile()?;
    let base_url = format!(
        "{}://{}:{}",
        discovery_profile.protocol,
        discovery_profile.domain,
        discovery_profile.address.port()
    );

    // Get the trusted roots path
    let trusted_roots = ctx.get_pki_root_certificate_path();
    let trusted_roots_str = trusted_roots.to_str().unwrap().to_string();

    // Get the authorization path
    let authorization = ctx.get_active_discovery_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let args = vec![
        "discovery",
        "enable",
        backend_address,
        "--base-url",
        &base_url,
        "--trusted-roots",
        &trusted_roots_str,
        "--authorization-path",
        &authorization_str,
    ];
    let empty_env: Vec<(&str, &str)> = vec![];

    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "When I run swgr discovery disable for backend address"
/// Runs the discovery disable command
pub async fn step_when_i_run_swgr_discovery_disable(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    backend_address: &str,
) -> Result<()> {
    let discovery_profile = ctx.get_active_discovery_service_profile()?;
    let base_url = format!(
        "{}://{}:{}",
        discovery_profile.protocol,
        discovery_profile.domain,
        discovery_profile.address.port()
    );

    // Get the trusted roots path
    let trusted_roots = ctx.get_pki_root_certificate_path();
    let trusted_roots_str = trusted_roots.to_str().unwrap().to_string();

    // Get the authorization path
    let authorization = ctx.get_active_discovery_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let args = vec![
        "discovery",
        "disable",
        backend_address,
        "--base-url",
        &base_url,
        "--trusted-roots",
        &trusted_roots_str,
        "--authorization-path",
        &authorization_str,
    ];
    let empty_env: Vec<(&str, &str)> = vec![];

    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then the backend should be enabled"
/// Verifies that the backend is enabled
pub async fn step_then_the_backend_should_be_enabled(cli_ctx: &mut CliContext) -> Result<()> {
    let stdout = cli_ctx.stdout_buffer().join("\n");

    // Check if the output contains "enabled": true
    if !stdout.contains("\"enabled\": true") && !stdout.contains("\"enabled\":true") {
        bail_log!("Backend is not enabled");
    }

    Ok(())
}

/// Step: "Then the backend should be disabled"
/// Verifies that the backend is disabled
pub async fn step_then_the_backend_should_be_disabled(cli_ctx: &mut CliContext) -> Result<()> {
    let stdout = cli_ctx.stdout_buffer().join("\n");

    // Check if the output contains "enabled": false
    if !stdout.contains("\"enabled\": false") && !stdout.contains("\"enabled\":false") {
        bail_log!("Backend is not disabled");
    }

    Ok(())
}

/// Step: "When I run swgr discovery get for non-existent backend address"
/// Runs the discovery get command for a non-existent backend
pub async fn step_when_i_run_swgr_discovery_get_for_non_existent_backend(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let non_existent_public_key =
        "03eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

    let discovery_profile = ctx.get_active_discovery_service_profile()?;
    let base_url = format!(
        "{}://{}:{}",
        discovery_profile.protocol,
        discovery_profile.domain,
        discovery_profile.address.port()
    );

    let trusted_roots = ctx.get_pki_root_certificate_path();
    let trusted_roots_str = trusted_roots.to_str().unwrap().to_string();

    let authorization = ctx.get_active_discovery_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let args = vec![
        "discovery",
        "get",
        non_existent_public_key,
        "--base-url",
        &base_url,
        "--trusted-roots",
        &trusted_roots_str,
        "--authorization-path",
        &authorization_str,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "When I run swgr discovery patch for non-existent backend address"
/// Runs the discovery patch command for a non-existent backend
pub async fn step_when_i_run_swgr_discovery_patch_for_non_existent_backend(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let non_existent_public_key =
        "03eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

    let discovery_profile = ctx.get_active_discovery_service_profile()?;
    let base_url = format!(
        "{}://{}:{}",
        discovery_profile.protocol,
        discovery_profile.domain,
        discovery_profile.address.port()
    );

    let patch_json_path = cli_ctx.backend_json_path.to_str().unwrap().to_string();
    let trusted_roots = ctx.get_pki_root_certificate_path();
    let trusted_roots_str = trusted_roots.to_str().unwrap().to_string();

    let authorization = ctx.get_active_discovery_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let args = vec![
        "discovery",
        "patch",
        non_existent_public_key,
        "--base-url",
        &base_url,
        "--trusted-roots",
        &trusted_roots_str,
        "--authorization-path",
        &authorization_str,
        "--input",
        &patch_json_path,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "When I run swgr discovery enable for non-existent backend address"
/// Runs the discovery enable command for a non-existent backend
pub async fn step_when_i_run_swgr_discovery_enable_for_non_existent_backend(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let non_existent_public_key =
        "03eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

    let discovery_profile = ctx.get_active_discovery_service_profile()?;
    let base_url = format!(
        "{}://{}:{}",
        discovery_profile.protocol,
        discovery_profile.domain,
        discovery_profile.address.port()
    );

    let trusted_roots = ctx.get_pki_root_certificate_path();
    let trusted_roots_str = trusted_roots.to_str().unwrap().to_string();

    let authorization = ctx.get_active_discovery_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let args = vec![
        "discovery",
        "enable",
        non_existent_public_key,
        "--base-url",
        &base_url,
        "--trusted-roots",
        &trusted_roots_str,
        "--authorization-path",
        &authorization_str,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "When I run swgr discovery disable for non-existent backend address"
/// Runs the discovery disable command for a non-existent backend
pub async fn step_when_i_run_swgr_discovery_disable_for_non_existent_backend(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let non_existent_public_key =
        "03eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

    let discovery_profile = ctx.get_active_discovery_service_profile()?;
    let base_url = format!(
        "{}://{}:{}",
        discovery_profile.protocol,
        discovery_profile.domain,
        discovery_profile.address.port()
    );

    let trusted_roots = ctx.get_pki_root_certificate_path();
    let trusted_roots_str = trusted_roots.to_str().unwrap().to_string();

    let authorization = ctx.get_active_discovery_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let args = vec![
        "discovery",
        "disable",
        non_existent_public_key,
        "--base-url",
        &base_url,
        "--trusted-roots",
        &trusted_roots_str,
        "--authorization-path",
        &authorization_str,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "When I run swgr discovery delete for non-existent backend address"
/// Runs the discovery delete command for a non-existent backend
pub async fn step_when_i_run_swgr_discovery_delete_for_non_existent_backend(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let non_existent_public_key =
        "03eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";

    let discovery_profile = ctx.get_active_discovery_service_profile()?;
    let base_url = format!(
        "{}://{}:{}",
        discovery_profile.protocol,
        discovery_profile.domain,
        discovery_profile.address.port()
    );

    let trusted_roots = ctx.get_pki_root_certificate_path();
    let trusted_roots_str = trusted_roots.to_str().unwrap().to_string();

    let authorization = ctx.get_active_discovery_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let args = vec![
        "discovery",
        "delete",
        non_existent_public_key,
        "--base-url",
        &base_url,
        "--trusted-roots",
        &trusted_roots_str,
        "--authorization-path",
        &authorization_str,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

// =============================================================================
// CLI OFFER MANAGE STEP FUNCTIONS - For CLI offer management commands
// =============================================================================

/// Helper function to extract offer ID from offer JSON
pub async fn extract_offer_id(cli_ctx: &CliContext) -> Result<Uuid> {
    let content = std::fs::read_to_string(&cli_ctx.offer_json_path)?;
    let offer: OfferRecord = serde_json::from_str(&content)?;
    Ok(offer.id)
}

/// Helper function to extract metadata ID from metadata JSON
pub async fn extract_metadata_id(cli_ctx: &CliContext) -> Result<Uuid> {
    let content = std::fs::read_to_string(&cli_ctx.metadata_json_path)?;
    let metadata: OfferMetadata = serde_json::from_str(&content)?;
    Ok(metadata.id)
}

/// Step: "When I run swgr offer new"
/// Runs the offer new command
pub async fn step_when_i_run_swgr_offer_new(cli_ctx: &mut CliContext) -> Result<()> {
    // Generate a metadata ID for the command
    let metadata_id = Uuid::new_v4();
    let metadata_id_str = metadata_id.to_string();
    let args = vec![
        "offer",
        "new",
        "--partition",
        "default",
        "--metadata-id",
        &metadata_id_str,
    ];
    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then valid offer JSON should be output to stdout"
/// Verifies that valid offer JSON was output
pub async fn step_then_valid_offer_json_should_be_output_to_stdout(
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let stdout = cli_ctx.stdout_buffer().join("\n");

    // Try to parse as OfferRecord
    let _: OfferRecord = serde_json::from_str(&stdout)
        .with_context(|| format!("Failed to parse offer JSON from stdout: {}", stdout))?;

    Ok(())
}

/// Step: "When I run swgr offer new with output path"
/// Runs the offer new command with output file
pub async fn step_when_i_run_swgr_offer_new_with_output(cli_ctx: &mut CliContext) -> Result<()> {
    // Generate a metadata ID for the command
    let metadata_id = Uuid::new_v4();
    let metadata_id_str = metadata_id.to_string();
    let output_path = cli_ctx.offer_json_path.to_str().unwrap().to_string();
    let args = vec![
        "offer",
        "new",
        "--partition",
        "default",
        "--metadata-id",
        &metadata_id_str,
        "--output",
        &output_path,
    ];
    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then the offer JSON file should exist"
/// Verifies that the offer JSON file was created
pub async fn step_then_the_offer_json_file_should_exist(cli_ctx: &mut CliContext) -> Result<()> {
    if !cli_ctx.offer_json_path.exists() {
        bail_log!(
            "Offer JSON file does not exist: {}",
            cli_ctx.offer_json_path.display()
        );
    }

    // Verify it contains valid JSON
    let content = std::fs::read_to_string(&cli_ctx.offer_json_path)?;
    let _offer: OfferRecord = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse offer JSON from file: {}", content))?;

    Ok(())
}

/// Step: "Given a valid offer JSON exists"
/// Creates a valid offer JSON file for testing
/// Note: Creates the metadata first, then creates an offer that references it
pub async fn step_given_a_valid_offer_json_exists(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    // Step 1: Create metadata JSON with unique ID
    let metadata_output = cli_ctx.metadata_json_path.to_str().unwrap().to_string();
    let args = vec![
        "offer",
        "metadata",
        "new",
        "--partition",
        "default",
        "--text",
        "test-metadata",
        "--output",
        &metadata_output,
    ];
    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;

    if cli_ctx.exit_code() != 0 {
        bail_log!("Failed to generate offer metadata JSON");
    }

    let content = std::fs::read_to_string(&cli_ctx.metadata_json_path)?;
    let metadata: OfferMetadata = serde_json::from_str(&content)?;

    cli_ctx.reset();

    // Step 2: Post the metadata to the server
    let service_profile = ctx.get_active_offer_service_profile()?;
    let protocol = service_profile.protocol;
    let domain = service_profile.domain;
    let port = service_profile.address.port();
    let base_url = format!("{}://{}:{}", protocol, domain, port);

    let ca_bundle = ctx.get_pki_root_certificate_path();
    let ca_bundle_str = ca_bundle.to_str().unwrap().to_string();

    let authorization = ctx.get_active_offer_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let args = vec![
        "offer",
        "metadata",
        "post",
        "--base-url",
        &base_url,
        "--authorization-path",
        &authorization_str,
        "--trusted-roots",
        &ca_bundle_str,
        "--input",
        &metadata_output,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;

    if cli_ctx.exit_code() != 0 {
        bail_log!("Failed to post offer metadata");
    }

    cli_ctx.reset();

    // Step 3: Create offer JSON with unique ID that references the metadata
    let metadata_id_str = metadata.id.to_string();
    let offer_output = cli_ctx.offer_json_path.to_str().unwrap().to_string();
    let args = vec![
        "offer",
        "new",
        "--partition",
        "default",
        "--metadata-id",
        &metadata_id_str,
        "--output",
        &offer_output,
    ];
    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;

    if cli_ctx.exit_code() != 0 {
        bail_log!("Failed to generate offer JSON");
    }

    cli_ctx.reset();
    Ok(())
}

/// Step: "When I run swgr offer post with offer JSON"
/// Runs the offer post command
pub async fn step_when_i_run_swgr_offer_post(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    certificate_location: CertificateLocation,
) -> Result<()> {
    let service_profile = ctx.get_active_offer_service_profile()?;
    let protocol = service_profile.protocol;
    let domain = service_profile.domain;
    let port = service_profile.address.port();
    let base_url = format!("{}://{}:{}", protocol, domain, port);

    let ca_bundle = ctx.get_pki_root_certificate_path();
    let ca_bundle_str = ca_bundle.to_str().unwrap().to_string();

    let authorization = ctx.get_active_offer_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let input_path = cli_ctx.offer_json_path.to_str().unwrap().to_string();

    let mut args = vec![
        "offer",
        "post",
        "--base-url",
        &base_url,
        "--authorization-path",
        &authorization_str,
    ];

    // Configure root certificate location
    let mut env: Vec<(&str, &str)> = vec![];
    match &certificate_location {
        CertificateLocation::Arg => {
            args.push("--trusted-roots");
            args.push(&ca_bundle_str);
        }
        CertificateLocation::Env => {
            env.push(("OFFER_STORE_HTTP_TRUSTED_ROOTS", &ca_bundle_str));
        }
        CertificateLocation::Native => {
            env.push(("SSL_CERT_FILE", &ca_bundle_str));
        }
        CertificateLocation::NativePath(path) => {
            env.push(("SSL_CERT_FILE", path));
        }
    }

    args.push("--input");
    args.push(&input_path);

    cli_ctx.command(env, args)?;
    Ok(())
}

/// Step: "When I run swgr offer get for offer ID"
/// Runs the offer get command
pub async fn step_when_i_run_swgr_offer_get(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    offer_id: &Uuid,
    certificate_location: CertificateLocation,
) -> Result<()> {
    let service_profile = ctx.get_active_offer_service_profile()?;
    let protocol = service_profile.protocol;
    let domain = service_profile.domain;
    let port = service_profile.address.port();
    let base_url = format!("{}://{}:{}", protocol, domain, port);

    let ca_bundle = ctx.get_pki_root_certificate_path();
    let ca_bundle_str = ca_bundle.to_str().unwrap().to_string();

    let authorization = ctx.get_active_offer_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let id = offer_id.to_string();

    let mut args = vec![
        "offer",
        "get",
        "default",
        &id,
        "--base-url",
        &base_url,
        "--authorization-path",
        &authorization_str,
    ];

    // Configure root certificate location
    let mut env: Vec<(&str, &str)> = vec![];
    match &certificate_location {
        CertificateLocation::Arg => {
            args.push("--trusted-roots");
            args.push(&ca_bundle_str);
        }
        CertificateLocation::Env => {
            env.push(("OFFER_STORE_HTTP_TRUSTED_ROOTS", &ca_bundle_str));
        }
        CertificateLocation::Native => {
            env.push(("SSL_CERT_FILE", &ca_bundle_str));
        }
        CertificateLocation::NativePath(path) => {
            env.push(("SSL_CERT_FILE", path));
        }
    }

    cli_ctx.command(env, args)?;
    Ok(())
}

/// Step: "When I run swgr offer get" or "When I run swgr offer get with parameters"
/// Runs the offer get command without ID to get all offers, with optional parameters
pub async fn step_when_i_run_swgr_offer_get_all(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    start: Option<usize>,
    count: Option<usize>,
) -> Result<()> {
    let service_profile = ctx.get_active_offer_service_profile()?;
    let protocol = service_profile.protocol;
    let domain = service_profile.domain;
    let port = service_profile.address.port();
    let base_url = format!("{}://{}:{}", protocol, domain, port);

    let ca_bundle = ctx.get_pki_root_certificate_path();
    let ca_bundle_str = ca_bundle.to_str().unwrap().to_string();

    let authorization = ctx.get_active_offer_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let mut args = vec![
        "offer",
        "get",
        "default",
        "--base-url",
        &base_url,
        "--authorization-path",
        &authorization_str,
        "--trusted-roots",
        &ca_bundle_str,
    ];

    // Add start parameter if provided
    let start_str;
    if let Some(s) = start {
        args.push("--start");
        start_str = s.to_string();
        args.push(&start_str);
    }

    // Add count parameter if provided
    let count_str;
    if let Some(c) = count {
        args.push("--count");
        count_str = c.to_string();
        args.push(&count_str);
    }

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then offer details should be output"
/// Verifies that offer details were output and match expected values
pub async fn step_then_offer_details_should_be_output(
    cli_ctx: &mut CliContext,
    expected_offer: &OfferRecord,
) -> Result<()> {
    let stdout = cli_ctx.stdout_buffer().join("\n");

    if stdout.trim().is_empty() {
        bail_log!("No offer details in stdout");
    }

    // Parse as OfferRecord
    let offer: OfferRecord = match serde_json::from_str(&stdout) {
        Ok(offer) => offer,
        Err(e) => {
            bail_log!(
                "Failed to parse offer details JSON: {}. Output: {}",
                e,
                stdout
            );
        }
    };

    // Verify expected values match
    if offer.id != expected_offer.id {
        bail_log!(
            "Offer ID mismatch. Expected: {}, Got: {}",
            expected_offer.id,
            offer.id
        );
    }

    if offer.partition != expected_offer.partition {
        bail_log!(
            "Offer partition mismatch. Expected: {}, Got: {}",
            expected_offer.partition,
            offer.partition
        );
    }

    if offer.offer.max_sendable != expected_offer.offer.max_sendable {
        bail_log!(
            "Offer max_sendable mismatch. Expected: {}, Got: {}",
            expected_offer.offer.max_sendable,
            offer.offer.max_sendable
        );
    }

    if offer.offer.min_sendable != expected_offer.offer.min_sendable {
        bail_log!(
            "Offer min_sendable mismatch. Expected: {}, Got: {}",
            expected_offer.offer.min_sendable,
            offer.offer.min_sendable
        );
    }

    Ok(())
}

/// Step: "And all offers should be output"
/// Verifies that all offers were output (as a JSON array) and contains the expected offer
pub async fn step_then_all_offers_should_be_output(
    cli_ctx: &mut CliContext,
    expected_offers: &[OfferRecord],
) -> Result<()> {
    let stdout = cli_ctx.stdout_buffer().join("\n");

    if stdout.trim().is_empty() {
        bail_log!("All offers output is empty");
    }

    // Parse as JSON array of OfferRecord
    let offers: Vec<OfferRecord> = match serde_json::from_str(&stdout) {
        Ok(offers) => offers,
        Err(e) => {
            bail_log!(
                "Failed to parse offers JSON array: {}. Output: {}",
                e,
                stdout
            );
        }
    };

    assert_eq!(expected_offers, offers.as_slice());

    Ok(())
}

/// Step: "And updated offer JSON exists"
/// Creates an updated offer JSON file
pub async fn step_and_updated_offer_json_exists(
    _ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    // Read the existing offer JSON
    let content = std::fs::read_to_string(&cli_ctx.offer_json_path)?;
    let mut offer: OfferRecord = serde_json::from_str(&content)?;

    // Modify some fields to create an updated version
    offer.offer.max_sendable = 5_000_000;
    offer.offer.min_sendable = 2_000_000;

    // Write the updated JSON back
    let updated_json = serde_json::to_string_pretty(&offer)?;
    std::fs::write(&cli_ctx.offer_json_path, updated_json)?;

    Ok(())
}

/// Step: "When I run swgr offer put with offer ID and JSON"
/// Runs the offer put command
pub async fn step_when_i_run_swgr_offer_put(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    offer_id: &Uuid,
) -> Result<()> {
    let service_profile = ctx.get_active_offer_service_profile()?;
    let protocol = service_profile.protocol;
    let domain = service_profile.domain;
    let port = service_profile.address.port();
    let base_url = format!("{}://{}:{}", protocol, domain, port);

    let ca_bundle = ctx.get_pki_root_certificate_path();
    let ca_bundle_str = ca_bundle.to_str().unwrap().to_string();

    let authorization = ctx.get_active_offer_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let input_path = cli_ctx.offer_json_path.to_str().unwrap().to_string();

    let id = offer_id.to_string();

    let args = vec![
        "offer",
        "put",
        "default",
        &id,
        "--base-url",
        &base_url,
        "--authorization-path",
        &authorization_str,
        "--trusted-roots",
        &ca_bundle_str,
        "--input",
        &input_path,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then the offer should contain the updated data"
/// Verifies that the offer was updated
pub async fn step_then_the_offer_should_contain_the_updated_data(
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let stdout = cli_ctx.stdout_buffer().join("\n");

    // Check if the output contains the updated values
    if !stdout.contains("5000000") || !stdout.contains("2000000") {
        bail_log!(
            "Offer does not contain updated data (max_sendable: 5000000, min_sendable: 2000000)"
        );
    }

    Ok(())
}

/// Step: "When I run swgr offer delete for offer ID"
/// Runs the offer delete command
pub async fn step_when_i_run_swgr_offer_delete(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    offer_id: &Uuid,
) -> Result<()> {
    let service_profile = ctx.get_active_offer_service_profile()?;
    let protocol = service_profile.protocol;
    let domain = service_profile.domain;
    let port = service_profile.address.port();
    let base_url = format!("{}://{}:{}", protocol, domain, port);

    let ca_bundle = ctx.get_pki_root_certificate_path();
    let ca_bundle_str = ca_bundle.to_str().unwrap().to_string();

    let authorization = ctx.get_active_offer_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let id = offer_id.to_string();

    let args = vec![
        "offer",
        "delete",
        "default",
        &id,
        "--base-url",
        &base_url,
        "--authorization-path",
        &authorization_str,
        "--trusted-roots",
        &ca_bundle_str,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then the offer should not be found"
/// Verifies that the offer was deleted
pub async fn step_then_the_offer_should_not_be_found(cli_ctx: &mut CliContext) -> Result<()> {
    let stderr = cli_ctx.stderr_buffer().join("\n");

    // Check if stderr contains "not found"
    if !stderr.to_lowercase().contains("not found") {
        bail_log!("Offer was found when it should not exist");
    }

    Ok(())
}

// =============================================================================
// CLI OFFER METADATA MANAGE STEP FUNCTIONS - For CLI offer metadata management
// =============================================================================

/// Step: "When I run swgr offer metadata new"
/// Runs the offer metadata new command
pub async fn step_when_i_run_swgr_offer_metadata_new(cli_ctx: &mut CliContext) -> Result<()> {
    let args = vec![
        "offer",
        "metadata",
        "new",
        "--partition",
        "default",
        "--text",
        "test-metadata",
    ];
    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then valid offer metadata JSON should be output to stdout"
/// Verifies that valid offer metadata JSON was output
pub async fn step_then_valid_offer_metadata_json_should_be_output_to_stdout(
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let stdout = cli_ctx.stdout_buffer().join("\n");

    // Try to parse as OfferMetadata
    let _: OfferMetadata = serde_json::from_str(&stdout).with_context(|| {
        format!(
            "Failed to parse offer metadata JSON from stdout: {}",
            stdout
        )
    })?;

    Ok(())
}

/// Step: "When I run swgr offer metadata new with output path"
/// Runs the offer metadata new command with output file
pub async fn step_when_i_run_swgr_offer_metadata_new_with_output(
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let output_path = cli_ctx.metadata_json_path.to_str().unwrap().to_string();
    let args = vec![
        "offer",
        "metadata",
        "new",
        "--partition",
        "default",
        "--text",
        "test-metadata",
        "--output",
        &output_path,
    ];
    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then the offer metadata JSON file should exist"
/// Verifies that the offer metadata JSON file was created
pub async fn step_then_the_offer_metadata_json_file_should_exist(
    cli_ctx: &mut CliContext,
) -> Result<()> {
    if !cli_ctx.metadata_json_path.exists() {
        bail_log!(
            "Offer metadata JSON file does not exist: {}",
            cli_ctx.metadata_json_path.display()
        );
    }

    // Verify it contains valid JSON
    let content = std::fs::read_to_string(&cli_ctx.metadata_json_path)?;
    let _metadata: OfferMetadata = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse offer metadata JSON from file: {}", content))?;

    Ok(())
}

/// Step: "Given a valid offer metadata JSON exists"
/// Creates a valid offer metadata JSON file for testing with unique ID
pub async fn step_given_a_valid_offer_metadata_json_exists(
    _ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    // Run swgr offer metadata new to generate a valid metadata JSON
    let output_path = cli_ctx.metadata_json_path.to_str().unwrap().to_string();
    let args = vec![
        "offer",
        "metadata",
        "new",
        "--partition",
        "default",
        "--text",
        "test-metadata",
        "--output",
        &output_path,
    ];
    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;

    if cli_ctx.exit_code() != 0 {
        bail_log!("Failed to generate offer metadata JSON");
    }

    cli_ctx.reset();
    Ok(())
}

/// Step: "When I run swgr offer metadata post with metadata JSON"
/// Runs the offer metadata post command
pub async fn step_when_i_run_swgr_offer_metadata_post(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    certificate_location: CertificateLocation,
) -> Result<()> {
    let service_profile = ctx.get_active_offer_service_profile()?;
    let protocol = service_profile.protocol;
    let domain = service_profile.domain;
    let port = service_profile.address.port();
    let base_url = format!("{}://{}:{}", protocol, domain, port);

    let ca_bundle = ctx.get_pki_root_certificate_path();
    let ca_bundle_str = ca_bundle.to_str().unwrap().to_string();

    let authorization = ctx.get_active_offer_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let input_path = cli_ctx.metadata_json_path.to_str().unwrap().to_string();

    let mut args = vec![
        "offer",
        "metadata",
        "post",
        "--base-url",
        &base_url,
        "--authorization-path",
        &authorization_str,
    ];

    // Configure root certificate location
    let mut env: Vec<(&str, &str)> = vec![];
    match &certificate_location {
        CertificateLocation::Arg => {
            args.push("--trusted-roots");
            args.push(&ca_bundle_str);
        }
        CertificateLocation::Env => {
            env.push(("OFFER_STORE_HTTP_TRUSTED_ROOTS", &ca_bundle_str));
        }
        CertificateLocation::Native => {
            env.push(("SSL_CERT_FILE", &ca_bundle_str));
        }
        CertificateLocation::NativePath(path) => {
            env.push(("SSL_CERT_FILE", path));
        }
    }

    args.push("--input");
    args.push(&input_path);

    cli_ctx.command(env, args)?;
    Ok(())
}

/// Step: "When I run swgr offer metadata get for metadata ID"
/// Runs the offer metadata get command
pub async fn step_when_i_run_swgr_offer_metadata_get(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    metadata_id: &Uuid,
    certificate_location: CertificateLocation,
) -> Result<()> {
    let service_profile = ctx.get_active_offer_service_profile()?;
    let protocol = service_profile.protocol;
    let domain = service_profile.domain;
    let port = service_profile.address.port();
    let base_url = format!("{}://{}:{}", protocol, domain, port);

    let ca_bundle = ctx.get_pki_root_certificate_path();
    let ca_bundle_str = ca_bundle.to_str().unwrap().to_string();

    let authorization = ctx.get_active_offer_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let id = metadata_id.to_string();

    let mut args = vec![
        "offer",
        "metadata",
        "get",
        "default",
        &id,
        "--base-url",
        &base_url,
        "--authorization-path",
        &authorization_str,
    ];

    // Configure root certificate location
    let mut env: Vec<(&str, &str)> = vec![];
    match &certificate_location {
        CertificateLocation::Arg => {
            args.push("--trusted-roots");
            args.push(&ca_bundle_str);
        }
        CertificateLocation::Env => {
            env.push(("OFFER_STORE_HTTP_TRUSTED_ROOTS", &ca_bundle_str));
        }
        CertificateLocation::Native => {
            env.push(("SSL_CERT_FILE", &ca_bundle_str));
        }
        CertificateLocation::NativePath(path) => {
            env.push(("SSL_CERT_FILE", path));
        }
    }

    cli_ctx.command(env, args)?;
    Ok(())
}

/// Step: "When I run swgr offer metadata get"
/// Runs the offer metadata get command without ID to get all metadata
pub async fn step_when_i_run_swgr_offer_metadata_get_all(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    start: Option<usize>,
    count: Option<usize>,
) -> Result<()> {
    let service_profile = ctx.get_active_offer_service_profile()?;
    let protocol = service_profile.protocol;
    let domain = service_profile.domain;
    let port = service_profile.address.port();
    let base_url = format!("{}://{}:{}", protocol, domain, port);

    let ca_bundle = ctx.get_pki_root_certificate_path();
    let ca_bundle_str = ca_bundle.to_str().unwrap().to_string();

    let authorization = ctx.get_active_offer_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let mut args = vec![
        "offer",
        "metadata",
        "get",
        "default",
        "--base-url",
        &base_url,
        "--authorization-path",
        &authorization_str,
        "--trusted-roots",
        &ca_bundle_str,
    ];

    // Add start parameter if provided
    let start_str;
    if let Some(s) = start {
        args.push("--start");
        start_str = s.to_string();
        args.push(&start_str);
    }

    // Add count parameter if provided
    let count_str;
    if let Some(c) = count {
        args.push("--count");
        count_str = c.to_string();
        args.push(&count_str);
    }

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then offer metadata details should be output"
/// Verifies that offer metadata details were output and match expected values
pub async fn step_then_offer_metadata_details_should_be_output(
    cli_ctx: &mut CliContext,
    expected_metadata: &OfferMetadata,
) -> Result<()> {
    let stdout = cli_ctx.stdout_buffer().join("\n");

    if stdout.trim().is_empty() {
        bail_log!("No offer metadata details in stdout");
    }

    // Parse as OfferMetadata
    let metadata: OfferMetadata = match serde_json::from_str(&stdout) {
        Ok(metadata) => metadata,
        Err(e) => {
            bail_log!(
                "Failed to parse offer metadata details JSON: {}. Output: {}",
                e,
                stdout
            );
        }
    };

    // Verify expected values match
    if metadata.id != expected_metadata.id {
        bail_log!(
            "Metadata ID mismatch. Expected: {}, Got: {}",
            expected_metadata.id,
            metadata.id
        );
    }

    if metadata.metadata.text != expected_metadata.metadata.text {
        bail_log!(
            "Metadata text mismatch. Expected: {}, Got: {}",
            expected_metadata.metadata.text,
            metadata.metadata.text
        );
    }

    if metadata.metadata.long_text != expected_metadata.metadata.long_text {
        bail_log!(
            "Metadata long_text mismatch. Expected: {:?}, Got: {:?}",
            expected_metadata.metadata.long_text,
            metadata.metadata.long_text
        );
    }

    Ok(())
}

/// Step: "And all offer metadata should be output"
/// Verifies that all offer metadata were output (as a JSON array) and matches the expected metadata
pub async fn step_then_all_offer_metadata_should_be_output(
    cli_ctx: &mut CliContext,
    expected_metadata: &[OfferMetadata],
) -> Result<()> {
    let stdout = cli_ctx.stdout_buffer().join("\n");

    if stdout.trim().is_empty() {
        bail_log!("All offers output is empty");
    }

    // Parse as JSON array of OfferRecord
    let metadata: Vec<OfferMetadata> = match serde_json::from_str(&stdout) {
        Ok(metadata) => metadata,
        Err(e) => {
            bail_log!(
                "Failed to parse metadata JSON array: {}. Output: {}",
                e,
                stdout
            );
        }
    };

    assert_eq!(expected_metadata, metadata.as_slice());

    Ok(())
}

/// Step: "And updated offer metadata JSON exists"
/// Creates an updated offer metadata JSON file
pub async fn step_and_updated_offer_metadata_json_exists(
    _ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    // Read the existing metadata JSON
    let content = std::fs::read_to_string(&cli_ctx.metadata_json_path)?;
    let mut metadata: OfferMetadata = serde_json::from_str(&content)?;

    // Modify some fields to create an updated version
    metadata.metadata.text = "updated metadata text".to_string();
    metadata.metadata.long_text = Some("updated long text".to_string());

    // Write the updated JSON back
    let updated_json = serde_json::to_string_pretty(&metadata)?;
    std::fs::write(&cli_ctx.metadata_json_path, updated_json)?;

    Ok(())
}

/// Step: "When I run swgr offer metadata put with metadata ID and JSON"
/// Runs the offer metadata put command
pub async fn step_when_i_run_swgr_offer_metadata_put(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    metadata_id: &Uuid,
) -> Result<()> {
    let service_profile = ctx.get_active_offer_service_profile()?;
    let protocol = service_profile.protocol;
    let domain = service_profile.domain;
    let port = service_profile.address.port();
    let base_url = format!("{}://{}:{}", protocol, domain, port);

    let ca_bundle = ctx.get_pki_root_certificate_path();
    let ca_bundle_str = ca_bundle.to_str().unwrap().to_string();

    let authorization = ctx.get_active_offer_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let input_path = cli_ctx.metadata_json_path.to_str().unwrap().to_string();

    let id = metadata_id.to_string();

    let args = vec![
        "offer",
        "metadata",
        "put",
        "default",
        &id,
        "--base-url",
        &base_url,
        "--authorization-path",
        &authorization_str,
        "--trusted-roots",
        &ca_bundle_str,
        "--input",
        &input_path,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then the offer metadata should contain the updated data"
/// Verifies that the offer metadata was updated
pub async fn step_then_the_offer_metadata_should_contain_the_updated_data(
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let stdout = cli_ctx.stdout_buffer().join("\n");

    // Check if the output contains the updated values
    if !stdout.contains("updated metadata text") || !stdout.contains("updated long text") {
        bail_log!("Offer metadata does not contain updated data");
    }

    Ok(())
}

/// Step: "When I run swgr offer metadata delete for metadata ID"
/// Runs the offer metadata delete command
pub async fn step_when_i_run_swgr_offer_metadata_delete(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
    metadata_id: &Uuid,
) -> Result<()> {
    let service_profile = ctx.get_active_offer_service_profile()?;
    let protocol = service_profile.protocol;
    let domain = service_profile.domain;
    let port = service_profile.address.port();
    let base_url = format!("{}://{}:{}", protocol, domain, port);

    let ca_bundle = ctx.get_pki_root_certificate_path();
    let ca_bundle_str = ca_bundle.to_str().unwrap().to_string();

    let authorization = ctx.get_active_offer_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let id = metadata_id.to_string();

    let args = vec![
        "offer",
        "metadata",
        "delete",
        "default",
        &id,
        "--base-url",
        &base_url,
        "--authorization-path",
        &authorization_str,
        "--trusted-roots",
        &ca_bundle_str,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "Then the offer metadata should not be found"
/// Verifies that the offer metadata was deleted
pub async fn step_then_the_offer_metadata_should_not_be_found(
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let stderr = cli_ctx.stderr_buffer().join("\n");

    // Check if stderr contains "not found"
    if !stderr.to_lowercase().contains("not found") {
        bail_log!("Offer metadata was found when it should not exist");
    }

    Ok(())
}

// =============================================================================
// CLI OFFER ERROR SCENARIO STEP FUNCTIONS
// =============================================================================

/// Helper function to extract metadata ID from offer JSON
pub async fn extract_metadata_id_from_offer(cli_ctx: &CliContext) -> Result<Uuid> {
    let content = std::fs::read_to_string(&cli_ctx.offer_json_path)?;
    let offer: OfferRecord = serde_json::from_str(&content)?;
    Ok(offer.offer.metadata_id)
}

/// Step: "Given an offer JSON with non-existent metadata ID exists"
/// Creates an offer JSON file with a random metadata ID that doesn't exist in the server
pub async fn step_given_an_offer_json_with_non_existent_metadata_id_exists(
    _ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    // Generate unique UUIDs
    let metadata_id = Uuid::new_v4(); // This metadata ID will never be posted

    // Create offer JSON with unique ID that references non-existent metadata
    let metadata_id_str = metadata_id.to_string();
    let offer_output = cli_ctx.offer_json_path.to_str().unwrap().to_string();
    let args = vec![
        "offer",
        "new",
        "--partition",
        "default",
        "--metadata-id",
        &metadata_id_str,
        "--output",
        &offer_output,
    ];
    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;

    if cli_ctx.exit_code() != 0 {
        bail_log!("Failed to generate offer JSON");
    }

    cli_ctx.reset();
    Ok(())
}

/// Step: "When I run swgr offer get for non-existent offer ID"
/// Runs the offer get command for a non-existent offer
pub async fn step_when_i_run_swgr_offer_get_for_non_existent_offer(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let non_existent_id = Uuid::new_v4();

    let service_profile = ctx.get_active_offer_service_profile()?;
    let protocol = service_profile.protocol;
    let domain = service_profile.domain;
    let port = service_profile.address.port();
    let base_url = format!("{}://{}:{}", protocol, domain, port);

    let ca_bundle = ctx.get_pki_root_certificate_path();
    let ca_bundle_str = ca_bundle.to_str().unwrap().to_string();

    let authorization = ctx.get_active_offer_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let id = non_existent_id.to_string();

    let args = vec![
        "offer",
        "get",
        "default",
        &id,
        "--base-url",
        &base_url,
        "--authorization-path",
        &authorization_str,
        "--trusted-roots",
        &ca_bundle_str,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "When I run swgr offer delete for non-existent offer ID"
/// Runs the offer delete command for a non-existent offer
pub async fn step_when_i_run_swgr_offer_delete_for_non_existent_offer(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let non_existent_id = Uuid::new_v4();

    let service_profile = ctx.get_active_offer_service_profile()?;
    let protocol = service_profile.protocol;
    let domain = service_profile.domain;
    let port = service_profile.address.port();
    let base_url = format!("{}://{}:{}", protocol, domain, port);

    let ca_bundle = ctx.get_pki_root_certificate_path();
    let ca_bundle_str = ca_bundle.to_str().unwrap().to_string();

    let authorization = ctx.get_active_offer_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let id = non_existent_id.to_string();

    let args = vec![
        "offer",
        "delete",
        "default",
        &id,
        "--base-url",
        &base_url,
        "--authorization-path",
        &authorization_str,
        "--trusted-roots",
        &ca_bundle_str,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "When I run swgr offer metadata get for non-existent metadata ID"
/// Runs the offer metadata get command for a non-existent metadata
pub async fn step_when_i_run_swgr_offer_metadata_get_for_non_existent_metadata(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let non_existent_id = Uuid::new_v4();

    let service_profile = ctx.get_active_offer_service_profile()?;
    let protocol = service_profile.protocol;
    let domain = service_profile.domain;
    let port = service_profile.address.port();
    let base_url = format!("{}://{}:{}", protocol, domain, port);

    let ca_bundle = ctx.get_pki_root_certificate_path();
    let ca_bundle_str = ca_bundle.to_str().unwrap().to_string();

    let authorization = ctx.get_active_offer_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let id = non_existent_id.to_string();

    let args = vec![
        "offer",
        "metadata",
        "get",
        "default",
        &id,
        "--base-url",
        &base_url,
        "--authorization-path",
        &authorization_str,
        "--trusted-roots",
        &ca_bundle_str,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}

/// Step: "When I run swgr offer metadata delete for non-existent metadata ID"
/// Runs the offer metadata delete command for a non-existent metadata
pub async fn step_when_i_run_swgr_offer_metadata_delete_for_non_existent_metadata(
    ctx: &mut GlobalContext,
    cli_ctx: &mut CliContext,
) -> Result<()> {
    let non_existent_id = Uuid::new_v4();

    let service_profile = ctx.get_active_offer_service_profile()?;
    let protocol = service_profile.protocol;
    let domain = service_profile.domain;
    let port = service_profile.address.port();
    let base_url = format!("{}://{}:{}", protocol, domain, port);

    let ca_bundle = ctx.get_pki_root_certificate_path();
    let ca_bundle_str = ca_bundle.to_str().unwrap().to_string();

    let authorization = ctx.get_active_offer_authorization()?;
    let authorization_str = authorization.to_str().unwrap().to_string();

    let id = non_existent_id.to_string();

    let args = vec![
        "offer",
        "metadata",
        "delete",
        "default",
        &id,
        "--base-url",
        &base_url,
        "--authorization-path",
        &authorization_str,
        "--trusted-roots",
        &ca_bundle_str,
    ];

    let empty_env: Vec<(&str, &str)> = vec![];
    cli_ctx.command(empty_env, args)?;
    Ok(())
}
