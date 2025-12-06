use crate::service::HasServiceErrorSource;
use async_trait::async_trait;
use email_address::EmailAddress;
use serde::{Deserialize, Serialize};
use std::error::Error;
pub use uuid::Uuid;

#[async_trait]
pub trait OfferStore {
    type Error: Error + Send + Sync + 'static + HasServiceErrorSource;

    async fn get_offer(
        &self,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<OfferRecord>, Self::Error>;

    async fn get_offers(
        &self,
        partition: &str,
        start: usize,
        count: usize,
    ) -> Result<Vec<OfferRecord>, Self::Error>;

    async fn post_offer(&self, offer: OfferRecord) -> Result<Option<Uuid>, Self::Error>;

    async fn put_offer(&self, offer: OfferRecord) -> Result<bool, Self::Error>;

    async fn delete_offer(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error>;
}

#[async_trait]
pub trait OfferMetadataStore {
    type Error: Error + Send + Sync + 'static + HasServiceErrorSource;

    async fn get_metadata(
        &self,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<OfferMetadata>, Self::Error>;

    async fn get_all_metadata(
        &self,
        partition: &str,
        start: usize,
        count: usize,
    ) -> Result<Vec<OfferMetadata>, Self::Error>;

    async fn post_metadata(&self, offer: OfferMetadata) -> Result<Option<Uuid>, Self::Error>;

    async fn put_metadata(&self, offer: OfferMetadata) -> Result<bool, Self::Error>;

    async fn delete_metadata(&self, partition: &str, id: &Uuid) -> Result<bool, Self::Error>;
}

#[async_trait]
pub trait HttpOfferClient: OfferStore + OfferMetadataStore {
    async fn health(&self) -> Result<(), <Self as OfferStore>::Error>;
}

#[async_trait]
pub trait OfferProvider {
    type Error: Error + Send + Sync + 'static + HasServiceErrorSource;

    async fn offer(
        &self,
        hostname: &str,
        partition: &str,
        id: &Uuid,
    ) -> Result<Option<Offer>, Self::Error>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub partition: String,
    pub id: Uuid,
    pub max_sendable: u64,
    pub min_sendable: u64,
    pub metadata_json_string: String,
    pub metadata_json_hash: [u8; 32],
    pub timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<chrono::DateTime<chrono::Utc>>,
}

impl Offer {
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now();

        if now < self.timestamp {
            return true;
        }

        if let Some(expires) = self.expires {
            if now > expires {
                return true;
            }
        }

        false
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferRecordSparse {
    pub max_sendable: u64,
    pub min_sendable: u64,
    pub metadata_id: Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferRecord {
    pub partition: String,
    pub id: Uuid,
    #[serde(flatten)]
    pub offer: OfferRecordSparse,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferMetadataSparse {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub long_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<OfferMetadataImage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<OfferMetadataIdentifier>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferMetadata {
    pub id: Uuid,
    pub partition: String,
    #[serde(flatten)]
    pub metadata: OfferMetadataSparse,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OfferMetadataImage {
    #[serde(with = "base64_bytes")]
    Png(Vec<u8>),
    #[serde(with = "base64_bytes")]
    Jpeg(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OfferMetadataIdentifier {
    Text(EmailAddress),
    Email(EmailAddress),
}

mod base64_bytes {
    use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
    use base64::Engine;
    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&BASE64_STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        BASE64_STANDARD.decode(s).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod test {
    use crate::offer::{
        OfferMetadata, OfferMetadataIdentifier, OfferMetadataImage, OfferMetadataSparse,
    };

    #[test]
    fn serialize_offer_metadata_for_services() {
        let metadata = OfferMetadata {
            id: Default::default(),
            partition: "default".to_string(),
            metadata: OfferMetadataSparse {
                text: "text".to_string(),
                long_text: Some("long text".to_string()),
                image: Some(OfferMetadataImage::Png(vec![0, 1])),
                identifier: Some(OfferMetadataIdentifier::Email(
                    "email@example.com".parse().unwrap(),
                )),
            },
        };

        let metadata = serde_json::to_string(&metadata).unwrap();
        assert_eq!(
            r#"{"id":"00000000-0000-0000-0000-000000000000","partition":"default","text":"text","longText":"long text","image":{"png":"AAE="},"identifier":{"email":"email@example.com"}}"#,
            metadata.as_str()
        );
    }

    #[test]
    fn deserialize_offer_metadata_for_services() {
        let metadata_expected = OfferMetadata {
            id: Default::default(),
            partition: "default".to_string(),
            metadata: OfferMetadataSparse {
                text: "text".to_string(),
                long_text: Some("long text".to_string()),
                image: Some(OfferMetadataImage::Png(vec![0, 1])),
                identifier: Some(OfferMetadataIdentifier::Email(
                    "email@example.com".parse().unwrap(),
                )),
            },
        };
        let metadata = r#"{"id":"00000000-0000-0000-0000-000000000000","partition":"default","text":"text","longText":"long text","image":{"png":"AAE="},"identifier":{"email":"email@example.com"}}"#;
        let metadata: OfferMetadata = serde_json::from_str(metadata).unwrap();
        assert_eq!(metadata_expected, metadata);
    }
}
