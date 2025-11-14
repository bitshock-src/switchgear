use crate::axum::auth::BearerTokenValidator;
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize, Serializer};
use std::fmt::Display;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferClaims {
    pub aud: OfferAudience,
    pub exp: usize,
}

#[derive(Debug, Deserialize, Eq, PartialOrd, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum OfferAudience {
    Offer,
}

impl Serialize for OfferAudience {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl Display for OfferAudience {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OfferAudience::Offer => f.write_str("offer"),
        }
    }
}

#[derive(Clone)]
pub struct OfferBearerTokenValidator {
    decoding_key: DecodingKey,
    validation: Validation,
}

impl OfferBearerTokenValidator {
    pub fn new(decoding_key: DecodingKey) -> Self {
        let mut validation = Validation::new(Algorithm::ES256);
        validation.set_audience(&[OfferAudience::Offer]);
        Self {
            decoding_key,
            validation,
        }
    }

    pub fn validate_token(&self, token: &str) -> jsonwebtoken::errors::Result<OfferClaims> {
        let token = decode::<OfferClaims>(token, &self.decoding_key, &self.validation)?;
        if token.claims.aud == OfferAudience::Offer {
            Ok(token.claims)
        } else {
            Err(jsonwebtoken::errors::Error::from(
                jsonwebtoken::errors::ErrorKind::InvalidToken,
            ))
        }
    }
}

impl BearerTokenValidator for OfferBearerTokenValidator {
    fn validate(&self, token: &str) -> bool {
        self.validate_token(token).is_ok()
    }
}
