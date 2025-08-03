use crate::api::lnurl::{LnUrlError, LnUrlErrorStatus};
use crate::api::service::HasServiceErrorSource;
use axum::http::header::InvalidHeaderValue;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use log::error;
use thiserror::Error;

#[macro_export]
macro_rules! lnurl_pay_error_from_service {
    ($error:expr) => {
        LnUrlPayServiceError::from_service_error(
            module_path!(),
            &format!("{}:{}", file!(), line!()),
            $error,
        )
    };
}

#[derive(Debug, Error, Clone)]
#[error("{status}: {message}")]
pub struct LnUrlPayServiceError {
    status: StatusCode,
    message: String,
}

impl LnUrlPayServiceError {
    pub fn not_found<E>(error: E) -> Self
    where
        E: std::fmt::Display,
    {
        Self {
            status: StatusCode::NOT_FOUND,
            message: error.to_string(),
        }
    }

    pub fn bad_request<E>(error: E) -> Self
    where
        E: std::fmt::Display,
    {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: error.to_string(),
        }
    }

    pub fn internal_error<E>(service: &str, location: &str, error: E) -> Self
    where
        E: std::fmt::Display,
    {
        error!(target:service, "{error} at: {location}");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }

    pub fn from_service_error<E>(service: &str, location: &str, error: E) -> Self
    where
        E: HasServiceErrorSource + std::fmt::Display,
    {
        error!(target:service, "{error} at: {location}");
        Self {
            status: error.get_service_error_source().to_http_status(),
            message: error.to_string(),
        }
    }
}

impl From<InvalidHeaderValue> for LnUrlPayServiceError {
    fn from(error: InvalidHeaderValue) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for LnUrlPayServiceError {
    fn into_response(self) -> Response {
        let reason = if self.status.is_server_error() {
            "internal server error".to_string()
        } else {
            self.message
        };
        let body = LnUrlError {
            status: LnUrlErrorStatus::Error,
            reason,
        };

        (self.status, axum::Json(body)).into_response()
    }
}
