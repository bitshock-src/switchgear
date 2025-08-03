use crate::api::discovery::{
    DiscoveryBackend, DiscoveryBackendRest, DiscoveryBackendSparse, DiscoveryBackendStore,
};
use crate::axum::crud::error::CrudError;
use crate::axum::crud::response::JsonCrudResponse;
use crate::axum::extract::socket::DiscoveryBackendAddressParam;
use crate::axum::header::no_cache_headers;
use crate::discovery::state::DiscoveryState;
use axum::http::HeaderValue;
use axum::{extract::State, Json};

pub struct DiscoveryHandlers;

impl DiscoveryHandlers {
    pub async fn get_backend<S>(
        DiscoveryBackendAddressParam { partition, address }: DiscoveryBackendAddressParam,
        State(state): State<DiscoveryState<S>>,
    ) -> Result<JsonCrudResponse<DiscoveryBackendRest>, CrudError>
    where
        S: DiscoveryBackendStore,
    {
        let backend = state
            .store()
            .get(&partition, &address)
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?
            .ok_or(CrudError::not_found())?;

        let backend = DiscoveryBackendRest {
            location: format!("{partition}/{}", backend.address.encoded()),
            backend,
        };

        let headers = no_cache_headers();

        Ok(JsonCrudResponse::ok(backend, headers))
    }

    pub async fn get_backends<S>(
        axum::extract::Path(partition): axum::extract::Path<String>,
        State(state): State<DiscoveryState<S>>,
    ) -> Result<JsonCrudResponse<Vec<DiscoveryBackendRest>>, CrudError>
    where
        S: DiscoveryBackendStore,
    {
        let backends = state
            .store()
            .get_all(&partition)
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?;

        let headers = no_cache_headers();

        let backends = backends
            .into_iter()
            .map(|backend| DiscoveryBackendRest {
                location: format!("{partition}/{}", backend.address.encoded()),
                backend,
            })
            .collect();

        Ok(JsonCrudResponse::ok(backends, headers))
    }

    pub async fn post_backend<S>(
        State(state): State<DiscoveryState<S>>,
        Json(backend): Json<DiscoveryBackend>,
    ) -> Result<JsonCrudResponse<()>, CrudError>
    where
        S: DiscoveryBackendStore,
    {
        let result = state
            .store()
            .post(backend.clone())
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?;

        let location = format!("{}/{}", backend.partition, backend.address.encoded());
        let location = HeaderValue::from_str(&location)?;

        match result {
            Some(_) => Ok(JsonCrudResponse::created_location(location)),
            None => Err(CrudError::conflict(location)),
        }
    }

    pub async fn put_backend<S>(
        State(state): State<DiscoveryState<S>>,
        DiscoveryBackendAddressParam { partition, address }: DiscoveryBackendAddressParam,
        Json(backend): Json<DiscoveryBackendSparse>,
    ) -> Result<JsonCrudResponse<()>, CrudError>
    where
        S: DiscoveryBackendStore,
    {
        let backend = DiscoveryBackend {
            partition,
            address,
            backend,
        };

        let was_created = state
            .store()
            .put(backend.clone())
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?;

        if was_created {
            Ok(JsonCrudResponse::created())
        } else {
            Ok(JsonCrudResponse::no_content())
        }
    }

    pub async fn delete_backend<S>(
        DiscoveryBackendAddressParam { partition, address }: DiscoveryBackendAddressParam,
        State(state): State<DiscoveryState<S>>,
    ) -> Result<JsonCrudResponse<()>, CrudError>
    where
        S: DiscoveryBackendStore,
    {
        if state
            .store()
            .delete(&partition, &address)
            .await
            .map_err(|e| crate::crud_error_from_service!(e))?
        {
            Ok(JsonCrudResponse::no_content())
        } else {
            Err(CrudError::not_found())
        }
    }
}
