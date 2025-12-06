use crate::di::inject::injectors::balance::BalancerInjector;
use crate::di::inject::injectors::config::ServiceEnablementInjector;
use anyhow::anyhow;

use std::future::Future;
use std::pin::Pin;
use switchgear_service_api::balance::LnBalancerBackgroundServices;
use tokio::sync::watch;

pub struct BackgroundBalancerServiceInjector {
    enablement: ServiceEnablementInjector,
    balancer_injector: BalancerInjector,
}

impl BackgroundBalancerServiceInjector {
    pub fn new(enablement: ServiceEnablementInjector, balancer_injector: BalancerInjector) -> Self {
        Self {
            enablement,
            balancer_injector,
        }
    }

    pub async fn start(
        &self,
        shutdown_rx: watch::Receiver<bool>,
    ) -> anyhow::Result<Option<Pin<Box<dyn Future<Output = std::io::Result<()>> + Send>>>> {
        if !self.enablement.lnurl_enabled() {
            return Ok(None);
        }

        let balancer = self
            .balancer_injector
            .get()
            .await?
            .ok_or_else(|| anyhow!("lnurl service enabled but has no balancer"))?;

        let f = async move {
            balancer.start(shutdown_rx).await;
            Ok(())
        };
        Ok(Some(Box::pin(f)))
    }
}
