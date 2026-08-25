use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::response::IntoResponse;
use serde::de::DeserializeOwned;
use std::marker::PhantomData;
use switchgear_error::{ContextError, ErrorOrigin, ForeignContext, IntoContextError};

pub struct Json<T, E> {
    pub value: T,
    _marker: PhantomData<E>,
}

impl<S, T, E> FromRequest<S> for Json<T, E>
where
    S: Send + Sync,
    T: DeserializeOwned,
    E: ContextError + IntoContextError<JsonRejection> + IntoResponse,
{
    type Rejection = E;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let axum::Json(value) = axum::Json::<T>::from_request(req, state)
            .await
            .foreign_context("json rejection", ErrorOrigin::Downstream)?;
        Ok(Self {
            value,
            _marker: PhantomData,
        })
    }
}
