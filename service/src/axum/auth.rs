use crate::axum::crud::error::{CrudError, WwwAuthenticateError};
use axum::{
    extract::{FromRequestParts, Request},
    response::{IntoResponse, Response},
};
use axum_extra::headers::{authorization::Bearer, Authorization};
use axum_extra::TypedHeader;
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{Layer, Service};

pub trait BearerTokenValidator {
    fn validate(&self, token: &str) -> bool;
}

#[derive(Clone)]
pub struct BearerTokenAuthLayer<V>
where
    V: Clone,
{
    token_validator: V,
    realm: String,
}

impl<V> BearerTokenAuthLayer<V>
where
    V: Clone,
{
    pub fn new(token_validator: V, realm: &str) -> Self {
        Self {
            token_validator,
            realm: realm.to_string(),
        }
    }
}

impl<S, V> Layer<S> for BearerTokenAuthLayer<V>
where
    V: Clone,
{
    type Service = BearerTokenAuthService<S, V>;

    fn layer(&self, inner: S) -> Self::Service {
        BearerTokenAuthService {
            inner,
            token_validator: self.token_validator.clone(),
            realm: self.realm.clone(),
        }
    }
}

#[derive(Clone)]
pub struct BearerTokenAuthService<S, V>
where
    V: Clone,
{
    inner: S,
    token_validator: V,
    realm: String,
}

impl<S, V> Service<Request> for BearerTokenAuthService<S, V>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Infallible>,
    V: BearerTokenValidator + Clone + Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let token_validator = self.token_validator.clone();
        let realm = self.realm.clone();

        let not_ready_inner = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, not_ready_inner);

        Box::pin(async move {
            let (mut parts, body) = req.into_parts();

            match TypedHeader::<Authorization<Bearer>>::from_request_parts(&mut parts, &()).await {
                Ok(TypedHeader(auth)) => {
                    if token_validator.validate(auth.token()) {
                        let req = Request::from_parts(parts, body);
                        inner.call(req).await
                    } else {
                        let error_response =
                            CrudError::unauthorized(&realm, WwwAuthenticateError::InvalidToken);
                        Ok(error_response.into_response())
                    }
                }
                Err(_) => {
                    let error_response =
                        CrudError::unauthorized(&realm, WwwAuthenticateError::MissingToken);
                    Ok(error_response.into_response())
                }
            }
        })
    }
}
