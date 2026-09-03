use crate::di::error::DiError;
use crate::di::inject::injectors::service::subscriber::{ecs_layer, metrics_env_filter};
use crate::level::Level;
use crate::{CliArgs, RootCommands};
use switchgear_error::ForeignContext;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::{Directive, LevelFilter, filter_fn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const GLOBAL_SERVICE_NAME: &str = "swgr";

pub struct TracingSystemInjector;

impl TracingSystemInjector {
    pub fn init(args: &CliArgs) -> Result<(), DiError> {
        match &args.command {
            RootCommands::Service { .. } => init_subscriber(),
            _ => {
                let level = args.log_level.unwrap_or(Level::Info);
                init_cli_subscriber(level)
            }
        }
    }
}

fn init_subscriber() -> Result<(), DiError> {
    let metrics_target = switchgear_metrics::metrics_target();
    tracing_subscriber::registry()
        .with(metrics_env_filter(LevelFilter::OFF)?)
        .with(
            ecs_layer(GLOBAL_SERVICE_NAME)
                .with_filter(filter_fn(move |m| m.target() != metrics_target)),
        )
        .try_init()
        .foreign_context("initializing tracing subscriber", None)
}

fn init_cli_subscriber(level: Level) -> Result<(), DiError> {
    let directive: Directive = LevelFilter::from(level).into();

    serif::Config::new()
        .with_output(serif::Output::Stderr)
        .with_default(directive)
        .with_timestamp(serif::TimeFormat::none())
        .with_target(false)
        .init();

    Ok(())
}
