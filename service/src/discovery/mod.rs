pub mod auth;
pub mod error;
pub mod handler;
pub mod service;
pub mod state;

pub type Json<T> = crate::axum::extract::json::Json<T, error::DiscoveryCrudError>;
