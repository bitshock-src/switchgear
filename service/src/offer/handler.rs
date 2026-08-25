use crate::axum::crud::response::JsonCrudResponse;
use crate::axum::header::no_cache_headers;
use crate::offer::error::OfferCrudError;
use crate::offer::state::OfferState;
use crate::offer::{Json, Query, UuidParam};
use axum::extract::State;
use axum::http::HeaderValue;
use serde::Deserialize;
use switchgear_error::{ChainedContext, ForeignContext};
use switchgear_service_api::offer::{
    OfferMetadata, OfferMetadataSparse, OfferMetadataStore, OfferRecord, OfferRecordSparse,
    OfferStore,
};

#[derive(Deserialize, Debug)]
pub struct GetAllOffersQueryParameters {
    pub start: Option<usize>,
    pub count: Option<usize>,
}

#[derive(Deserialize, Debug)]
pub struct GetAllMetadataQueryParameters {
    pub start: Option<usize>,
    pub count: Option<usize>,
}

#[derive(Deserialize, Debug)]
pub struct GetOfferQueryParameters {
    pub sparse: Option<bool>,
}

pub struct OfferHandlers;

impl OfferHandlers {
    #[tracing::instrument(skip_all)]
    pub async fn get_offer<S, M>(
        Query { value: params, .. }: Query<GetOfferQueryParameters>,
        UuidParam { partition, id, .. }: UuidParam,
        State(state): State<OfferState<S, M>>,
    ) -> Result<JsonCrudResponse<OfferRecord>, OfferCrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        let offer = state
            .offer_store()
            .get_offer(&partition, &id, params.sparse)
            .await
            .chained_context("fetching offer", None)?
            .ok_or_else(OfferCrudError::not_found)?;

        let headers = no_cache_headers();

        Ok(JsonCrudResponse::ok(offer, headers))
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_offers<S, M>(
        axum::extract::Path(partition): axum::extract::Path<String>,
        Query { value: params, .. }: Query<GetAllOffersQueryParameters>,
        State(state): State<OfferState<S, M>>,
    ) -> Result<JsonCrudResponse<Vec<OfferRecord>>, OfferCrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        let count = params.count.unwrap_or(state.max_page_size());
        if count > state.max_page_size() {
            return Err(OfferCrudError::bad());
        }
        let offers = state
            .offer_store()
            .get_offers(&partition, params.start.unwrap_or(0), count)
            .await
            .chained_context("listing offers", None)?;

        let headers = no_cache_headers();

        Ok(JsonCrudResponse::ok(offers, headers))
    }

    #[tracing::instrument(skip_all)]
    pub async fn post_offer<S, M>(
        State(state): State<OfferState<S, M>>,
        Json {
            value: mut offer, ..
        }: Json<OfferRecord>,
    ) -> Result<JsonCrudResponse<()>, OfferCrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        offer.offer.metadata = None;
        let location = format!("{}/{}", offer.partition, offer.id);

        let result = state
            .offer_store()
            .post_offer(offer)
            .await
            .chained_context("creating offer", None)?;

        let location = HeaderValue::from_str(&location)
            .foreign_context("building offer location header", None)?;

        match result {
            Some(_) => Ok(JsonCrudResponse::created_location(location)),
            None => Err(OfferCrudError::conflict(location)),
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn put_offer<S, M>(
        State(state): State<OfferState<S, M>>,
        UuidParam { partition, id, .. }: UuidParam,
        Json { value: offer, .. }: Json<OfferRecordSparse>,
    ) -> Result<JsonCrudResponse<()>, OfferCrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        let mut offer = OfferRecord {
            partition,
            id,
            offer,
        };

        offer.offer.metadata = None;

        let was_created = state
            .offer_store()
            .put_offer(offer)
            .await
            .chained_context("updating offer", None)?;

        if was_created {
            Ok(JsonCrudResponse::created())
        } else {
            Ok(JsonCrudResponse::no_content())
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn delete_offer<S, M>(
        UuidParam { partition, id, .. }: UuidParam,
        State(state): State<OfferState<S, M>>,
    ) -> Result<JsonCrudResponse<()>, OfferCrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        let deleted = state
            .offer_store()
            .delete_offer(&partition, &id)
            .await
            .chained_context("deleting offer", None)?;
        if deleted {
            Ok(JsonCrudResponse::no_content())
        } else {
            Err(OfferCrudError::not_found())
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_metadata<S, M>(
        UuidParam { partition, id, .. }: UuidParam,
        State(state): State<OfferState<S, M>>,
    ) -> Result<JsonCrudResponse<OfferMetadata>, OfferCrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        let metadata = state
            .metadata_store()
            .get_metadata(&partition, &id)
            .await
            .chained_context("fetching metadata", None)?
            .ok_or_else(OfferCrudError::not_found)?;

        let headers = no_cache_headers();

        Ok(JsonCrudResponse::ok(metadata, headers))
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_all_metadata<S, M>(
        axum::extract::Path(partition): axum::extract::Path<String>,
        Query { value: params, .. }: Query<GetAllMetadataQueryParameters>,
        State(state): State<OfferState<S, M>>,
    ) -> Result<JsonCrudResponse<Vec<OfferMetadata>>, OfferCrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        let count = params.count.unwrap_or(state.max_page_size());
        if count > state.max_page_size() {
            return Err(OfferCrudError::bad());
        }
        let metadata = state
            .metadata_store()
            .get_all_metadata(&partition, params.start.unwrap_or(0), count)
            .await
            .chained_context("listing metadata", None)?;

        let headers = no_cache_headers();

        Ok(JsonCrudResponse::ok(metadata, headers))
    }

    #[tracing::instrument(skip_all)]
    pub async fn post_metadata<S, M>(
        State(state): State<OfferState<S, M>>,
        Json {
            value: metadata, ..
        }: Json<OfferMetadata>,
    ) -> Result<JsonCrudResponse<()>, OfferCrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        let location = format!("{}/{}", metadata.partition, metadata.id);

        let result = state
            .metadata_store()
            .post_metadata(metadata)
            .await
            .chained_context("creating metadata", None)?;

        let location = HeaderValue::from_str(&location)
            .foreign_context("building metadata location header", None)?;

        match result {
            Some(_) => Ok(JsonCrudResponse::created_location(location)),
            None => Err(OfferCrudError::conflict(location)),
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn put_metadata<S, M>(
        State(state): State<OfferState<S, M>>,
        UuidParam { partition, id, .. }: UuidParam,
        Json {
            value: metadata, ..
        }: Json<OfferMetadataSparse>,
    ) -> Result<JsonCrudResponse<()>, OfferCrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        let metadata = OfferMetadata {
            id,
            partition,
            metadata,
        };

        let was_created = state
            .metadata_store()
            .put_metadata(metadata)
            .await
            .chained_context("updating metadata", None)?;

        if was_created {
            Ok(JsonCrudResponse::created())
        } else {
            Ok(JsonCrudResponse::no_content())
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn delete_metadata<S, M>(
        UuidParam { partition, id, .. }: UuidParam,
        State(state): State<OfferState<S, M>>,
    ) -> Result<JsonCrudResponse<()>, OfferCrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        let deleted = state
            .metadata_store()
            .delete_metadata(&partition, &id)
            .await
            .chained_context("deleting metadata", None)?;
        if deleted {
            Ok(JsonCrudResponse::no_content())
        } else {
            Err(OfferCrudError::not_found())
        }
    }
}
