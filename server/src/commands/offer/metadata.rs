use crate::commands::offer::{create_offer_client, OfferManagementClientConfig};
use crate::commands::{cli_read_to_string, cli_write_all};
use anyhow::{bail, Context};
use clap::Parser;
use log::info;
use std::path::{Path, PathBuf};
use switchgear_service_api::offer::{OfferMetadata, OfferMetadataSparse, OfferMetadataStore};
use uuid::Uuid;

#[derive(Parser, Debug)]
pub enum OfferMetadataManagementCommands {
    /// Generate offer metadata JSON
    #[command(name = "new")]
    New {
        /// Partition name
        #[arg(short, long)]
        partition: String,
        /// Metadata text description
        #[arg(short, long)]
        text: String,
        /// Optional output path, defaults to stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Get offer metadata
    #[command(name = "get")]
    Get {
        /// Partition name
        partition: String,
        /// Optional offer metadata uuid, default returns all metadata for partition
        id: Option<Uuid>,
        /// Start position when returning multiple metadata
        #[arg(short, long, conflicts_with = "id", default_value_t = 0)]
        start: usize,
        /// Count position when returning multiple metadata
        #[arg(short, long, conflicts_with = "id", default_value_t = 100)]
        count: usize,
        /// Optional output metadata path, defaults to stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[clap(flatten)]
        client: OfferManagementClientConfig,
    },

    /// Load new offer metadata
    #[command(name = "post")]
    Post {
        /// Optional offer metadata JSON source path, defaults to stdin
        #[arg(short, long)]
        input: Option<PathBuf>,
        #[clap(flatten)]
        client: OfferManagementClientConfig,
    },

    /// Update offer metadata
    #[command(name = "put")]
    Put {
        /// Partition name
        partition: String,
        /// Offer metadata uuid
        id: Uuid,
        /// Optional offer metadata JSON source path, defaults to stdin
        #[arg(short, long)]
        input: Option<PathBuf>,
        #[clap(flatten)]
        client: OfferManagementClientConfig,
    },

    /// Delete offer metadata
    #[command(name = "delete")]
    Delete {
        /// Partition name
        partition: String,
        /// Offer metadata uuid
        id: Uuid,
        #[clap(flatten)]
        client: OfferManagementClientConfig,
    },
}

pub fn new_metadata(partition: &str, text: &str, output: Option<&Path>) -> anyhow::Result<()> {
    let metadata: OfferMetadata = OfferMetadata {
        id: Uuid::new_v4(),
        partition: partition.to_string(),
        metadata: OfferMetadataSparse {
            text: text.to_string(),
            long_text: None,
            image: None,
            identifier: None,
        },
    };

    let metadata = serde_json::to_string_pretty(&metadata)?;
    cli_write_all(output, metadata.as_bytes()).with_context(|| {
        format!(
            "writing metadata to: {}",
            output.map_or_else(|| "stdout".to_string(), |o| o.to_string_lossy().to_string())
        )
    })?;

    info!("Modify this JSON file to create unique offer metadata");
    info!("Load it into the Offer Service. See: swgr offer metadata post --help");
    Ok(())
}

pub async fn get_metadata(
    partition: &str,
    id: Option<&Uuid>,
    start: usize,
    count: usize,
    output: Option<&Path>,
    client_configuration: &OfferManagementClientConfig,
) -> anyhow::Result<()> {
    let client = create_offer_client(client_configuration)?;
    if let Some(id) = id {
        if let Some(metadata) = client.get_metadata(partition, id).await? {
            let metadata = serde_json::to_string_pretty(&metadata)
                .with_context(|| format!("serializing metadata {id}"))?;
            cli_write_all(output, metadata.as_bytes()).with_context(|| {
                format!(
                    "writing metadata to: {}",
                    output
                        .map_or_else(|| "stdout".to_string(), |o| o.to_string_lossy().to_string())
                )
            })?;
        } else {
            bail!("Metadata {id} not found");
        }
    } else {
        let metadata = client.get_all_metadata(partition, start, count).await?;
        let metadata = serde_json::to_string_pretty(&metadata)
            .with_context(|| format!("serializing metadata for {partition}"))?;
        cli_write_all(output, metadata.as_bytes()).with_context(|| {
            format!(
                "writing metadata to: {}",
                output.map_or_else(|| "stdout".to_string(), |o| o.to_string_lossy().to_string())
            )
        })?;
    }

    Ok(())
}

pub async fn post_metadata(
    metadata_path: Option<&Path>,
    client_configuration: &OfferManagementClientConfig,
) -> anyhow::Result<()> {
    let client = create_offer_client(client_configuration)?;
    let mut metadata = String::new();
    cli_read_to_string(metadata_path, &mut metadata).with_context(|| {
        format!(
            "reading metadata: {}",
            metadata_path.map_or_else(|| "stdin".to_string(), |o| o.to_string_lossy().to_string())
        )
    })?;

    let metadata: OfferMetadata = serde_json::from_str(&metadata).with_context(|| {
        format!(
            "parsing metadata from: {}",
            metadata_path.map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
        )
    })?;
    if let Some(created) = client.post_metadata(metadata.clone()).await? {
        info!("Created: {created}");
    } else {
        bail!("Conflict. Metadata already exists at: {}", metadata.id);
    }
    Ok(())
}

pub async fn put_metadata(
    partition: &str,
    id: &Uuid,
    offer_path: Option<&Path>,
    client_configuration: &OfferManagementClientConfig,
) -> anyhow::Result<()> {
    let client = create_offer_client(client_configuration)?;

    let mut metadata = String::new();
    cli_read_to_string(offer_path, &mut metadata).with_context(|| {
        format!(
            "reading metadata: {}",
            offer_path.map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
        )
    })?;
    let metadata: OfferMetadataSparse = serde_json::from_str(&metadata).with_context(|| {
        format!(
            "parsing metadata from: {}",
            offer_path.map_or_else(|| "stdin".to_string(), |b| b.to_string_lossy().to_string())
        )
    })?;
    let metadata = OfferMetadata {
        partition: partition.to_string(),
        id: *id,
        metadata,
    };
    if client.put_metadata(metadata.clone()).await? {
        info!("Created: {}", metadata.id);
    } else {
        info!("Updated: {}", metadata.id);
    }
    Ok(())
}

pub async fn delete_metadata(
    partition: &str,
    id: &Uuid,
    client_configuration: &OfferManagementClientConfig,
) -> anyhow::Result<()> {
    let client = create_offer_client(client_configuration)?;
    if client.delete_metadata(partition, id).await? {
        info!("Deleted: {id}");
    } else {
        bail!("Not Found: {id}");
    }
    Ok(())
}
