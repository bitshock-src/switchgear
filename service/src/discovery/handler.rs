use crate::axum::crud::error::CrudError;
use crate::axum::crud::response::JsonCrudResponse;
use crate::axum::header::no_cache_headers;
use crate::discovery::state::DiscoveryState;
use axum::extract::Path;
use axum::http::{HeaderMap, HeaderValue};
use axum::{extract::State, Json};
use switchgear_service_api::discovery::{
    DiscoveryBackend, DiscoveryBackendPatch, DiscoveryBackendPatchSparse, DiscoveryBackendSparse,
    DiscoveryBackendStore, DiscoveryBackends,
};

pub struct DiscoveryHandlers;

impl DiscoveryHandlers {
    pub async fn get_backend<S>(
        Path(public_key): Path<String>,
        State(state): State<DiscoveryState<S>>,
    ) -> Result<JsonCrudResponse<DiscoveryBackend>, CrudError>
    where
        S: DiscoveryBackendStore,
    {
        let public_key = public_key.parse().map_err(|_| CrudError::bad())?;

        let backend = state
            .store()
            .get(&public_key)
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?
            .ok_or(CrudError::not_found())?;

        let headers = no_cache_headers();

        Ok(JsonCrudResponse::ok(backend, headers))
    }

    pub async fn get_backends<S>(
        headers: HeaderMap,
        State(state): State<DiscoveryState<S>>,
    ) -> Result<JsonCrudResponse<Vec<DiscoveryBackend>>, CrudError>
    where
        S: DiscoveryBackendStore,
    {
        let etag_request = headers
            .get(http::header::IF_NONE_MATCH)
            .map(|h| {
                h.to_str()
                    .map_err(|_| CrudError::bad())
                    .and_then(|etag_str| {
                        DiscoveryBackends::etag_from_str(etag_str).map_err(|_| CrudError::bad())
                    })
            })
            .transpose()?;

        let backends = state
            .store()
            .get_all(etag_request)
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?;

        let mut headers = no_cache_headers();
        headers.insert(http::header::ETAG, backends.etag_string().try_into()?);

        match backends.backends {
            None => Ok(JsonCrudResponse::not_modified(headers)),
            Some(backends) => Ok(JsonCrudResponse::ok(backends, headers)),
        }
    }

    pub async fn post_backend<S>(
        State(state): State<DiscoveryState<S>>,
        Json(backend): Json<DiscoveryBackend>,
    ) -> Result<JsonCrudResponse<()>, CrudError>
    where
        S: DiscoveryBackendStore,
    {
        let location = backend.public_key.to_string();

        let result = state
            .store()
            .post(backend)
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?;

        let location = HeaderValue::from_str(&location)?;

        match result {
            Some(_) => Ok(JsonCrudResponse::created_location(location)),
            None => Err(CrudError::conflict(location)),
        }
    }

    pub async fn put_backend<S>(
        State(state): State<DiscoveryState<S>>,
        Path(public_key): Path<String>,
        Json(backend): Json<DiscoveryBackendSparse>,
    ) -> Result<JsonCrudResponse<()>, CrudError>
    where
        S: DiscoveryBackendStore,
    {
        let public_key = public_key.parse().map_err(|_| CrudError::bad())?;

        let backend = DiscoveryBackend {
            public_key,
            backend,
        };

        let was_created = state
            .store()
            .put(backend)
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?;

        if was_created {
            Ok(JsonCrudResponse::created())
        } else {
            Ok(JsonCrudResponse::no_content())
        }
    }

    pub async fn patch_backend<S>(
        State(state): State<DiscoveryState<S>>,
        Path(public_key): Path<String>,
        Json(backend): Json<DiscoveryBackendPatchSparse>,
    ) -> Result<JsonCrudResponse<()>, CrudError>
    where
        S: DiscoveryBackendStore,
    {
        let public_key = public_key.parse().map_err(|_| CrudError::bad())?;

        let backend = DiscoveryBackendPatch {
            public_key,
            backend,
        };

        let patched = state
            .store()
            .patch(backend)
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?;

        if patched {
            Ok(JsonCrudResponse::no_content())
        } else {
            Err(CrudError::not_found())
        }
    }

    pub async fn delete_backend<S>(
        Path(public_key): Path<String>,
        State(state): State<DiscoveryState<S>>,
    ) -> Result<JsonCrudResponse<()>, CrudError>
    where
        S: DiscoveryBackendStore,
    {
        let public_key = public_key.parse().map_err(|_| CrudError::bad())?;

        if state
            .store()
            .delete(&public_key)
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?
        {
            Ok(JsonCrudResponse::no_content())
        } else {
            Err(CrudError::not_found())
        }
    }
}
