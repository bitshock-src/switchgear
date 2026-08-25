pub mod auth;
pub mod error;
pub mod handler;
pub mod service;
pub mod state;

pub type Json<T> = crate::axum::extract::json::Json<T, error::OfferCrudError>;
pub type Query<T> = crate::axum::extract::query::Query<T, error::OfferCrudError>;
pub type UuidParam = crate::axum::extract::uuid::UuidParam<error::OfferCrudError>;
