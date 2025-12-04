use crate::api::offer::{
    OfferMetadata, OfferMetadataSparse, OfferMetadataStore, OfferRecord, OfferRecordSparse,
    OfferStore,
};
use crate::axum::crud::error::CrudError;
use crate::axum::crud::response::JsonCrudResponse;
use crate::axum::extract::uuid::UuidParam;
use crate::axum::header::no_cache_headers;
use crate::offer::state::OfferState;
use axum::extract::Query;
use axum::http::HeaderValue;
use axum::{extract::State, Json};
use serde::Deserialize;

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

pub struct OfferHandlers;

impl OfferHandlers {
    pub async fn get_offer<S, M>(
        UuidParam { partition, id }: UuidParam,
        State(state): State<OfferState<S, M>>,
    ) -> Result<JsonCrudResponse<OfferRecord>, CrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        let offer = state
            .offer_store()
            .get_offer(&partition, &id)
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?
            .ok_or(CrudError::not_found())?;

        let headers = no_cache_headers();

        Ok(JsonCrudResponse::ok(offer, headers))
    }

    pub async fn get_offers<S, M>(
        axum::extract::Path(partition): axum::extract::Path<String>,
        Query(params): Query<GetAllOffersQueryParameters>,
        State(state): State<OfferState<S, M>>,
    ) -> Result<JsonCrudResponse<Vec<OfferRecord>>, CrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        let count = params.count.unwrap_or(state.max_page_size());
        if count > state.max_page_size() {
            return Err(CrudError::bad());
        }
        let offers = state
            .offer_store()
            .get_offers(&partition, params.start.unwrap_or(0), count)
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?;

        let headers = no_cache_headers();

        Ok(JsonCrudResponse::ok(offers, headers))
    }

    pub async fn post_offer<S, M>(
        State(state): State<OfferState<S, M>>,
        Json(offer): Json<OfferRecord>,
    ) -> Result<JsonCrudResponse<()>, CrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        let result = state
            .offer_store()
            .post_offer(offer.clone())
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?;

        let location = format!("{}/{}", offer.partition, offer.id);
        let location = HeaderValue::from_str(&location)?;

        match result {
            Some(_) => Ok(JsonCrudResponse::created_location(location)),
            None => Err(CrudError::conflict(location)),
        }
    }

    pub async fn put_offer<S, M>(
        State(state): State<OfferState<S, M>>,
        UuidParam { partition, id }: UuidParam,
        Json(offer): Json<OfferRecordSparse>,
    ) -> Result<JsonCrudResponse<()>, CrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        let offer = OfferRecord {
            partition,
            id,
            offer,
        };

        let was_created = state
            .offer_store()
            .put_offer(offer.clone())
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?;

        if was_created {
            Ok(JsonCrudResponse::created())
        } else {
            Ok(JsonCrudResponse::no_content())
        }
    }

    pub async fn delete_offer<S, M>(
        UuidParam { partition, id }: UuidParam,
        State(state): State<OfferState<S, M>>,
    ) -> Result<JsonCrudResponse<()>, CrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        if state
            .offer_store()
            .delete_offer(&partition, &id)
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?
        {
            Ok(JsonCrudResponse::no_content())
        } else {
            Err(CrudError::not_found())
        }
    }

    pub async fn get_metadata<S, M>(
        UuidParam { partition, id }: UuidParam,
        State(state): State<OfferState<S, M>>,
    ) -> Result<JsonCrudResponse<OfferMetadata>, CrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        let metadata = state
            .metadata_store()
            .get_metadata(&partition, &id)
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?
            .ok_or(CrudError::not_found())?;

        let headers = no_cache_headers();

        Ok(JsonCrudResponse::ok(metadata, headers))
    }

    pub async fn get_all_metadata<S, M>(
        axum::extract::Path(partition): axum::extract::Path<String>,
        Query(params): Query<GetAllMetadataQueryParameters>,
        State(state): State<OfferState<S, M>>,
    ) -> Result<JsonCrudResponse<Vec<OfferMetadata>>, CrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        let count = params.count.unwrap_or(state.max_page_size());
        if count > state.max_page_size() {
            return Err(CrudError::bad());
        }
        let metadata = state
            .metadata_store()
            .get_all_metadata(&partition, params.start.unwrap_or(0), count)
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?;

        let headers = no_cache_headers();

        Ok(JsonCrudResponse::ok(metadata, headers))
    }

    pub async fn post_metadata<S, M>(
        State(state): State<OfferState<S, M>>,
        Json(metadata): Json<OfferMetadata>,
    ) -> Result<JsonCrudResponse<()>, CrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        let result = state
            .metadata_store()
            .post_metadata(metadata.clone())
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?;

        let location = format!("{}/{}", metadata.partition, metadata.id);
        let location = HeaderValue::from_str(&location)?;

        match result {
            Some(_) => Ok(JsonCrudResponse::created_location(location)),
            None => Err(CrudError::conflict(location)),
        }
    }

    pub async fn put_metadata<S, M>(
        State(state): State<OfferState<S, M>>,
        UuidParam { partition, id }: UuidParam,
        Json(metadata): Json<OfferMetadataSparse>,
    ) -> Result<JsonCrudResponse<()>, CrudError>
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
            .put_metadata(metadata.clone())
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?;

        if was_created {
            Ok(JsonCrudResponse::created())
        } else {
            Ok(JsonCrudResponse::no_content())
        }
    }

    pub async fn delete_metadata<S, M>(
        UuidParam { partition, id }: UuidParam,
        State(state): State<OfferState<S, M>>,
    ) -> Result<JsonCrudResponse<()>, CrudError>
    where
        S: OfferStore,
        M: OfferMetadataStore,
    {
        if state
            .metadata_store()
            .delete_metadata(&partition, &id)
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?
        {
            Ok(JsonCrudResponse::no_content())
        } else {
            Err(CrudError::not_found())
        }
    }
}
