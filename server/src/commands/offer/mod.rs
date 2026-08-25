use crate::commands::error::CliError;
use crate::commands::offer::metadata::OfferMetadataManagementCommands;
use crate::commands::offer::record::OfferRecordManagementCommands;
use crate::commands::token::TokenCommands;
use clap::{Parser, Subcommand};
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;
use std::time::Duration;
use switchgear_components::offer::http::HttpOfferStore;
use switchgear_components::secrets::{SecretStore, SecretStoreConfig, SecretStoreSecretConfig};
use switchgear_error::{ChainedContext, ErrorOrigin, ForeignContext};
use url::Url;

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
) -> Result<HttpOfferStore, CliError> {
    let base_url = match &client_configuration.base_url {
        None => {
            let base_url = env::var("OFFER_STORE_HTTP_BASE_URL")
                .foreign_context("Missing OFFER_STORE_HTTP_BASE_URL", ErrorOrigin::Downstream)?;
            Url::parse(&base_url).with_foreign_context(|| format!("parsing {base_url}"), None)?
        }
        Some(base_url) => base_url.clone(),
    };
    let authorization_path = match &client_configuration.authorization_path {
        None => {
            let authorization_path = env::var("OFFER_STORE_HTTP_AUTHORIZATION").foreign_context(
                "Missing OFFER_STORE_HTTP_AUTHORIZATION",
                ErrorOrigin::Downstream,
            )?;
            PathBuf::from(authorization_path)
        }
        Some(authorization_path) => authorization_path.clone(),
    };

    let trusted_roots_path = match &client_configuration.trusted_roots {
        None => env::var("OFFER_STORE_HTTP_TRUSTED_ROOTS")
            .ok()
            .map(PathBuf::from),
        Some(trusted_roots_path) => Some(trusted_roots_path.clone()),
    };

    let trusted_roots = if let Some(trusted_roots_path) = trusted_roots_path {
        CertificateDer::pem_file_iter(&trusted_roots_path)
            .with_foreign_context(
                || format!("parsing root certificate: {}", trusted_roots_path.display()),
                None,
            )?
            .collect::<Result<Vec<_>, _>>()
            .with_foreign_context(
                || format!("parsing root certificate: {}", trusted_roots_path.display()),
                None,
            )?
    } else {
        vec![]
    };

    let secret_store = SecretStore::create(&SecretStoreConfig {
        ttl: 300.0,
        secrets: BTreeMap::from([(
            "ADHOC".to_owned(),
            SecretStoreSecretConfig {
                path: authorization_path,
            },
        )]),
    });

    HttpOfferStore::create(
        base_url,
        Duration::from_secs(1),
        Duration::from_secs(1),
        &trusted_roots,
        secret_store,
        "ADHOC",
    )
    .chained_context("creating http offer store client", None)
}
