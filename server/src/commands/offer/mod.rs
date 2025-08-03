use crate::commands::offer::metadata::OfferMetadataManagementCommands;
use crate::commands::offer::record::OfferRecordManagementCommands;
use crate::commands::token::TokenCommands;
use anyhow::{anyhow, Context};
use clap::{Parser, Subcommand};
use reqwest::{Certificate, Url};
use std::path::PathBuf;
use std::time::Duration;
use std::{env, fs};
use switchgear_service::components::offer::http::HttpOfferStore;

pub mod metadata;
pub mod record;
pub mod token;

#[derive(Subcommand, Debug)]
pub enum OfferCommands {
    /// Manage offer service token
    #[clap(subcommand)]
    Token(TokenCommands),

    #[clap(flatten)]
    Offer(OfferRecordManagementCommands),

    /// Manage offer metadata
    #[clap(subcommand, name = "metadata")]
    Metadata(OfferMetadataManagementCommands),
}

#[derive(Parser, Debug)]
pub struct OfferManagementClientConfig {
    /// Optional Offer Service base Url. Also set with OFFER_STORE_HTTP_BASE_URL env var
    #[clap(short, long)]
    base_url: Option<Url>,
    /// Optional Offer Service authorization token file path. Also set with OFFER_STORE_HTTP_AUTHORIZATION env var
    #[clap(short, long)]
    authorization_path: Option<PathBuf>,
    /// Optional Offer Service trusted roots file path, in pem format. Also set with OFFER_STORE_HTTP_TRUSTED_ROOTS env var
    #[clap(short, long)]
    trusted_roots: Option<PathBuf>,
}

pub fn create_offer_client(
    client_configuration: &OfferManagementClientConfig,
) -> anyhow::Result<HttpOfferStore> {
    let base_url = match &client_configuration.base_url {
        None => {
            let base_url = env::var("OFFER_STORE_HTTP_BASE_URL")
                .map_err(|_| anyhow!("Missing OFFER_STORE_HTTP_BASE_URL"))?;
            Url::parse(&base_url).with_context(|| format!("parsing {base_url}"))?
        }
        Some(base_url) => base_url.clone(),
    };
    let authorization_path = match &client_configuration.authorization_path {
        None => {
            let authorization_path = env::var("OFFER_STORE_HTTP_AUTHORIZATION")
                .map_err(|_| anyhow!("Missing OFFER_STORE_HTTP_AUTHORIZATION"))?;
            PathBuf::from(authorization_path)
        }
        Some(authorization_path) => authorization_path.clone(),
    };
    let authorization = fs::read_to_string(&authorization_path).with_context(|| {
        format!(
            "reading authorization file: {}",
            authorization_path.to_string_lossy()
        )
    })?;

    let trusted_roots_path = match &client_configuration.trusted_roots {
        None => env::var("OFFER_STORE_HTTP_TRUSTED_ROOTS")
            .map_err(|_| anyhow!("Missing OFFER_STORE_HTTP_TRUSTED_ROOTS"))
            .ok()
            .map(PathBuf::from),
        Some(trusted_roots_path) => Some(trusted_roots_path.clone()),
    };

    let trusted_roots = if let Some(trusted_roots_path) = trusted_roots_path {
        let trusted_roots = fs::read(&trusted_roots_path).with_context(|| {
            format!(
                "reading trusted roots file: {}",
                trusted_roots_path.to_string_lossy()
            )
        })?;

        vec![Certificate::from_pem(&trusted_roots).with_context(|| {
            format!(
                "parsing trusted roots file: {}",
                trusted_roots_path.to_string_lossy()
            )
        })?]
    } else {
        vec![]
    };

    Ok(HttpOfferStore::create(
        base_url,
        Duration::from_secs(1),
        Duration::from_secs(1),
        trusted_roots,
        authorization,
    )?)
}
