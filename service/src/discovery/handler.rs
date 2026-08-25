use crate::axum::crud::response::JsonCrudResponse;
use crate::axum::header::no_cache_headers;
use crate::discovery::Json;
use crate::discovery::error::DiscoveryCrudError;
use crate::discovery::state::DiscoveryState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue};
use switchgear_error::{ChainedContext, ErrorOrigin, ForeignContext};
use switchgear_service_api::discovery::{
    DiscoveryBackend, DiscoveryBackendPatch, DiscoveryBackendPatchSparse, DiscoveryBackendSparse,
    DiscoveryBackendStore, DiscoveryBackends,
};

pub struct DiscoveryHandlers;

impl DiscoveryHandlers {
    #[tracing::instrument(skip_all)]
    pub async fn get_backend<S>(
        Path(public_key): Path<String>,
        State(state): State<DiscoveryState<S>>,
    ) -> Result<JsonCrudResponse<DiscoveryBackend>, DiscoveryCrudError>
    where
        S: DiscoveryBackendStore,
    {
        let public_key = public_key
            .parse()
            .foreign_context("parsing discovery backend public key", None)?;

        let backend = state
            .store()
            .get(&public_key)
            .await
            .chained_context("fetching discovery backend", None)?
            .ok_or_else(DiscoveryCrudError::not_found)?;

        let headers = no_cache_headers();

        Ok(JsonCrudResponse::ok(backend, headers))
    }

    #[tracing::instrument(skip_all)]
    pub async fn get_backends<S>(
        headers: HeaderMap,
        State(state): State<DiscoveryState<S>>,
    ) -> Result<JsonCrudResponse<Vec<DiscoveryBackend>>, DiscoveryCrudError>
    where
        S: DiscoveryBackendStore,
    {
        let etag_request = headers
            .get(http::header::IF_NONE_MATCH)
            .map(|h| {
                h.to_str()
                    .foreign_context("reading if-none-match header value", None)
                    .and_then(|etag_str| {
                        DiscoveryBackends::etag_from_str(etag_str)
                            .foreign_context("parsing if-none-match etag", ErrorOrigin::Downstream)
                    })
            })
            .transpose()?;

        let backends = state
            .store()
            .get_all(etag_request)
            .await
            .chained_context("listing discovery backends", None)?;

        let mut headers = no_cache_headers();
        headers.insert(
            http::header::ETAG,
            HeaderValue::from_str(&backends.etag_string())
                .foreign_context("building discovery backends etag header", None)?,
        );

        match backends.backends {
            None => Ok(JsonCrudResponse::not_modified(headers)),
            Some(backends) => Ok(JsonCrudResponse::ok(backends, headers)),
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn post_backend<S>(
        State(state): State<DiscoveryState<S>>,
        Json { value: backend, .. }: Json<DiscoveryBackend>,
    ) -> Result<JsonCrudResponse<()>, DiscoveryCrudError>
    where
        S: DiscoveryBackendStore,
    {
        let location = backend.public_key.to_string();

        let result = state
            .store()
            .post(backend)
            .await
            .chained_context("creating discovery backend", None)?;

        let location = HeaderValue::from_str(&location)
            .foreign_context("building discovery backend location header", None)?;

        match result {
            Some(_) => Ok(JsonCrudResponse::created_location(location)),
            None => Err(DiscoveryCrudError::conflict(location)),
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn put_backend<S>(
        State(state): State<DiscoveryState<S>>,
        Path(public_key): Path<String>,
        Json { value: backend, .. }: Json<DiscoveryBackendSparse>,
    ) -> Result<JsonCrudResponse<()>, DiscoveryCrudError>
    where
        S: DiscoveryBackendStore,
    {
        let public_key = public_key
            .parse()
            .foreign_context("parsing discovery backend public key", None)?;

        let backend = DiscoveryBackend {
            public_key,
            backend,
        };

        let was_created = state
            .store()
            .put(backend)
            .await
            .chained_context("updating discovery backend", None)?;

        if was_created {
            Ok(JsonCrudResponse::created())
        } else {
            Ok(JsonCrudResponse::no_content())
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn patch_backend<S>(
        State(state): State<DiscoveryState<S>>,
        Path(public_key): Path<String>,
        Json { value: backend, .. }: Json<DiscoveryBackendPatchSparse>,
    ) -> Result<JsonCrudResponse<()>, DiscoveryCrudError>
    where
        S: DiscoveryBackendStore,
    {
        let public_key = public_key
            .parse()
            .foreign_context("parsing discovery backend public key", None)?;

        let backend = DiscoveryBackendPatch {
            public_key,
            backend,
        };

        let patched = state
            .store()
            .patch(backend)
            .await
            .chained_context("patching discovery backend", None)?;

        if patched {
            Ok(JsonCrudResponse::no_content())
        } else {
            Err(DiscoveryCrudError::not_found())
        }
    }

    #[tracing::instrument(skip_all)]
    pub async fn delete_backend<S>(
        Path(public_key): Path<String>,
        State(state): State<DiscoveryState<S>>,
    ) -> Result<JsonCrudResponse<()>, DiscoveryCrudError>
    where
        S: DiscoveryBackendStore,
    {
        let public_key = public_key
            .parse()
            .foreign_context("parsing discovery backend public key", None)?;

        let deleted = state
            .store()
            .delete(&public_key)
            .await
            .chained_context("deleting discovery backend", None)?;
        if deleted {
            Ok(JsonCrudResponse::no_content())
        } else {
            Err(DiscoveryCrudError::not_found())
        }
    }
}
