use std::task::{Context, Poll};
use tower::{Layer, Service};
use tracing::Dispatch;
use tracing::instrument::{WithDispatch, WithSubscriber};

#[derive(Clone)]
pub struct WithSubscriberLayer {
    dispatch: Dispatch,
}

impl WithSubscriberLayer {
    pub fn new<S>(subscriber: S) -> Self
    where
        S: Into<Dispatch>,
    {
        Self {
            dispatch: subscriber.into(),
        }
    }
}

impl<S> Layer<S> for WithSubscriberLayer {
    type Service = WithSubscriberService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        WithSubscriberService {
            inner,
            dispatch: self.dispatch.clone(),
        }
    }
}

#[derive(Clone)]
pub struct WithSubscriberService<S> {
    inner: S,
    dispatch: Dispatch,
}

impl<S, Req> Service<Req> for WithSubscriberService<S>
where
    S: Service<Req>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = WithDispatch<S::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        tracing::dispatcher::with_default(&self.dispatch, || self.inner.poll_ready(cx))
    }

    fn call(&mut self, req: Req) -> Self::Future {
        self.inner.call(req).with_subscriber(self.dispatch.clone())
    }
}
