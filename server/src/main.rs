mod commands;
mod config;
mod di;
mod signals;

use crate::commands::offer::metadata::OfferMetadataManagementCommands;
use crate::commands::offer::record::OfferRecordManagementCommands;
use clap::{Parser, Subcommand};
use commands::discovery::backend::DiscoveryBackendManagementCommands;
use commands::discovery::DiscoveryCommands;
use commands::offer::OfferCommands;
use commands::services::ServiceEnablement;
use commands::token::TokenCommands;
use log::{error, LevelFilter};
use simplelog::{ColorChoice, ConfigBuilder, TermLogger, TerminalMode};
use std::path::PathBuf;
use std::process::ExitCode;

/// lnurl load balance server
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct CliArgs {
    /// log level; overrides RUST_LOG
    #[clap(short, long, value_parser)]
    log_level: Option<LevelFilter>,

    #[clap(subcommand)]
    command: RootCommands,
}

#[derive(Subcommand, Debug)]
enum RootCommands {
    /// Run the lnurl load balancer service
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

    if let RootCommands::Service { .. } = args.command {
        match args.log_level {
            None => {
                if let Err(e) = env_logger::try_init() {
                    eprintln!("failed to initialize env_logger: {e}");
                    return ExitCode::FAILURE;
                }
            }
            Some(level) => {
                if let Err(e) = env_logger::builder().filter_level(level).try_init() {
                    eprintln!("failed to initialize env_logger: {e}");
                    return ExitCode::FAILURE;
                }
            }
        }
    } else {
        let level = args.log_level.unwrap_or(LevelFilter::Info);
        if let Err(e) = TermLogger::init(
            level,
            ConfigBuilder::new()
                .set_time_level(LevelFilter::Off)
                .build(),
            TerminalMode::Stderr,
            ColorChoice::Auto,
        ) {
            eprintln!("failed to initialize TermLogger: {e}");
            return ExitCode::FAILURE;
        }
    }

    match _main(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e:?}");
            ExitCode::FAILURE
        }
    }
}

async fn _main(args: CliArgs) -> anyhow::Result<()> {
    match args.command {
        RootCommands::Service { config, enablement } => {
            commands::services::execute(config, enablement).await
        }
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
                OfferRecordManagementCommands::New { output } => {
                    commands::offer::record::new_offer(output.as_deref())
                }
                OfferRecordManagementCommands::Get {
                    partition,
                    id,
                    output,
                    client,
                } => {
                    commands::offer::record::get_offer(
                        &partition,
                        id.as_ref(),
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
                OfferMetadataManagementCommands::New { output } => {
                    commands::offer::metadata::new_metadata(output.as_deref())
                }
                OfferMetadataManagementCommands::Get {
                    partition,
                    id,
                    output,
                    client,
                } => {
                    commands::offer::metadata::get_metadata(
                        &partition,
                        id.as_ref(),
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
                DiscoveryBackendManagementCommands::New { node_type, output } => {
                    commands::discovery::backend::new_backend(node_type, output.as_deref())
                }
                DiscoveryBackendManagementCommands::List { partition, client } => {
                    commands::discovery::backend::list_backends(&partition, &client).await
                }
                DiscoveryBackendManagementCommands::Get {
                    partition,
                    address,
                    output,
                    client,
                } => {
                    commands::discovery::backend::get_backend(
                        &partition,
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
                    partition,
                    address,
                    input,
                    client,
                } => {
                    commands::discovery::backend::put_backend(
                        &partition,
                        &address,
                        input.as_deref(),
                        &client,
                    )
                    .await
                }
                DiscoveryBackendManagementCommands::Delete {
                    partition,
                    address,
                    client,
                } => {
                    commands::discovery::backend::delete_backend(&partition, &address, &client)
                        .await
                }
            },
        },
    }
}
