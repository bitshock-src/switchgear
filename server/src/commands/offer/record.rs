use crate::commands::error::CliError;
use crate::commands::offer::{OfferManagementClientConfig, create_offer_client};
use crate::commands::{cli_read_to_string, cli_write_all};
use chrono::{DateTime, Utc};
use clap::Parser;
use log::info;
use std::path::{Path, PathBuf};
use switchgear_error::{ChainedContext, ErrorOrigin, ForeignContext};
use switchgear_service_api::offer::{OfferRecord, OfferRecordSparse, OfferStore};
use uuid::Uuid;

#[derive(Parser, Debug)]
pub enum OfferRecordManagementCommands {
    /// Generate offer JSON
    #[command(name = "new")]
    New {
        /// Partition name
        #[arg(short, long)]
        partition: String,
        /// Offer Metadata UUID
        #[arg(short, long)]
        metadata_id: Uuid,
        /// Optional output path, defaults to stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Get an offer
    #[command(name = "get")]
    Get {
        /// Partition name
        partition: String,
        /// Optional offer uuid, default returns all offers for partition
        id: Option<Uuid>,
        /// Start position when returning multiple offers
        #[arg(short, long, conflicts_with = "id", default_value_t = 0)]
        start: usize,
        /// Count when returning multiple offers
        #[arg(short, long, conflicts_with = "id", default_value_t = 100)]
        count: usize,
        /// Optional output path, defaults to stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[clap(flatten)]
        client: OfferManagementClientConfig,
    },

    /// Load a new offer
    #[command(name = "post")]
    Post {
        /// Optional offer JSON source path, defaults to stdin
        #[arg(short, long)]
        input: Option<PathBuf>,
        #[clap(flatten)]
        client: OfferManagementClientConfig,
    },

    /// Update an offer
    #[command(name = "put")]
    Put {
        /// Partition name
        partition: String,
        /// Offer uuid
        id: Uuid,
        /// Optional offer JSON source path, defaults to stdin
        #[arg(short, long)]
        input: Option<PathBuf>,
        #[clap(flatten)]
        client: OfferManagementClientConfig,
    },

    /// Delete an offer
    #[command(name = "delete")]
    Delete {
        /// Partition name
        partition: String,
        /// Offer uuid
        id: Uuid,
        #[clap(flatten)]
        client: OfferManagementClientConfig,
    },
}

pub fn new_offer(
    partition: &str,
    metadata_id: &Uuid,
    output: Option<&Path>,
) -> Result<(), CliError> {
    let offer = OfferRecord {
        partition: partition.to_string(),
        id: Uuid::new_v4(),
        offer: OfferRecordSparse {
            max_sendable: 0,
            min_sendable: 0,
            metadata_id: *metadata_id,
            metadata: None,
            #[allow(clippy::expect_used)]
            timestamp: DateTime::<Utc>::from_timestamp_secs(0).expect("unix epoch"),
            #[allow(clippy::expect_used)]
            expires: Some(
                DateTime::<Utc>::from_timestamp_secs(86_400).expect("unix epoch + 24 hours"),
            ),
        },
    };

    let offer = serde_json::to_string_pretty(&offer)
        .foreign_context("serializing new offer", ErrorOrigin::Internal)?;
    cli_write_all(output, offer.as_bytes()).with_foreign_context(
        || {
            format!(
                "writing offer to: {}",
                output.map_or_else(|| "stdout".to_string(), |o| o.to_string_lossy().to_string())
            )
        },
        None,
    )?;

    info!("Modify this JSON file to create a unique offer");
    info!("Load it into the Offer Service. See: swgr offer post --help");

    Ok(())
}

pub async fn get_offer(
    partition: &str,
    id: Option<&Uuid>,
    start: usize,
    count: usize,
    output: Option<&Path>,
    client_configuration: &OfferManagementClientConfig,
) -> Result<(), CliError> {
    let client = create_offer_client(client_configuration)?;
    if let Some(id) = id {
        let fetched = client
            .get_offer(partition, id, None)
            .await
            .with_chained_context(|| format!("fetching offer {id}"), None)?;
        if let Some(offer) = fetched {
            let offer = serde_json::to_string_pretty(&offer).with_foreign_context(
                || format!("serializing offer {id}"),
                ErrorOrigin::Internal,
            )?;
            cli_write_all(output, offer.as_bytes()).with_foreign_context(
                || {
                    format!(
                        "writing offer to: {}",
                        output.map_or_else(
                            || "stdout".to_string(),
                            |o| o.to_string_lossy().to_string()
                        )
                    )
                },
                None,
            )?;
        } else {
            return Err(CliError::internal(format!("Offer {id} not found")));
        }
    } else {
        let offers = client
            .get_offers(partition, start, count)
            .await
            .with_chained_context(|| format!("listing offers in partition {partition}"), None)?;
        let offers = serde_json::to_string_pretty(&offers).with_foreign_context(
            || format!("serializing offer for {partition}"),
            ErrorOrigin::Internal,
        )?;
        cli_write_all(output, offers.as_bytes()).with_foreign_context(
            || {
                format!(
                    "writing offer to: {}",
                    output
                        .map_or_else(|| "stdout".to_string(), |o| o.to_string_lossy().to_string())
                )
            },
            None,
        )?;
    }

    Ok(())
}

pub async fn post_offer(
    offer_path: Option<&Path>,
    client_configuration: &OfferManagementClientConfig,
) -> Result<(), CliError> {
    let client = create_offer_client(client_configuration)?;
    let mut offer = String::new();
    cli_read_to_string(offer_path, &mut offer).with_foreign_context(
        || {
            format!(
                "reading offer: {}",
                offer_path.map_or_else(|| "stdin".to_string(), |o| o.to_string_lossy().to_string())
            )
        },
        None,
    )?;

    let offer: OfferRecord = serde_json::from_str(&offer).with_foreign_context(
        || {
            format!(
                "parsing offer from: {}",
                offer_path.map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
            )
        },
        None,
    )?;
    if let Some(created) = client
        .post_offer(offer.clone())
        .await
        .with_chained_context(|| format!("posting offer {}", offer.id), None)?
    {
        info!("Offer created: {created}");
    } else {
        return Err(CliError::internal(format!(
            "Conflict. Offer already exists at: {}",
            offer.id
        )));
    }
    Ok(())
}

pub async fn put_offer(
    partition: &str,
    id: &Uuid,
    offer_path: Option<&Path>,
    client_configuration: &OfferManagementClientConfig,
) -> Result<(), CliError> {
    let client = create_offer_client(client_configuration)?;

    let mut offer = String::new();
    cli_read_to_string(offer_path, &mut offer).with_foreign_context(
        || {
            format!(
                "reading offer: {}",
                offer_path.map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
            )
        },
        None,
    )?;
    let offer: OfferRecordSparse = serde_json::from_str(&offer).with_foreign_context(
        || {
            format!(
                "parsing offer from: {}",
                offer_path.map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
            )
        },
        None,
    )?;
    let offer = OfferRecord {
        partition: partition.to_string(),
        id: *id,
        offer,
    };
    if client
        .put_offer(offer.clone())
        .await
        .with_chained_context(|| format!("putting offer {}", offer.id), None)?
    {
        info!("Offer created: {}", offer.id);
    } else {
        info!("Offer updated: {}", offer.id);
    }
    Ok(())
}

pub async fn delete_offer(
    partition: &str,
    id: &Uuid,
    client_configuration: &OfferManagementClientConfig,
) -> Result<(), CliError> {
    let client = create_offer_client(client_configuration)?;
    if client
        .delete_offer(partition, id)
        .await
        .with_chained_context(|| format!("deleting offer {id}"), None)?
    {
        info!("Offer deleted: {id}");
    } else {
        return Err(CliError::internal(format!("Offer not Found: {id}")));
    }
    Ok(())
}
