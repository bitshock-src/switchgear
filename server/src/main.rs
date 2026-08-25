mod commands;
mod config;
mod di;
mod error;
mod signals;

use crate::commands::offer::metadata::OfferMetadataManagementCommands;
use crate::commands::offer::record::OfferRecordManagementCommands;
use crate::di::inject::injectors::config::ServerConfigInjector;
use crate::di::inject::injectors::service::tracing::TracingSystemInjector;
use crate::error::{ServerError, ServerErrorAccumulator};
use clap::{Parser, Subcommand};
use commands::discovery::DiscoveryCommands;
use commands::discovery::backend::DiscoveryBackendManagementCommands;
use commands::offer::OfferCommands;
use commands::services::ServiceEnablement;
use commands::token::TokenCommands;
use log::LevelFilter as LogLevelFilter;
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::path::PathBuf;
use std::process::ExitCode;
use switchgear_error::{ChainedContext, ErrorOrigin, ForeignContext};

/// lnurl load balance server
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub(crate) struct CliArgs {
    /// log level (CLI subcommands only; `swgr service` reads RUST_LOG)
    #[clap(short, long, value_parser)]
    pub(crate) log_level: Option<LogLevelFilter>,

    #[clap(subcommand)]
    pub(crate) command: RootCommands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum RootCommands {
    /// Run Switchgear services
    Service {
        /// Path to the YAML configuration file.
        #[clap(short, long, value_parser)]
        config: PathBuf,

        #[arg(value_enum, required = false)]
        enablement: Vec<ServiceEnablement>,
    },
    /// Manage offers
    #[clap(subcommand)]
    Offer(OfferCommands),

    /// Manage discovery
    #[clap(subcommand)]
    Discovery(DiscoveryCommands),
}

#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[tokio::main]
async fn main() -> ExitCode {
    let args = CliArgs::parse();

    if let Err(e) = TracingSystemInjector::init(&args) {
        eprintln!("failed to initialize tracing: {e}");
        return ExitCode::FAILURE;
    }

    match _main(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            for leaf in err.flatten() {
                leaf.emit_event("swgr failure");
            }
            ExitCode::FAILURE
        }
    }
}

