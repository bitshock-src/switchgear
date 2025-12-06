use axum::http::header::InvalidHeaderValue;
use axum::http::{HeaderMap, HeaderValue};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use log::error;
use std::fmt::Display;
use switchgear_service_api::service::HasServiceErrorSource;
use thiserror::Error;

#[macro_export]
macro_rules! crud_error_from_service {
    ($error:expr) => {
        CrudError::from_service_error(module_path!(), &format!("{}:{}", file!(), line!()), $error)
    };
}

pub enum WwwAuthenticateError {
    MissingToken,
    InvalidToken,
}

impl Display for WwwAuthenticateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WwwAuthenticateError::MissingToken => write!(f, "missing_token"),
            WwwAuthenticateError::InvalidToken => write!(f, "invalid_token"),
        }
    }
}

#[derive(Debug, Error)]
#[error("{status}")]
pub struct CrudError {
    status: StatusCode,
    headers: HeaderMap,
}

impl CrudError {
    pub fn not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            headers: Default::default(),
        }
    }

    pub fn bad() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            headers: Default::default(),
        }
    }

    pub fn conflict(location: HeaderValue) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            headers: HeaderMap::from_iter(vec![(axum::http::header::LOCATION, location)]),
        }
    }

    pub fn unauthorized(realm: &str, error: WwwAuthenticateError) -> Self {
        let value = match error {
            WwwAuthenticateError::MissingToken => {
                HeaderValue::from_str(&format!(r#"Bearer realm="{realm}""#))
            }
            WwwAuthenticateError::InvalidToken => {
                HeaderValue::from_str(&format!(r#"Bearer realm="{realm}", error="{error}""#))
            }
        };

        let value = value.unwrap_or_else(|_| HeaderValue::from_static("Bearer"));

        Self {
            status: StatusCode::UNAUTHORIZED,
            headers: HeaderMap::from_iter(vec![(axum::http::header::WWW_AUTHENTICATE, value)]),
        }
    }

    pub fn from_service_error<E>(service: &str, location: &str, error: E) -> Self
    where
        E: HasServiceErrorSource + std::fmt::Display,
    {
        error!(target:service, "{error} at: {location}");
        Self {
            status: error.get_service_error_source().to_http_status(),
            headers: Default::default(),
        }
    }
}

impl From<InvalidHeaderValue> for CrudError {
    fn from(_: InvalidHeaderValue) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            headers: Default::default(),
        }
    }
}

impl IntoResponse for CrudError {
    fn into_response(self) -> Response {
        (self.headers, self.status).into_response()
    }
}
