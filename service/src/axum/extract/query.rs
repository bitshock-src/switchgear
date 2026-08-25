use axum::extract::FromRequestParts;
use axum::extract::rejection::QueryRejection;
use axum::http::request::Parts;
use axum::response::IntoResponse;
use serde::de::DeserializeOwned;
use std::marker::PhantomData;
use switchgear_error::{ContextError, ErrorOrigin, ForeignContext, IntoContextError};

pub struct Query<T, E> {
    pub value: T,
    _marker: PhantomData<E>,
}

impl<S, T, E> FromRequestParts<S> for Query<T, E>
where
    S: Send + Sync,
    T: DeserializeOwned,
    E: ContextError + IntoContextError<QueryRejection> + IntoResponse,
{
    type Rejection = E;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let axum::extract::Query(value) =
            axum::extract::Query::<T>::from_request_parts(parts, state)
                .await
                .foreign_context("query rejection", ErrorOrigin::Downstream)?;
        Ok(Self {
            value,
            _marker: PhantomData,
        })
    }
}
