use crate::api::offer::{OfferMetadataStore, OfferStore};
use jsonwebtoken::DecodingKey;

#[derive(Clone)]
pub struct OfferState<S, M> {
    offer_store: S,
    metadata_store: M,
    auth_authority: DecodingKey,
}

impl<S, M> OfferState<S, M>
where
    S: OfferStore,
    M: OfferMetadataStore,
{
    pub fn new(offer_store: S, metadata_store: M, auth_authority: DecodingKey) -> Self {
        Self {
            offer_store,
            metadata_store,
            auth_authority,
        }
    }

    pub fn offer_store(&self) -> &S {
        &self.offer_store
    }

    pub fn metadata_store(&self) -> &M {
        &self.metadata_store
    }

    pub fn auth_authority(&self) -> &DecodingKey {
        &self.auth_authority
    }
}
