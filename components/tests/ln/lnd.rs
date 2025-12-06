use crate::try_create_lnd_backend_implementation;
use anyhow::anyhow;
use bitcoin_hashes::Hash;
use lightning_invoice::Bolt11Invoice;
use rand::{distributions::Alphanumeric, Rng};
use sha2::Digest;
use std::str::FromStr;
use std::time::Duration;
use switchgear_components::pool::lnd::grpc::client::TonicLndGrpcClient;
use switchgear_components::pool::{Bolt11InvoiceDescription, LnRpcClient};
use switchgear_testing::credentials::lightning::LnCredentials;

type LnClientBox = Box<
    dyn LnRpcClient<Error = switchgear_components::pool::error::LnPoolError>
        + Send
        + Sync
        + 'static,
>;

async fn create_lnd_tonic_client(credentials: &LnCredentials) -> anyhow::Result<LnClientBox> {
    let _ = rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow!("failed to stand up rustls encryption platform"));

    let backend = try_create_lnd_backend_implementation(credentials)?;

    let client = TonicLndGrpcClient::create(Duration::from_secs(1), backend, &[])?;

    Ok(Box::new(client))
}

#[tokio::test]
async fn test_lnd_tonic_invoice_with_direct_description() {
    let credentials = LnCredentials::create().unwrap();
    let client = create_lnd_tonic_client(&credentials).await.unwrap();

    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();
    let expected_amount_msat = 2_000_000; // 2000 sats in msat
    let expected_expiry_secs = 7200; // 2 hours expiry

    let description = Bolt11InvoiceDescription::Direct(&random_string);
    let invoice_str = client
        .get_invoice(
            Some(expected_amount_msat),
            description,
            Some(expected_expiry_secs),
        )
        .await
        .expect("Failed to generate LND invoice with direct description");

    let invoice = Bolt11Invoice::from_str(&invoice_str).expect("Failed to parse generated invoice");

    // Validate amount
    assert_eq!(
        invoice.amount_milli_satoshis().unwrap(),
        expected_amount_msat
    );

    // Validate description is Direct type with correct content
    match invoice.description() {
        lightning_invoice::Bolt11InvoiceDescriptionRef::Direct(desc) => {
            let desc_str = desc.to_string();
            assert_eq!(
                desc_str, random_string,
                "Invoice description '{desc_str}' doesn't match expected '{random_string}'",
            );
        }
        lightning_invoice::Bolt11InvoiceDescriptionRef::Hash(_) => {
            panic!("Expected Direct description but got Hash description");
        }
    }

    assert_eq!(invoice.expiry_time().as_secs(), expected_expiry_secs);
}

#[tokio::test]
async fn test_lnd_tonic_invoice_with_hash_description() {
    let credentials = LnCredentials::create().unwrap();
    let client = create_lnd_tonic_client(&credentials).await.unwrap();

    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();
    let expected_amount_msat = 1_500_000; // 1500 sats in msat

    // Create a hash from the random string
    let hash: [u8; 32] = sha2::Sha256::digest(random_string.as_bytes()).into();

    let description = Bolt11InvoiceDescription::Hash(&hash);
    let invoice_str = client
        .get_invoice(Some(expected_amount_msat), description, Some(3600))
        .await
        .expect("Failed to generate LND invoice with hash description");

    let invoice = Bolt11Invoice::from_str(&invoice_str).expect("Failed to parse generated invoice");

    // Validate amount
    assert_eq!(
        invoice.amount_milli_satoshis().unwrap(),
        expected_amount_msat
    );

    // Validate description is Hash type with correct hash
    match invoice.description() {
        lightning_invoice::Bolt11InvoiceDescriptionRef::Hash(invoice_hash) => {
            let invoice_hash_bytes = invoice_hash.0.to_byte_array();
            assert_eq!(hash, invoice_hash_bytes);
        }
        lightning_invoice::Bolt11InvoiceDescriptionRef::Direct(_) => {
            panic!("Expected Hash description but got Direct description");
        }
    }
}

#[tokio::test]
async fn test_lnd_tonic_invoice_with_none_amount() {
    let credentials = LnCredentials::create().unwrap();
    let client = create_lnd_tonic_client(&credentials).await.unwrap();

    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();

    let description = Bolt11InvoiceDescription::Direct(&random_string);
    let invoice_str = client
        .get_invoice(
            None, // No amount specified
            description,
            Some(3600),
        )
        .await
        .expect("Failed to generate LND invoice with no amount");

    let invoice = Bolt11Invoice::from_str(&invoice_str).expect("Failed to parse generated invoice");

    // Validate that amount is None (zero-amount invoice)
    assert!(
        invoice.amount_milli_satoshis().is_none(),
        "Expected no amount but got: {:?}",
        invoice.amount_milli_satoshis()
    );

    // Validate description
    match invoice.description() {
        lightning_invoice::Bolt11InvoiceDescriptionRef::Direct(desc) => {
            assert_eq!(desc.to_string(), random_string);
        }
        _ => panic!("Expected Direct description"),
    }

    // Validate expiry
    assert_eq!(invoice.expiry_time().as_secs(), 3600);
}

#[tokio::test]
async fn test_lnd_tonic_invoice_with_direct_into_hash_description() {
    let credentials = LnCredentials::create().unwrap();
    let client = create_lnd_tonic_client(&credentials).await.unwrap();

    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();
    let expected_amount_msat = 1_000_000; // 1000 sats in msat
    let expected_expiry_secs = 3600; // 1 hour expiry

    let description = Bolt11InvoiceDescription::DirectIntoHash(&random_string);
    let invoice_str = client
        .get_invoice(
            Some(expected_amount_msat),
            description,
            Some(expected_expiry_secs),
        )
        .await
        .expect("Failed to generate LND invoice with DirectIntoHash description");

    let invoice = Bolt11Invoice::from_str(&invoice_str).expect("Failed to parse generated invoice");

    // Validate amount
    assert_eq!(
        invoice.amount_milli_satoshis().unwrap(),
        expected_amount_msat
    );

    // Validate description is Hash type with the correct hash of our random string
    match invoice.description() {
        lightning_invoice::Bolt11InvoiceDescriptionRef::Hash(invoice_hash) => {
            // Calculate expected hash
            let expected_hash: [u8; 32] = sha2::Sha256::digest(random_string.as_bytes()).into();
            let invoice_hash_bytes = invoice_hash.0.to_byte_array();
            assert_eq!(
                expected_hash, invoice_hash_bytes,
                "Invoice hash should match SHA256 of input description"
            );
        }
        lightning_invoice::Bolt11InvoiceDescriptionRef::Direct(_) => {
            panic!("Expected Hash description but got Direct description");
        }
    }

    // Validate expiry
    assert_eq!(invoice.expiry_time().as_secs(), expected_expiry_secs);
}

#[tokio::test]
async fn test_lnd_tonic_metrics() {
    let credentials = LnCredentials::create().unwrap();
    let client = create_lnd_tonic_client(&credentials).await.unwrap();

    let metrics_result = client
        .get_metrics()
        .await
        .expect("Failed to connect to LND node and retrieve metrics");

    assert!(
        metrics_result.healthy,
        "Expected metrics response (proving LND connectivity) but got None"
    );
}
