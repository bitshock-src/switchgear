use crate::axum::extract::uuid::UuidParam;
use crate::lnurl::pay::error::LnUrlPayServiceError;
use axum::{
    extract::{FromRequestParts, Request},
    response::{IntoResponse, Response},
};
use std::collections::HashSet;
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::{Layer, Service};

#[derive(Clone)]
pub struct PartitionsLayer {
    partitions: Arc<HashSet<String>>,
}

impl PartitionsLayer {
    pub fn new(partitions: Arc<HashSet<String>>) -> Self {
        Self { partitions }
    }
}

impl<S> Layer<S> for PartitionsLayer {
    type Service = PartitionsService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        PartitionsService {
            inner,
            partitions: self.partitions.clone(),
        }
    }
}

#[derive(Clone)]
pub struct PartitionsService<S> {
    inner: S,
    partitions: Arc<HashSet<String>>,
}

impl<S> Service<Request> for PartitionsService<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Infallible>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let not_ready_inner = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, not_ready_inner);

        let partitions = self.partitions.clone();

        Box::pin(async move {
            let (mut parts, body) = req.into_parts();

            let uuid_param = match UuidParam::from_request_parts(&mut parts, &()).await {
                Ok(param) => param,
                Err(status) => {
                    return Ok(status.into_response());
                }
            };

            if partitions.contains(&uuid_param.partition) {
                let req = Request::from_parts(parts, body);
                inner.call(req).await
            } else {
                let error_response =
                    LnUrlPayServiceError::not_found(format!("offer not found: {}", &uuid_param.id));
                Ok(error_response.into_response())
            }
        })
    }
}
