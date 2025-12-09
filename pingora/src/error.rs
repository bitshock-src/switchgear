use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Display, Formatter};
use switchgear_service_api::service::{HasServiceErrorSource, ServiceErrorSource};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PingoraLnErrorSourceKind {
    #[error("{0}")]
    Error(String),
    #[error("no available lightning nodes")]
    NoAvailableNodes,
    #[error("{0}")]
    ServiceError(Box<dyn std::error::Error + Send + Sync + 'static>),
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

    pub fn from_service_error<
        E: Error + HasServiceErrorSource + Send + Sync + 'static,
        C: Into<Cow<'static, str>>,
    >(
        context: C,
        source: E,
    ) -> Self {
        Self {
            context: context.into(),
            esource: source.get_service_error_source(),
            source: PingoraLnErrorSourceKind::ServiceError(source.into()),
        }
    }

    pub fn general_error<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
        error: String,
    ) -> Self {
        Self {
            context: context.into(),
            source: PingoraLnErrorSourceKind::Error(error),
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
