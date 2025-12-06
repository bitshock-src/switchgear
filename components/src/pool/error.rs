use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use switchgear_service_api::service::{HasServiceErrorSource, ServiceErrorSource};
use thiserror::Error;
use tonic::{transport, Code, Status};

#[derive(Error, Debug)]
pub enum LnPoolErrorSourceKind {
    #[error("CLN tonic gRPC error: {0}")]
    TonicError(Status),
    #[error("CLN transport connection error: {0}")]
    TransportError(transport::Error),
    #[error("invalid configuration for: {0}")]
    InvalidConfiguration(String),
    #[error("invalid credentials for {0}")]
    InvalidCredentials(String),
    #[error("memory error: {0}")]
    MemoryError(String),
    #[error("json error: {0}")]
    JsonError(serde_json::Error),
}

#[derive(Error, Debug)]
pub struct LnPoolError {
    context: Cow<'static, str>,
    #[source]
    source: LnPoolErrorSourceKind,
    esource: ServiceErrorSource,
}

impl Display for LnPoolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "LnPoolError: while {}: {}",
            self.context.as_ref(),
            self.source
        )
    }
}

impl LnPoolError {
    fn new<C: Into<Cow<'static, str>>>(
        source: LnPoolErrorSourceKind,
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self {
            context: context.into(),
            source,
            esource,
        }
    }

    pub fn from_invalid_configuration<C: Into<Cow<'static, str>>>(
        source: C,
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self::new(
            LnPoolErrorSourceKind::InvalidConfiguration(source.into().to_string()),
            esource,
            context.into(),
        )
    }

    pub fn from_invalid_credentials<C: Into<Cow<'static, str>>>(
        source: C,
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self::new(
            LnPoolErrorSourceKind::InvalidCredentials(source.into().to_string()),
            esource,
            context.into(),
        )
    }

    pub fn from_tonic_error<C: Into<Cow<'static, str>>>(source: Status, context: C) -> Self {
        let esource = Self::from_tonic_code(source.code());
        Self::new(LnPoolErrorSourceKind::TonicError(source), esource, context)
    }

    pub fn from_transport_error<C: Into<Cow<'static, str>>>(
        source: transport::Error,
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self::new(
            LnPoolErrorSourceKind::TransportError(source),
            esource,
            context,
        )
    }

    pub fn from_memory_error<C: Into<Cow<'static, str>>>(source: String, context: C) -> Self {
        Self::new(
            LnPoolErrorSourceKind::MemoryError(source),
            ServiceErrorSource::Internal,
            context,
        )
    }

    pub fn from_json_error<C: Into<Cow<'static, str>>>(
        source: serde_json::Error,
        context: C,
    ) -> Self {
        Self::new(
            LnPoolErrorSourceKind::JsonError(source),
            ServiceErrorSource::Internal,
            context,
        )
    }

    pub fn context(&self) -> &str {
        self.context.as_ref()
    }

    pub fn source(&self) -> &LnPoolErrorSourceKind {
        &self.source
    }

    pub fn esource(&self) -> ServiceErrorSource {
        self.esource
    }

    fn from_tonic_code(code: Code) -> ServiceErrorSource {
        match code {
            Code::InvalidArgument | Code::OutOfRange | Code::AlreadyExists => {
                ServiceErrorSource::Downstream
            }

            _ => ServiceErrorSource::Upstream,
        }
    }
}

impl HasServiceErrorSource for LnPoolError {
    fn get_service_error_source(&self) -> ServiceErrorSource {
        self.esource
    }
}
