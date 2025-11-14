use crate::axum::auth::BearerTokenValidator;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize, Serializer};
use std::fmt::Display;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryClaims {
    pub aud: DiscoveryAudience,
    pub exp: usize,
}

#[derive(Debug, Deserialize, Eq, PartialOrd, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum DiscoveryAudience {
    Discovery,
}

impl Serialize for DiscoveryAudience {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl Display for DiscoveryAudience {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscoveryAudience::Discovery => f.write_str("discovery"),
        }
    }
}

#[derive(Clone)]
pub struct DiscoveryBearerTokenValidator {
    decoding_key: DecodingKey,
    validation: Validation,
}

impl DiscoveryBearerTokenValidator {
    pub fn new(decoding_key: DecodingKey) -> Self {
        let mut validation = Validation::new(Algorithm::ES256);
        validation.set_audience(&[DiscoveryAudience::Discovery]);
        Self {
            decoding_key,
            validation,
        }
    }

    pub fn validate_token(&self, token: &str) -> jsonwebtoken::errors::Result<DiscoveryClaims> {
        let token = decode::<DiscoveryClaims>(token, &self.decoding_key, &self.validation)?;
        if token.claims.aud == DiscoveryAudience::Discovery {
            Ok(token.claims)
        } else {
            Err(jsonwebtoken::errors::Error::from(
                jsonwebtoken::errors::ErrorKind::InvalidToken,
            ))
        }
    }
}

impl BearerTokenValidator for DiscoveryBearerTokenValidator {
    fn validate(&self, token: &str) -> bool {
        self.validate_token(token).is_ok()
    }
}
