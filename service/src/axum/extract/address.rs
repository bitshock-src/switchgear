use crate::api::discovery::DiscoveryBackendAddress;
use axum::{extract::FromRequestParts, extract::Path, http::request::Parts, http::StatusCode};

#[derive(Debug, Clone)]
pub struct DiscoveryBackendAddressParam {
    pub address: DiscoveryBackendAddress,
}

impl<S> FromRequestParts<S> for DiscoveryBackendAddressParam
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path((variant, value)): Path<(String, String)> = Path::from_request_parts(parts, state)
            .await
            .map_err(|_| StatusCode::NOT_FOUND)?;

        let raw_addr = DiscoveryBackendAddress::try_from((variant, value))
            .map_err(|_| StatusCode::NOT_FOUND)?;

        Ok(DiscoveryBackendAddressParam { address: raw_addr })
    }
}
