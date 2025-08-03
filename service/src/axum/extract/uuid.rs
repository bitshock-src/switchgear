use axum::{extract::FromRequestParts, extract::Path, http::request::Parts, http::StatusCode};
use uuid::Uuid;

fn parse_uuid(id: String) -> Result<Uuid, StatusCode> {
    id.parse::<Uuid>().map_err(|_| StatusCode::NOT_FOUND)
}

#[derive(Debug, Clone)]
pub struct UuidParam {
    pub partition: String,
    pub id: Uuid,
}

impl<S> FromRequestParts<S> for UuidParam
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path((partition, id_str)): Path<(String, String)> =
            Path::from_request_parts(parts, state)
                .await
                .map_err(|_| StatusCode::NOT_FOUND)?;

        let id = parse_uuid(id_str)?;

        Ok(UuidParam { partition, id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_valid_uuid() {
        // Test the conversion logic directly
        let valid_uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let uuid = valid_uuid_str.parse::<Uuid>();
        assert!(uuid.is_ok());

        let expected_uuid = uuid.unwrap();
        assert_eq!(expected_uuid.to_string(), valid_uuid_str);
    }

    #[tokio::test]
    async fn test_invalid_uuid() {
        let invalid_uuid_str = "not-a-valid-uuid";
        let uuid = invalid_uuid_str.parse::<Uuid>();
        assert!(uuid.is_err());
    }

    #[tokio::test]
    async fn test_empty_uuid() {
        let empty_str = "";
        let uuid = empty_str.parse::<Uuid>();
        assert!(uuid.is_err());
    }

    #[tokio::test]
    async fn test_malformed_uuid() {
        let malformed_str = "550e8400-e29b-41d4-a716";
        let uuid = malformed_str.parse::<Uuid>();
        assert!(uuid.is_err());
    }

    #[tokio::test]
    async fn test_parse_uuid_function_valid() {
        let valid_uuid_str = "550e8400-e29b-41d4-a716-446655440000".to_string();
        let result = parse_uuid(valid_uuid_str.clone());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), valid_uuid_str);
    }

    #[tokio::test]
    async fn test_parse_uuid_function_invalid() {
        let invalid_uuid_str = "not-a-valid-uuid".to_string();
        let result = parse_uuid(invalid_uuid_str);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::NOT_FOUND);
    }
}
