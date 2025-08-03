use crate::commands::discovery::backend::DiscoveryBackendManagementCommands;
use crate::commands::token::TokenCommands;
use clap::Subcommand;

pub mod backend;
pub mod token;

#[derive(Subcommand, Debug)]
pub enum DiscoveryCommands {
    /// Manage discovery service token
    #[clap(subcommand)]
    Token(TokenCommands),

    #[clap(flatten)]
    Backend(DiscoveryBackendManagementCommands),
}
