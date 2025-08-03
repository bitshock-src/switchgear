use crate::api::service::{HasServiceErrorSource, ServiceErrorSource};
use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LnPoolErrorSourceKind {
    #[error("CLN tonic gRPC error: {0}")]
    ClnTonicError(crate::components::pool::cln::grpc::client::tonic::Status),
    #[error("LND tonic gRPC error: {0}")]
    LndTonicError(crate::components::pool::lnd::grpc::client::tonic::Status),
    #[error("CLN transport connection error: {0}")]
    ClnTransportError(crate::components::pool::cln::grpc::client::tonic::transport::Error),
    #[error("LND transport connection error: {0}")]
    LndTransportError(crate::components::pool::lnd::grpc::client::tonic::transport::Error),
    #[error("LND connection error: {0}")]
    LndConnectError(fedimint_tonic_lnd::ConnectError),
    #[error("Generic Lightning pool operation failed")]
    Generic,
    #[error("invalid configuration for: {0}")]
    InvalidConfiguration(String),
    #[error("invalid credentials for {0}")]
    InvalidCredentials(String),
    #[error("invalid endpoint URI: {0}")]
    InvalidEndpointUri(
        crate::components::pool::cln::grpc::client::tonic::codegen::http::uri::InvalidUri,
    ),
    #[error("operation timed out")]
    Timeout,
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
    pub fn new<C: Into<Cow<'static, str>>>(
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

    pub fn from_cln_invalid_endpoint_uri<C: Into<Cow<'static, str>>>(
        invalid_uri: crate::components::pool::cln::grpc::client::tonic::codegen::http::uri::InvalidUri,
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self::new(
            LnPoolErrorSourceKind::InvalidEndpointUri(invalid_uri),
            esource,
            context.into(),
        )
    }

    pub fn from_cln_tonic_error<C: Into<Cow<'static, str>>>(
        source: crate::components::pool::cln::grpc::client::tonic::Status,
        context: C,
    ) -> Self {
        let esource = Self::from_cln_tonic_code(source.code());
        Self::new(
            LnPoolErrorSourceKind::ClnTonicError(source),
            esource,
            context,
        )
    }

    pub fn from_cln_tonic_error_with_esource<C: Into<Cow<'static, str>>>(
        source: crate::components::pool::cln::grpc::client::tonic::Status,
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self::new(
            LnPoolErrorSourceKind::ClnTonicError(source),
            esource,
            context,
        )
    }

    pub fn from_lnd_tonic_error<C: Into<Cow<'static, str>>>(
        source: crate::components::pool::lnd::grpc::client::tonic::Status,
        context: C,
    ) -> Self {
        let esource = Self::from_lnd_tonic_code(source.code());

        Self::new(
            LnPoolErrorSourceKind::LndTonicError(source),
            esource,
            context,
        )
    }

    pub fn from_lnd_tonic_error_with_esource<C: Into<Cow<'static, str>>>(
        source: crate::components::pool::lnd::grpc::client::tonic::Status,
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self::new(
            LnPoolErrorSourceKind::LndTonicError(source),
            esource,
            context,
        )
    }

    pub fn from_cln_transport_error<C: Into<Cow<'static, str>>>(
        source: crate::components::pool::cln::grpc::client::tonic::transport::Error,
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self::new(
            LnPoolErrorSourceKind::ClnTransportError(source),
            esource,
            context,
        )
    }

    pub fn from_lnd_connect_error<C: Into<Cow<'static, str>>>(
        source: fedimint_tonic_lnd::ConnectError,
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self::new(
            LnPoolErrorSourceKind::LndConnectError(source),
            esource,
            context,
        )
    }

    pub fn from_timeout_error<C: Into<Cow<'static, str>>>(
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self::new(LnPoolErrorSourceKind::Timeout, esource, context)
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

    pub fn from_cln_tonic_code(
        code: crate::components::pool::cln::grpc::client::tonic::Code,
    ) -> ServiceErrorSource {
        match code {
            crate::components::pool::cln::grpc::client::tonic::Code::InvalidArgument
            | crate::components::pool::cln::grpc::client::tonic::Code::OutOfRange => {
                ServiceErrorSource::Downstream
            }

            _ => ServiceErrorSource::Upstream,
        }
    }

    pub fn from_lnd_tonic_code(
        code: crate::components::pool::lnd::grpc::client::tonic::Code,
    ) -> ServiceErrorSource {
        match code {
            crate::components::pool::lnd::grpc::client::tonic::Code::OutOfRange
            | crate::components::pool::lnd::grpc::client::tonic::Code::AlreadyExists => {
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
