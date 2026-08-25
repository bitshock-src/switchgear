use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::panic::Location;
use switchgear_error::{ContextError, ErrorOrigin, IntoBoxedTrait, IntoContextError};
use switchgear_service_api::balance::LnBalancerError;
use switchgear_service_api::discovery::DiscoveryBackendStoreError;
use switchgear_service_api::offer::OfferStoreError;

pub struct TestError {
    context: Cow<'static, str>,
    source: TestErrorSource,
    esource: ErrorOrigin,
    location: &'static Location<'static>,
}

impl Debug for TestError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestError")
            .field("context", &self.context)
            .field("source", &self.source)
            .field("esource", &self.esource)
            .finish()
    }
}

impl Display for TestError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "TestError: while {}: {}",
            self.context.as_ref(),
            self.source
        )
    }
}

impl Error for TestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}

#[derive(Debug)]
pub enum TestErrorSource {
    Error(String),
}

impl Display for TestErrorSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error(s) => write!(f, "{s}"),
        }
    }
}

impl Error for TestErrorSource {}

impl TestError {
    #[track_caller]
    pub(crate) fn error<C: Into<Cow<'static, str>>>(
        error: String,
        esource: ErrorOrigin,
        context: C,
    ) -> Self {
        Self {
            context: context.into(),
            source: TestErrorSource::Error(error),
            esource,
            location: Location::caller(),
        }
    }
}

impl ContextError for TestError {
    fn origin(&self) -> ErrorOrigin {
        self.esource
    }

    fn location(&self) -> &'static Location<'static> {
        self.location
    }

    fn source_context(&self) -> Option<&dyn ContextError> {
        None
    }
}

impl OfferStoreError for TestError {}
impl DiscoveryBackendStoreError for TestError {}
impl LnBalancerError for TestError {}

impl IntoBoxedTrait<dyn OfferStoreError> for TestError {
    fn into_boxed(self) -> Box<dyn OfferStoreError> {
        Box::new(self)
    }
}
impl IntoBoxedTrait<dyn DiscoveryBackendStoreError> for TestError {
    fn into_boxed(self) -> Box<dyn DiscoveryBackendStoreError> {
        Box::new(self)
    }
}
impl IntoBoxedTrait<dyn LnBalancerError> for TestError {
    fn into_boxed(self) -> Box<dyn LnBalancerError> {
        Box::new(self)
    }
}

impl IntoContextError<serde_json::Error> for TestError {
    #[track_caller]
    fn error<M: Into<Cow<'static, str>>>(
        source: Box<serde_json::Error>,
        message: M,
        origin: Option<ErrorOrigin>,
    ) -> Self {
        Self {
            context: message.into(),
            source: TestErrorSource::Error(source.to_string()),
            esource: origin.unwrap_or(ErrorOrigin::Internal),
            location: Location::caller(),
        }
    }
}
