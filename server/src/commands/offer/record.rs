use crate::commands::offer::{create_offer_client, OfferManagementClientConfig};
use crate::commands::{cli_read_to_string, cli_write_all};
use anyhow::Context;
use chrono::{DateTime, Utc};
use clap::Parser;
use log::{info, warn};
use std::path::{Path, PathBuf};
use switchgear_service::api::offer::{OfferRecord, OfferRecordRest, OfferRecordSparse, OfferStore};
use uuid::Uuid;

#[derive(Parser, Debug)]
pub enum OfferRecordManagementCommands {
    /// Generate offer JSON
    #[command(name = "new")]
    New {
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

pub fn new_offer(output: Option<&Path>) -> anyhow::Result<()> {
    let offer = OfferRecord {
        partition: "default".to_string(),
        id: "6a38ebdd-83ef-4b94-b843-3b18cd90a833".parse()?,
        offer: OfferRecordSparse {
            max_sendable: 1_000_000,
            min_sendable: 1_000_000,
            metadata_id: "88deff7e-ca45-4144-8fca-286a5a18fb1a".parse()?,
            #[allow(clippy::expect_used)]
            timestamp: DateTime::<Utc>::from_timestamp(0, 0).expect("unix epoch"),
            expires: None,
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
    info!("Load it into the Offer Service with: swgr offer post -i <file-path>");

    Ok(())
}

pub async fn get_offer(
    partition: &str,
    id: Option<&Uuid>,
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
            warn!("Offer {id} not found");
        }
    } else {
        let offers = client.get_offers(partition).await?;
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
        info!("Created: {created}");
    } else {
        warn!("Conflict. Offer already exists at: {}", offer.id);
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
        info!("Created: {}", offer.id);
    } else {
        info!("Updated: {}", offer.id);
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
        info!("Deleted: {id}");
    } else {
        warn!("Not Found: {id}");
    }
    Ok(())
}
