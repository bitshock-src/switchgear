use crate::api::service::StatusCode;
use axum::http::{HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};

pub struct JsonCrudResponse<T> {
    body: Option<T>,
    status: StatusCode,
    headers: HeaderMap,
}

impl<T> JsonCrudResponse<T> {
    pub fn ok(body: T, headers: HeaderMap) -> Self {
        Self {
            body: Some(body),
            status: StatusCode::OK,
            headers,
        }
    }

    pub fn created_location(location: HeaderValue) -> Self {
        Self {
            body: None,
            status: StatusCode::CREATED,
            headers: HeaderMap::from_iter(vec![(axum::http::header::LOCATION, location)]),
        }
    }

    pub fn created() -> Self {
        Self {
            body: None,
            status: StatusCode::CREATED,
            headers: Default::default(),
        }
    }

    pub fn no_content() -> Self {
        JsonCrudResponse {
            body: None,
            status: StatusCode::NO_CONTENT,
            headers: Default::default(),
        }
    }

    pub fn not_modified(headers: HeaderMap) -> Self {
        JsonCrudResponse {
            body: None,
            status: StatusCode::NOT_MODIFIED,
            headers,
        }
    }
}

impl<T> IntoResponse for JsonCrudResponse<T>
where
    axum::Json<T>: IntoResponse,
{
    fn into_response(self) -> Response {
        match self.body {
            None => (self.status, self.headers).into_response(),
            Some(body) => (self.status, self.headers, axum::Json(body)).into_response(),
        }
    }
}
