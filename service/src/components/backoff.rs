use backoff::backoff::{Backoff, Stop};
use backoff::{ExponentialBackoff, ExponentialBackoffBuilder};
use std::sync::Arc;
use std::time::Duration;

pub trait BackoffProvider: Clone + Send + Sync {
    type Item: Backoff + Send;

    fn get_backoff(&self) -> Self::Item;
}

#[derive(Clone)]
pub struct StopBackoffProvider;

impl BackoffProvider for StopBackoffProvider {
    type Item = Stop;

    fn get_backoff(&self) -> Self::Item {
        Stop {}
    }
}

#[derive(Clone)]
pub struct ExponentialBackoffProvider {
    builder: Arc<ExponentialBackoffBuilder>,
}

impl ExponentialBackoffProvider {
    pub fn new(builder: ExponentialBackoffBuilder) -> Self {
        Self {
            builder: Arc::new(builder),
        }
    }
}

impl BackoffProvider for ExponentialBackoffProvider {
    type Item = ExponentialBackoff;

    fn get_backoff(&self) -> Self::Item {
        self.builder.build()
    }
}

pub enum BackoffInstance {
    Stop(Stop),
    Exponential(ExponentialBackoff),
}

impl Backoff for BackoffInstance {
    fn next_backoff(&mut self) -> Option<Duration> {
        match self {
            BackoffInstance::Stop(b) => b.next_backoff(),
            BackoffInstance::Exponential(b) => b.next_backoff(),
        }
    }
}
