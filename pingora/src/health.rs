use crate::PingoraLnClientPool;
use async_trait::async_trait;
use pingora_error::ErrorType;
use pingora_load_balancing::health_check::HealthCheck;
use pingora_load_balancing::Backend;

pub struct PingoraLnHealthCheck<P> {
    pool: P,
    consecutive_success: usize,
    consecutive_failure: usize,
}

impl<P> PingoraLnHealthCheck<P> {
    pub fn new(pool: P, consecutive_success: usize, consecutive_failure: usize) -> Self {
        Self {
            pool,
            consecutive_success,
            consecutive_failure,
        }
    }
}

#[async_trait]
impl<P> HealthCheck for PingoraLnHealthCheck<P>
where
    P: PingoraLnClientPool<Key = Backend> + Send + Sync,
    P::Error: switchgear_service_api::service::HasServiceErrorSource,
{
    async fn check(&self, target: &Backend) -> pingora_error::Result<()> {
        let metrics = self.pool.get_metrics(target).await.map_err(|e| {
            pingora_error::Error::because(
                ErrorType::InternalError,
                format!("health health for backend {target:?}"),
                e,
            )
        })?;

        if metrics.healthy {
            Ok(())
        } else {
            Err(pingora_error::Error::new(ErrorType::ConnectError))
        }
    }

    fn health_threshold(&self, success: bool) -> usize {
        if success {
            self.consecutive_success
        } else {
            self.consecutive_failure
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::PingoraLnError;
    use crate::PingoraLnMetrics;
    use pingora_core::protocols::l4::socket::SocketAddr;
    use std::net::SocketAddr as StdSocketAddr;
    use switchgear_service_api::discovery::DiscoveryBackend;
    use switchgear_service_api::offer::Offer;
    use switchgear_service_api::service::ServiceErrorSource;

    struct MockPingoraLnClientPool {
        should_be_healthy: bool,
        return_error: bool,
    }

    #[async_trait]
    impl PingoraLnClientPool for MockPingoraLnClientPool {
        type Error = PingoraLnError;
        type Key = Backend;

        async fn get_invoice(
            &self,
            _offer: &Offer,
            _key: &Self::Key,
            _amount_msat: Option<u64>,
            _expiry_secs: Option<u64>,
        ) -> Result<String, Self::Error> {
            unimplemented!("get_invoice is not used in health check tests")
        }

        async fn get_metrics(&self, _key: &Self::Key) -> Result<PingoraLnMetrics, Self::Error> {
            if self.return_error {
                Err(PingoraLnError::general_error(
                    ServiceErrorSource::Upstream,
                    "get_metrics",
                    "forced error".to_string(),
                ))
            } else {
                Ok(PingoraLnMetrics {
                    healthy: self.should_be_healthy,
                    node_effective_inbound_msat: 0,
                })
            }
        }

        fn connect(&self, _key: Self::Key, _backend: &DiscoveryBackend) -> Result<(), Self::Error> {
            unimplemented!("connect is not used in health check tests")
        }
    }

    fn create_mock_backend() -> Backend {
        let std_addr: StdSocketAddr = "127.0.0.1:8080".parse().unwrap();
        Backend {
            addr: SocketAddr::Inet(std_addr),
            weight: 0,
            ext: Default::default(),
        }
    }

    #[tokio::test]
    async fn check_when_node_healthy_then_returns_ok() {
        let mock_pool = MockPingoraLnClientPool {
            should_be_healthy: true,
            return_error: false,
        };

        let health_check = PingoraLnHealthCheck::new(mock_pool, 5, 5);
        let backend = create_mock_backend();
        let result = health_check.check(&backend).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn check_when_node_unhealthy_then_returns_connect_error() {
        let mock_pool = MockPingoraLnClientPool {
            should_be_healthy: false,
            return_error: false,
        };
        let health_check = PingoraLnHealthCheck::new(mock_pool, 5, 5);
        let backend = create_mock_backend();

        let result = health_check.check(&backend).await;

        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.as_ref().etype(), &ErrorType::ConnectError);
        }
    }

    #[tokio::test]
    async fn check_when_pool_error_then_returns_internal_error() {
        let mock_pool = MockPingoraLnClientPool {
            should_be_healthy: true,
            return_error: true,
        };
        let health_check = PingoraLnHealthCheck::new(mock_pool, 5, 5);
        let backend = create_mock_backend();

        let result = health_check.check(&backend).await;

        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.etype, pingora_error::ErrorType::InternalError);
        }
    }

    #[test]
    fn health_threshold_when_called_then_returns_five() {
        let mock_pool = MockPingoraLnClientPool {
            should_be_healthy: true,
            return_error: true,
        };
        let health_check = PingoraLnHealthCheck::new(mock_pool, 5, 5);

        assert_eq!(health_check.health_threshold(true), 5);
        assert_eq!(health_check.health_threshold(false), 5);
    }
}
