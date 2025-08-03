use crate::config::{BackendSelectionConfig, BackoffConfig, LnUrlBalancerServiceConfig};
use crate::di::delegates::{BackoffProviderDelegate, LnBalancerDelegate};
use crate::di::inject::injectors::config::{ServerConfigInjector, ServiceEnablementInjector};
use crate::di::inject::injectors::store::discovery::DiscoveryStoreInjector;
use anyhow::anyhow;
use pingora_load_balancing::discovery::ServiceDiscovery;
use pingora_load_balancing::health_check::HealthCheck;
use pingora_load_balancing::{Backends, LoadBalancer};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use switchgear_pingora::balance::{
    ConsistentMaxIterations, PingoraLnBalancer, RandomMaxIterations, RoundRobinMaxIterations,
};
use switchgear_pingora::discovery::DefaultPingoraLnDiscovery;
use switchgear_pingora::health::PingoraLnHealthCheck;
use switchgear_service::components::backoff::{ExponentialBackoffProvider, StopBackoffProvider};
use switchgear_service::components::pool::default_pool::DefaultLnClientPool;

#[derive(Clone)]
pub struct BalancerInjector {
    config: ServerConfigInjector,
    enablement: ServiceEnablementInjector,
    discovery: DiscoveryStoreInjector,
    singleton: Rc<RefCell<Option<Option<LnBalancerDelegate>>>>,
}

impl BalancerInjector {
    pub fn new(
        config: ServerConfigInjector,
        enablement: ServiceEnablementInjector,
        discovery: DiscoveryStoreInjector,
    ) -> Self {
        Self {
            config,
            enablement,
            discovery,
            singleton: Default::default(),
        }
    }

    pub async fn get(&self) -> anyhow::Result<Option<LnBalancerDelegate>> {
        if let Some(b) = self.singleton.borrow().as_ref() {
            return Ok(b.clone());
        }
        self.inject().await
    }

    async fn inject(&self) -> anyhow::Result<Option<LnBalancerDelegate>> {
        if !self.enablement.lnurl_enabled() {
            *self.singleton.borrow_mut() = Some(None);
            return Ok(None);
        }

        let lnurl_config = self
            .config
            .get()
            .lnurl_service
            .as_ref()
            .ok_or_else(|| anyhow!("lnurl service enabled but has no config"))?;

        let discovery = self
            .discovery
            .get()
            .await?
            .ok_or_else(|| anyhow!("lnurl service enabled but has no discovery store"))?;

        let backoff = match lnurl_config.backoff {
            BackoffConfig::Stop => BackoffProviderDelegate::Stop(StopBackoffProvider),
            BackoffConfig::Exponential {
                initial_interval_secs,
                randomization_factor,
                multiplier,
                max_interval_secs,
                max_elapsed_time_secs,
            } => {
                let mut builder = backoff::ExponentialBackoffBuilder::new();
                if let Some(interval) = initial_interval_secs {
                    builder.with_initial_interval(Duration::from_secs_f64(interval));
                }
                if let Some(factor) = randomization_factor {
                    builder.with_randomization_factor(factor);
                }
                if let Some(mult) = multiplier {
                    builder.with_multiplier(mult);
                }
                if let Some(max_interval) = max_interval_secs {
                    builder.with_max_interval(Duration::from_secs_f64(max_interval));
                }
                builder.with_max_elapsed_time(max_elapsed_time_secs.map(Duration::from_secs_f64));

                BackoffProviderDelegate::Exponential(ExponentialBackoffProvider::new(builder))
            }
        };

        let pool =
            DefaultLnClientPool::new(Duration::from_secs_f64(lnurl_config.ln_client_timeout_secs));

        let discovery = DefaultPingoraLnDiscovery::new(
            discovery,
            pool.clone(),
            lnurl_config.partitions.clone(),
        );

        let health = PingoraLnHealthCheck::new(
            pool.clone(),
            lnurl_config.health_check_consecutive_success_to_healthy,
            lnurl_config.health_check_consecutive_failure_to_unhealthy,
        );

        let balancer = match lnurl_config.backend_selection {
            BackendSelectionConfig::RoundRobin => {
                let balancer = Arc::new(Self::create_pingora_load_balancer(
                    lnurl_config,
                    discovery,
                    health,
                ));
                LnBalancerDelegate::RoundRobin(PingoraLnBalancer::new(
                    balancer.clone(),
                    pool.clone(),
                    pool,
                    backoff,
                    RoundRobinMaxIterations,
                    lnurl_config.parallel_health_check,
                    lnurl_config.selection_capacity_bias,
                ))
            }
            BackendSelectionConfig::Random => {
                let balancer = Arc::new(Self::create_pingora_load_balancer(
                    lnurl_config,
                    discovery,
                    health,
                ));
                LnBalancerDelegate::Random(PingoraLnBalancer::new(
                    balancer.clone(),
                    pool.clone(),
                    pool,
                    backoff,
                    RandomMaxIterations,
                    lnurl_config.parallel_health_check,
                    lnurl_config.selection_capacity_bias,
                ))
            }
            BackendSelectionConfig::Consistent { max_iterations } => {
                let balancer = Arc::new(Self::create_pingora_load_balancer(
                    lnurl_config,
                    discovery,
                    health,
                ));
                LnBalancerDelegate::Consistent(PingoraLnBalancer::new(
                    balancer.clone(),
                    pool.clone(),
                    pool,
                    backoff,
                    ConsistentMaxIterations::new(max_iterations),
                    lnurl_config.parallel_health_check,
                    lnurl_config.selection_capacity_bias,
                ))
            }
        };

        *self.singleton.borrow_mut() = Some(Some(balancer.clone()));
        Ok(Some(balancer))
    }

    fn create_pingora_load_balancer<D, S, H>(
        lnurl_config: &LnUrlBalancerServiceConfig,
        discovery: D,
        health: H,
    ) -> LoadBalancer<S>
    where
        D: ServiceDiscovery + Send + Sync + 'static,
        S: pingora_load_balancing::selection::BackendSelection + 'static,
        S::Iter: pingora_load_balancing::selection::BackendIter,
        H: HealthCheck + Send + Sync + 'static,
    {
        let mut load_balancer =
            LoadBalancer::<S>::from_backends(Backends::new(Box::new(discovery)));
        load_balancer.set_health_check(Box::new(health));
        load_balancer.health_check_frequency = Some(Duration::from_secs_f64(
            lnurl_config.health_check_frequency_secs,
        ));
        load_balancer.update_frequency = Some(Duration::from_secs_f64(
            lnurl_config.backend_update_frequency_secs,
        ));
        load_balancer.parallel_health_check = lnurl_config.parallel_health_check;

        load_balancer
    }
}
