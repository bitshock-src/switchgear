pub use axum::http::StatusCode;
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceErrorSource {
    Upstream,
    Downstream,
    Internal,
}

impl fmt::Display for ServiceErrorSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServiceErrorSource::Upstream => write!(f, "Upstream"),
            ServiceErrorSource::Downstream => write!(f, "Downstream"),
            ServiceErrorSource::Internal => write!(f, "Internal"),
        }
    }
}

impl Error for ServiceErrorSource {}

impl ServiceErrorSource {
    pub fn to_http_status(&self) -> StatusCode {
        match self {
            ServiceErrorSource::Downstream => StatusCode::BAD_REQUEST,
            ServiceErrorSource::Upstream => StatusCode::BAD_GATEWAY,
            ServiceErrorSource::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

pub trait HasServiceErrorSource {
    fn get_service_error_source(&self) -> ServiceErrorSource;
}
