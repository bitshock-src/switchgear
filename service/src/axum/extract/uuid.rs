use axum::extract::FromRequestParts;
use axum::extract::rejection::PathRejection;
use axum::http::request::Parts;
use axum::response::IntoResponse;
use std::marker::PhantomData;
use switchgear_error::{ContextError, ErrorOrigin, ForeignContext, IntoContextError};
use uuid::Uuid;

pub struct UuidParam<E> {
    pub partition: String,
    pub id: Uuid,
    _marker: PhantomData<E>,
}

impl<S, E> FromRequestParts<S> for UuidParam<E>
where
    S: Send + Sync,
    E: ContextError
        + IntoContextError<PathRejection>
        + IntoContextError<uuid::Error>
        + IntoResponse,
{
    type Rejection = E;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let axum::extract::Path((partition, id_str)): axum::extract::Path<(String, String)> =
            axum::extract::Path::from_request_parts(parts, state)
                .await
                .foreign_context("path rejection", ErrorOrigin::Downstream)?;
        let id = id_str
            .parse::<Uuid>()
            .foreign_context("uuid parse rejection", ErrorOrigin::Downstream)?;
        Ok(Self {
            partition,
            id,
            _marker: PhantomData,
        })
    }
}
