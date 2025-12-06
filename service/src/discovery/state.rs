use jsonwebtoken::DecodingKey;
use switchgear_service_api::discovery::DiscoveryBackendStore;

#[derive(Clone)]
pub struct DiscoveryState<S> {
    store: S,
    auth_authority: DecodingKey,
}

impl<S> DiscoveryState<S>
where
    S: DiscoveryBackendStore,
{
    pub fn new(store: S, auth_authority: DecodingKey) -> Self {
        Self {
            store,
            auth_authority,
        }
    }

    pub fn store(&self) -> &S {
        &self.store
    }

    pub fn auth_authority(&self) -> &DecodingKey {
        &self.auth_authority
    }
}