async fn _main(args: CliArgs) -> Result<(), ServerError> {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| ServerError::internal("failed to stand up rustls encryption platform"))?;

    let mut otel_providers: Vec<(&'static str, SdkTracerProvider)> = Vec::new();
    let mut errors = ServerErrorAccumulator::new();

    let cli_result: Result<(), crate::commands::error::CliError> = match args.command {
        RootCommands::Service { config, enablement } => match ServerConfigInjector::new(config)
            .chained_context("loading server configuration", None)
        {
            Ok(ci) => commands::services::execute(ci, enablement, &mut otel_providers).await,
            Err(e) => Err(e),
        },
        RootCommands::Offer(offer) => match offer {
            OfferCommands::Token(token) => match token {
                TokenCommands::Mint {
                    key,
                    expires,
                    output,
                } => commands::offer::token::mint(&key, expires, output.as_deref()),
                TokenCommands::Key { public, private } => commands::token::key(&public, &private),
                TokenCommands::MintAll {
                    public,
                    private,
                    expires,
                    output,
                } => {
                    commands::offer::token::mint_all(&public, &private, expires, output.as_deref())
                }
                TokenCommands::Verify {
                    public,
                    token,
                    output,
                } => commands::offer::token::verify(&public, token.as_deref(), output.as_deref()),
            },
            OfferCommands::Offer(offer) => match offer {
                OfferRecordManagementCommands::New {
                    partition,
                    metadata_id,
                    output,
                } => {
                    commands::offer::record::new_offer(&partition, &metadata_id, output.as_deref())
                }
                OfferRecordManagementCommands::Get {
                    partition,
                    id,
                    start,
                    count,
                    output,
                    client,
                } => {
                    commands::offer::record::get_offer(
                        &partition,
                        id.as_ref(),
                        start,
                        count,
                        output.as_deref(),
                        &client,
                    )
                    .await
                }
                OfferRecordManagementCommands::Post { input, client } => {
                    commands::offer::record::post_offer(input.as_deref(), &client).await
                }
                OfferRecordManagementCommands::Put {
                    partition,
                    id,
                    input,
                    client,
                } => {
                    commands::offer::record::put_offer(
                        &partition,
                        id.as_ref(),
                        input.as_deref(),
                        &client,
                    )
                    .await
                }
                OfferRecordManagementCommands::Delete {
                    partition,
                    id,
                    client,
                } => commands::offer::record::delete_offer(&partition, id.as_ref(), &client).await,
            },
            OfferCommands::Metadata(metadata) => match metadata {
                OfferMetadataManagementCommands::New {
                    partition,
                    text,
                    output,
                } => commands::offer::metadata::new_metadata(&partition, &text, output.as_deref()),
                OfferMetadataManagementCommands::Get {
                    partition,
                    id,
                    start,
                    count,
                    output,
                    client,
                } => {
                    commands::offer::metadata::get_metadata(
                        &partition,
                        id.as_ref(),
                        start,
                        count,
                        output.as_deref(),
                        &client,
                    )
                    .await
                }
                OfferMetadataManagementCommands::Post { input, client } => {
                    commands::offer::metadata::post_metadata(input.as_deref(), &client).await
                }
                OfferMetadataManagementCommands::Put {
                    partition,
                    id,
                    input,
                    client,
                } => {
                    commands::offer::metadata::put_metadata(
                        &partition,
                        id.as_ref(),
                        input.as_deref(),
                        &client,
                    )
                    .await
                }
                OfferMetadataManagementCommands::Delete {
                    partition,
                    id,
                    client,
                } => {
                    commands::offer::metadata::delete_metadata(&partition, id.as_ref(), &client)
                        .await
                }
            },
        },
        RootCommands::Discovery(discovery) => match discovery {
            DiscoveryCommands::Token(token) => match token {
                TokenCommands::Mint {
                    key,
                    expires,
                    output,
                } => commands::discovery::token::mint(&key, expires, output.as_deref()),
                TokenCommands::Key { public, private } => commands::token::key(&public, &private),
                TokenCommands::MintAll {
                    public,
                    private,
                    expires,
                    output,
                } => commands::discovery::token::mint_all(
                    &public,
                    &private,
                    expires,
                    output.as_deref(),
                ),
                TokenCommands::Verify {
                    public,
                    token,
                    output,
                } => {
                    commands::discovery::token::verify(&public, token.as_deref(), output.as_deref())
                }
            },
            DiscoveryCommands::Backend(service) => match service {
                DiscoveryBackendManagementCommands::New {
                    node_type,
                    public_key,
                    partition,
                    name,
                    output,
                } => commands::discovery::backend::new_backend(
                    node_type,
                    &public_key,
                    name.as_deref(),
                    &partition,
                    output.as_deref(),
                ),
                DiscoveryBackendManagementCommands::List { client } => {
                    commands::discovery::backend::list_backends(&client).await
                }
                DiscoveryBackendManagementCommands::Get {
                    public_key: address,
                    output,
                    client,
                } => {
                    commands::discovery::backend::get_backend(
                        address.as_deref(),
                        output.as_deref(),
                        &client,
                    )
                    .await
                }
                DiscoveryBackendManagementCommands::Post { input, client } => {
                    commands::discovery::backend::post_backend(input.as_deref(), &client).await
                }
                DiscoveryBackendManagementCommands::Put {
                    public_key: address,
                    input,
                    client,
                } => {
                    commands::discovery::backend::put_backend(&address, input.as_deref(), &client)
                        .await
                }
                DiscoveryBackendManagementCommands::Patch {
                    public_key: address,
                    input,
                    client,
                } => {
                    commands::discovery::backend::patch_backend(&address, input.as_deref(), &client)
                        .await
                }
                DiscoveryBackendManagementCommands::Enable {
                    public_key: address,
                    client,
                } => commands::discovery::backend::enable_backend(&address, true, &client).await,
                DiscoveryBackendManagementCommands::Disable {
                    public_key: address,
                    client,
                } => commands::discovery::backend::enable_backend(&address, false, &client).await,
                DiscoveryBackendManagementCommands::Delete {
                    public_key: address,
                    client,
                } => commands::discovery::backend::delete_backend(&address, &client).await,
            },
        },
    };

    let result: Result<(), ServerError> =
        cli_result.with_chained_context(|| format!("cli command '{}'", summarize_command()), None);
    errors.push_result(result);

    let mut shutdowns = tokio::task::JoinSet::new();
    for (name, provider) in otel_providers {
        shutdowns.spawn_blocking(move || (name, provider.shutdown()));
    }
    while let Some(res) = shutdowns.join_next().await {
        match res {
            Ok((_, Ok(()))) => {}
            Ok((name, Err(e))) => {
                let r: Result<(), ServerError> = Err(e).with_foreign_context(
                    || format!("shutting down otel tracer provider for {name}"),
                    ErrorOrigin::Internal,
                );
                errors.push_result(r);
            }
            Err(e) => {
                let r: Result<(), ServerError> =
                    Err(e).foreign_context("otel shutdown task join failed", ErrorOrigin::Internal);
                errors.push_result(r);
            }
        }
    }

    errors.finish()
}

fn summarize_command() -> String {
    let matches = <CliArgs as clap::CommandFactory>::command().get_matches();
    let mut parts: Vec<&str> = Vec::new();
    let mut cur = &matches;
    while let Some((name, sub)) = cur.subcommand() {
        parts.push(name);
        cur = sub;
    }
    parts.join(" ")
}
