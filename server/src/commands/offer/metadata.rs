use crate::commands::offer::{create_offer_client, OfferManagementClientConfig};
use crate::commands::{cli_read_to_string, cli_write_all};
use anyhow::Context;
use clap::Parser;
use log::{info, warn};
use std::path::{Path, PathBuf};
use switchgear_service::api::offer::{
    OfferMetadata, OfferMetadataIdentifier, OfferMetadataRest, OfferMetadataSparse,
    OfferMetadataStore,
};
use uuid::Uuid;

#[derive(Parser, Debug)]
pub enum OfferMetadataManagementCommands {
    /// Generate offer metadata JSON
    #[command(name = "new")]
    New {
        /// Optional output path, defaults to stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Get offer metadata
    #[command(name = "get")]
    Get {
        /// Partition name
        partition: String,
        /// Optional offer metadata uuid, default returns all offers for partition
        id: Option<Uuid>,
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

pub fn new_metadata(output: Option<&Path>) -> anyhow::Result<()> {
    let metadata: OfferMetadata = OfferMetadata {
        id: "88deff7e-ca45-4144-8fca-286a5a18fb1a".parse()?,
        partition: "default".to_string(),
        metadata: OfferMetadataSparse {
            text: "mandatory offer text".to_string(),
            long_text: Some("optional long offer text".to_string()),
            image: None,
            identifier: Some(OfferMetadataIdentifier::Email(
                "optional@email.com".parse()?,
            )),
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
    info!("Load it into the Offer Service with: swgr offer metadata post -i <file-path>");
    Ok(())
}

pub async fn get_metadata(
    partition: &str,
    id: Option<&Uuid>,
    output: Option<&Path>,
    client_configuration: &OfferManagementClientConfig,
) -> anyhow::Result<()> {
    let client = create_offer_client(client_configuration)?;
    if let Some(id) = id {
        if let Some(metadata) = client.get_metadata(partition, id).await? {
            let metadata = OfferMetadataRest {
                location: format!("{}/{}", metadata.partition, metadata.id),
                metadata,
            };
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
            warn!("Metadata {id} not found");
        }
    } else {
        let metadata = client.get_all_metadata(partition).await?;
        let metadata = metadata
            .into_iter()
            .map(|metadata| OfferMetadataRest {
                location: format!("{}/{}", metadata.partition, metadata.id),
                metadata,
            })
            .collect::<Vec<_>>();
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
        warn!("Conflict. Metadata already exists at: {}", metadata.id);
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
        warn!("Not Found: {id}");
    }
    Ok(())
}
