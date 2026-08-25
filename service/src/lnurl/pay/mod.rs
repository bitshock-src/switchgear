pub mod error;
pub mod handler;
pub mod partitions;
pub mod state;

pub type Query<T> = crate::axum::extract::query::Query<T, error::LnUrlPayServiceError>;
pub type UuidParam = crate::axum::extract::uuid::UuidParam<error::LnUrlPayServiceError>;
