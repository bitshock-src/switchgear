use crate::di::inject::injectors::balance::BalancerInjector;
use crate::di::inject::injectors::config::{ServerConfigInjector, ServiceEnablementInjector};
use crate::di::inject::injectors::service::balance::BalancerServiceInjector;
use crate::di::inject::injectors::service::balance_background::BackgroundBalancerServiceInjector;
use crate::di::inject::injectors::service::discovery::DiscoveryServiceInjector;
use crate::di::inject::injectors::service::offer::OfferServiceInjector;
use crate::di::inject::injectors::store::discovery::DiscoveryStoreInjector;
use crate::di::inject::injectors::store::offer::OfferStoreInjector;
use crate::signals::get_signals_fut;
use anyhow::{anyhow, Context};
use clap::ValueEnum;
use log::info;
use signal_hook::low_level::signal_name;
use std::path::PathBuf;
use tokio::sync::watch;

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq, Hash)]
#[clap(rename_all = "kebab-case")]
pub enum ServiceEnablement {
    All,
    Discovery,
    Offer,
    #[clap(rename_all = "lowercase")]
    LnUrl,
}

pub async fn execute(
    config_path: PathBuf,
    enablement: Vec<ServiceEnablement>,
) -> anyhow::Result<()> {
    info!("starting services");

    let (signals_fut, signals_handle) = get_signals_fut()?;

    let config_injector = ServerConfigInjector::new(config_path)?;
    let enablement_injector = ServiceEnablementInjector::new(enablement);

    let discovery_store_injector = DiscoveryStoreInjector::new(config_injector.clone());

    let offer_store_injector = OfferStoreInjector::new(config_injector.clone());

    let discovery_service_injector = DiscoveryServiceInjector::new(
        config_injector.clone(),
        enablement_injector.clone(),
        discovery_store_injector.clone(),
    );

    let offer_service_injector = OfferServiceInjector::new(
        config_injector.clone(),
        enablement_injector.clone(),
        offer_store_injector.clone(),
    );

    let balancer_injector = BalancerInjector::new(
        config_injector.clone(),
        enablement_injector.clone(),
        discovery_store_injector.clone(),
    );

    let balancer_service_injector = BalancerServiceInjector::new(
        config_injector.clone(),
        enablement_injector.clone(),
        balancer_injector.clone(),
        offer_store_injector.clone(),
    );

    let background_balancer_service_injector = BackgroundBalancerServiceInjector::new(
        enablement_injector.clone(),
        balancer_injector.clone(),
    );

    let discovery_service_fut = discovery_service_injector.connect().await?;
    let discovery_service_fut = async move {
        match discovery_service_fut {
            None => std::future::pending().await,
            Some(f) => f.await,
        }
    };

    let offer_service_fut = offer_service_injector.connect().await?;
    let offer_service_fut = async move {
        match offer_service_fut {
            None => std::future::pending().await,
            Some(f) => f.await,
        }
    };

    let balancer_service_fut = balancer_service_injector.connect().await?;
    let balancer_service_fut = async move {
        match balancer_service_fut {
            None => std::future::pending().await,
            Some(f) => f.await,
        }
    };

    let (load_balancer_background_shutdown_tx, load_balancer_background_shutdown_rx) =
        watch::channel(false);
    let background_balancer_service_fut = background_balancer_service_injector
        .start(load_balancer_background_shutdown_rx)
        .await?;
    let load_balancer_background_handle = tokio::spawn(async move {
        if let Some(f) = background_balancer_service_fut {
            f.await
        } else {
            Ok(())
        }
    });

    let mut errors = vec![];

    if let Err(e) = tokio::select! {
        lnurl_result = balancer_service_fut => {
            lnurl_result.with_context(|| "running lnurl HTTP service")
        }

        discovery_result = discovery_service_fut => {
            discovery_result.with_context(|| "running discovery HTTP service")
        }

        offers_result = offer_service_fut => {
            offers_result.with_context(|| "running offers HTTP service")
        }

        signal = signals_fut => match signal {
            None => {
                Err(anyhow!("monitoring OS signals"))
            }
            Some(signal) => {
                let signal_str = signal_name(signal).unwrap_or("unknown");
                info!("received signal: {signal_str}, terminating");
                Ok(())
            }
        }
    } {
        errors.push(e);
    }

    // Shutdown background service
    info!("shutting down load balancer background services");
    let _ = load_balancer_background_shutdown_tx.send(true);

    if let Err(e) = load_balancer_background_handle
        .await
        .with_context(|| "waiting for load balancer background services to terminate")
    {
        errors.push(e);
    }

    info!("load balancer background services shut down");

    signals_handle.close();
    info!("signal stream closed");

    if errors.is_empty() {
        info!("server terminated clean");
        Ok(())
    } else {
        Err(anyhow!("server terminated with errors:\n{:?}", errors))
    }
}
