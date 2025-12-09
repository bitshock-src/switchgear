use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use switchgear_service_api::service::{HasServiceErrorSource, ServiceErrorSource};
use thiserror::Error;

#[derive(Error, Debug)]
pub struct TestError {
    context: Cow<'static, str>,
    #[source]
    source: TestErrorSource,
    esource: ServiceErrorSource,
}

impl Display for TestError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DiscoveryBackendStoreError: while {}: {}",
            self.context.as_ref(),
            self.source
        )
    }
}

#[derive(Error, Debug)]
pub enum TestErrorSource {
    #[error("{0}")]
    Error(String),
}

impl TestError {
    pub fn error<C: Into<Cow<'static, str>>>(
        error: String,
        esource: ServiceErrorSource,
        context: C,
    ) -> Self {
        Self {
            context: context.into(),
            source: TestErrorSource::Error(error),
            esource,
        }
    }
}

impl HasServiceErrorSource for TestError {
    fn get_service_error_source(&self) -> ServiceErrorSource {
        self.esource
    }
}
