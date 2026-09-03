use crate::commands::error::{CliError, CliErrorAccumulator};
use crate::di::inject::injectors::balance::BalancerInjector;
use crate::di::inject::injectors::config::{ServerConfigInjector, ServiceEnablementInjector};
use crate::di::inject::injectors::service::balance::BalancerServiceInjector;
use crate::di::inject::injectors::service::balance_background::BackgroundBalancerServiceInjector;
use crate::di::inject::injectors::service::discovery::DiscoveryServiceInjector;
use crate::di::inject::injectors::service::offer::OfferServiceInjector;
use crate::di::inject::injectors::store::discovery::DiscoveryStoreInjector;
use crate::di::inject::injectors::store::offer::OfferStoreInjector;
use crate::signals::get_signals_fut;
use clap::ValueEnum;
use signal_hook::low_level::signal_name;
use switchgear_error::{ChainedContext, ErrorOrigin, ForeignContext};
use switchgear_service_api::discovery::DiscoveryBackendStore;
use switchgear_service_api::offer::OfferStore;
use tokio::sync::watch;
use tracing::info;

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
    config_injector: ServerConfigInjector,
    enablement: Vec<ServiceEnablement>,
) -> Result<(), CliError> {
    info!("starting services");

    let (signals_fut, signals_handle) = get_signals_fut()
        .foreign_context("registering OS signal handlers", ErrorOrigin::Internal)?;

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

    let discovery_service_fut = discovery_service_injector
        .connect()
        .await
        .chained_context("connecting discovery service", None)?;
    let discovery_service_fut = async move {
        match discovery_service_fut {
            None => std::future::pending().await,
            Some(f) => f.await,
        }
    };

    let offer_service_fut = offer_service_injector
        .connect()
        .await
        .chained_context("connecting offer service", None)?;
    let offer_service_fut = async move {
        match offer_service_fut {
            None => std::future::pending().await,
            Some(f) => f.await,
        }
    };

    let balancer_service_fut = balancer_service_injector
        .connect()
        .await
        .chained_context("connecting lnurl service", None)?;
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
        .await
        .chained_context("starting background balancer service", None)?;
    let load_balancer_background_handle = tokio::spawn(async move {
        if let Some(f) = background_balancer_service_fut {
            f.await
        } else {
            Ok(())
        }
    });

    let mut errors = CliErrorAccumulator::new();

    let select_result: Result<(), CliError> = tokio::select! {
        lnurl_result = balancer_service_fut => {
            lnurl_result.foreign_context("running lnurl HTTP service", ErrorOrigin::Internal)
        }
        discovery_result = discovery_service_fut => {
            discovery_result.foreign_context("running discovery HTTP service", ErrorOrigin::Internal)
        }
        offers_result = offer_service_fut => {
            offers_result.foreign_context("running offers HTTP service", ErrorOrigin::Internal)
        }
        signal = signals_fut => match signal {
            None => Err(CliError::internal("monitoring OS signals")),
            Some(signal) => {
                let signal_str = signal_name(signal).unwrap_or("unknown");
                info!("received signal: {signal_str}, terminating");
                Ok(())
            }
        }
    };
    errors.push_result(select_result);

    info!("shutting down load balancer background services");
    let _ = load_balancer_background_shutdown_tx.send(true);

    let join_result = load_balancer_background_handle.await.foreign_context(
        "waiting for load balancer background services to terminate",
        ErrorOrigin::Internal,
    );
    let join_result: Result<(), CliError> = match join_result {
        Ok(inner) => inner.foreign_context(
            "load balancer background services returned error",
            ErrorOrigin::Internal,
        ),
        Err(e) => Err(e),
    };
    errors.push_result(join_result);

    info!("load balancer background services shut down");

    info!("disconnecting stores");

    let discovery_disconnect = async {
        match discovery_store_injector.get().await {
            Ok(Some(store)) => store
                .disconnect()
                .await
                .chained_context("disconnecting discovery store", None),
            Ok(None) => Ok(()),
            Err(e) => Err(e).chained_context("resolving discovery store for disconnect", None),
        }
    };
    let offer_disconnect = async {
        match offer_store_injector.get().await {
            Ok(Some(store)) => store
                .disconnect()
                .await
                .chained_context("disconnecting offer store", None),
            Ok(None) => Ok(()),
            Err(e) => Err(e).chained_context("resolving offer store for disconnect", None),
        }
    };

    let (discovery_res, offer_res): (Result<(), CliError>, Result<(), CliError>) =
        tokio::join!(discovery_disconnect, offer_disconnect);
    errors.push_result(discovery_res);
    errors.push_result(offer_res);

    info!("stores disconnected");

    info!("shutting down service tracing");

    let (lnurl_res, discovery_res, offer_res) = tokio::join!(
        balancer_service_injector.shutdown_tracing(),
        discovery_service_injector.shutdown_tracing(),
        offer_service_injector.shutdown_tracing(),
    );
    errors.push_result(lnurl_res.chained_context("shutting down lnurl service tracing", None));
    errors.push_result(
        discovery_res.chained_context("shutting down discovery service tracing", None),
    );
    errors.push_result(offer_res.chained_context("shutting down offer service tracing", None));

    info!("service tracing shut down");

    signals_handle.close();
    info!("signal stream closed");

    let result = errors.finish();
    if result.is_ok() {
        info!("server terminated clean");
    }
    result
}
