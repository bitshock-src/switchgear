use crate::commands::offer::{create_offer_client, OfferManagementClientConfig};
use crate::commands::{cli_read_to_string, cli_write_all};
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use clap::Parser;
use log::info;
use std::path::{Path, PathBuf};
use switchgear_service::api::offer::{OfferRecord, OfferRecordRest, OfferRecordSparse, OfferStore};
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

pub fn new_offer(partition: &str, metadata_id: &Uuid, output: Option<&Path>) -> anyhow::Result<()> {
    let offer = OfferRecord {
        partition: partition.to_string(),
        id: Uuid::new_v4(),
        offer: OfferRecordSparse {
            max_sendable: 0,
            min_sendable: 0,
            metadata_id: *metadata_id,
            #[allow(clippy::expect_used)]
            timestamp: DateTime::<Utc>::from_timestamp_secs(0).expect("unix epoch"),
            expires: Some(
                DateTime::<Utc>::from_timestamp_secs(86_400).expect("unix epoch + 24 hours"),
            ),
        },
    };

    let offer = serde_json::to_string_pretty(&offer)?;
    cli_write_all(output, offer.as_bytes()).with_context(|| {
        format!(
            "writing offer to: {}",
            output.map_or_else(|| "stdout".to_string(), |o| o.to_string_lossy().to_string())
        )
    })?;

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
) -> anyhow::Result<()> {
    let client = create_offer_client(client_configuration)?;
    if let Some(id) = id {
        if let Some(offer) = client.get_offer(partition, id).await? {
            let offer = OfferRecordRest {
                location: format!("{}/{}", offer.partition, offer.id),
                offer,
            };
            let offer = serde_json::to_string_pretty(&offer)
                .with_context(|| format!("serializing offer {id}"))?;
            cli_write_all(output, offer.as_bytes()).with_context(|| {
                format!(
                    "writing offer to: {}",
                    output
                        .map_or_else(|| "stdout".to_string(), |o| o.to_string_lossy().to_string())
                )
            })?;
        } else {
            bail!("Offer {id} not found");
        }
    } else {
        let offers = client.get_offers(partition, start, count).await?;
        let offers = offers
            .into_iter()
            .map(|offer| OfferRecordRest {
                location: format!("{}/{}", offer.partition, offer.id),
                offer,
            })
            .collect::<Vec<_>>();
        let offers = serde_json::to_string_pretty(&offers)
            .with_context(|| format!("serializing offer for {partition}"))?;
        cli_write_all(output, offers.as_bytes()).with_context(|| {
            format!(
                "writing offer to: {}",
                output.map_or_else(|| "stdout".to_string(), |o| o.to_string_lossy().to_string())
            )
        })?;
    }

    Ok(())
}

pub async fn post_offer(
    offer_path: Option<&Path>,
    client_configuration: &OfferManagementClientConfig,
) -> anyhow::Result<()> {
    let client = create_offer_client(client_configuration)?;
    let mut offer = String::new();
    cli_read_to_string(offer_path, &mut offer).with_context(|| {
        format!(
            "reading offer: {}",
            offer_path.map_or_else(|| "stdin".to_string(), |o| o.to_string_lossy().to_string())
        )
    })?;

    let offer: OfferRecord = serde_json::from_str(&offer).with_context(|| {
        format!(
            "parsing offer from: {}",
            offer_path.map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
        )
    })?;
    if let Some(created) = client.post_offer(offer.clone()).await? {
        info!("Offer created: {created}");
    } else {
        bail!("Conflict. Offer already exists at: {}", offer.id);
    }
    Ok(())
}

pub async fn put_offer(
    partition: &str,
    id: &Uuid,
    offer_path: Option<&Path>,
    client_configuration: &OfferManagementClientConfig,
) -> anyhow::Result<()> {
    let client = create_offer_client(client_configuration)?;

    let mut offer = String::new();
    cli_read_to_string(offer_path, &mut offer).with_context(|| {
        format!(
            "reading offer: {}",
            offer_path.map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
        )
    })?;
    let offer: OfferRecordSparse = serde_json::from_str(&offer).with_context(|| {
        format!(
            "parsing offer from: {}",
            offer_path.map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
        )
    })?;
    let offer = OfferRecord {
        partition: partition.to_string(),
        id: *id,
        offer,
    };
    if client.put_offer(offer.clone()).await? {
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
) -> anyhow::Result<()> {
    let client = create_offer_client(client_configuration)?;
    if client.delete_offer(partition, id).await? {
        info!("Offer deleted: {id}");
    } else {
        bail!("Offer not Found: {id}");
    }
    Ok(())
}
