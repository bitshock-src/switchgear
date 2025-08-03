use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use switchgear_service::api::service::{HasServiceErrorSource, ServiceErrorSource};
use switchgear_service::components::discovery::error::DiscoveryBackendStoreError;
use switchgear_service::components::pool::error::LnPoolError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PingoraLnErrorSourceKind {
    #[error("{0}")]
    PoolError(LnPoolError),
    #[error("{0}")]
    DiscoveryBackendStoreError(DiscoveryBackendStoreError),
    #[error("no available lightning nodes")]
    NoAvailableNodes,
    #[error("{0}")]
    IoError(std::io::Error),
}

#[derive(Error, Debug)]
pub struct PingoraLnError {
    context: Cow<'static, str>,
    #[source]
    source: PingoraLnErrorSourceKind,
    esource: ServiceErrorSource,
}

impl Display for PingoraLnError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PingoraLnError: while {}: {}",
            self.context.as_ref(),
            self.source
        )
    }
}

impl PingoraLnError {
    pub fn new<C: Into<Cow<'static, str>>>(
        source: PingoraLnErrorSourceKind,
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self {
            context: context.into(),
            source,
            esource,
        }
    }

    pub fn from_pool_err<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
        error: LnPoolError,
    ) -> Self {
        Self {
            context: context.into(),
            source: PingoraLnErrorSourceKind::PoolError(error),
            esource,
        }
    }

    pub fn from_discovery_backend_store_err<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
        error: DiscoveryBackendStoreError,
    ) -> Self {
        Self {
            context: context.into(),
            source: PingoraLnErrorSourceKind::DiscoveryBackendStoreError(error),
            esource,
        }
    }

    pub fn no_available_nodes<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self {
            context: context.into(),
            source: PingoraLnErrorSourceKind::NoAvailableNodes,
            esource,
        }
    }

    pub fn from_io_err<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
        error: std::io::Error,
    ) -> Self {
        Self {
            context: context.into(),
            source: PingoraLnErrorSourceKind::IoError(error),
            esource,
        }
    }

    /// Get the context message
    pub fn context(&self) -> &str {
        self.context.as_ref()
    }

    /// Get the error source
    pub fn source(&self) -> &PingoraLnErrorSourceKind {
        &self.source
    }

    /// Get the service error source
    pub fn esource(&self) -> ServiceErrorSource {
        self.esource
    }
}

impl HasServiceErrorSource for PingoraLnError {
    fn get_service_error_source(&self) -> ServiceErrorSource {
        self.esource
    }
}
