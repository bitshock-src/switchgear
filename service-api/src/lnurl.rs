use crate::offer::{OfferMetadataIdentifier, OfferMetadataImage, OfferMetadataSparse};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use serde::de::{Error, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LnUrlOffer {
    pub callback: Url,
    pub max_sendable: u64,
    pub min_sendable: u64,
    pub tag: LnUrlOfferTag,
    pub metadata: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment_allowed: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LnUrlOfferMetadata(pub OfferMetadataSparse);

impl Serialize for LnUrlOfferMetadata {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut len = 1;
        if self.0.long_text.is_some() {
            len += 1;
        }

        if self.0.image.is_some() {
            len += 1;
        }

        if self.0.identifier.is_some() {
            len += 1;
        }

        let mut seq = serializer.serialize_seq(Some(len))?;

        let text = (LNURL_OFFER_METADATA_ENTRY_TYPE_TEXT, self.0.text.as_str());
        seq.serialize_element(&text)?;

        if let Some(long_text) = &self.0.long_text {
            let long_text = (LNURL_OFFER_METADATA_ENTRY_TYPE_TEXT_LONG, long_text);
            seq.serialize_element(&long_text)?;
        }

        if let Some(image) = &self.0.image {
            let image = match image {
                OfferMetadataImage::Png(image) => {
                    let image = BASE64_STANDARD.encode(image);
                    (LNURL_OFFER_METADATA_ENTRY_TYPE_PNG_IMAGE, image)
                }
                OfferMetadataImage::Jpeg(image) => {
                    let image = BASE64_STANDARD.encode(image);
                    (LNURL_OFFER_METADATA_ENTRY_TYPE_JPEG_IMAGE, image)
                }
            };
            seq.serialize_element(&image)?;
        }

        if let Some(identifier) = &self.0.identifier {
            let identifier = match identifier {
                OfferMetadataIdentifier::Text(identifier) => (
                    LNURL_OFFER_METADATA_ENTRY_TYPE_TEXT_IDENTIFIER,
                    identifier.email(),
                ),
                OfferMetadataIdentifier::Email(identifier) => (
                    LNURL_OFFER_METADATA_ENTRY_TYPE_TEXT_EMAIL,
                    identifier.email(),
                ),
            };
            seq.serialize_element(&identifier)?;
        }

        seq.end()
    }
}

impl<'de> Deserialize<'de> for LnUrlOfferMetadata {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LnUrlOfferMetadataVisitor;

        impl<'de> Visitor<'de> for LnUrlOfferMetadataVisitor {
            type Value = LnUrlOfferMetadata;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an array of [type, value] tuples representing metadata")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut text = None;
                let mut long_text = None;
                let mut image = None;
                let mut identifier = None;

                while let Some(entry) = seq.next_element::<[String; 2]>()? {
                    match entry[0].as_str() {
                        LNURL_OFFER_METADATA_ENTRY_TYPE_TEXT => {
                            text = Some(entry[1].clone());
                        }
                        LNURL_OFFER_METADATA_ENTRY_TYPE_TEXT_LONG => {
                            long_text = Some(entry[1].clone());
                        }
                        LNURL_OFFER_METADATA_ENTRY_TYPE_PNG_IMAGE => {
                            let decoded = BASE64_STANDARD.decode(&entry[1]).map_err(|e| {
                                Error::custom(format!("Invalid base64 PNG data: {e}"))
                            })?;
                            image = Some(OfferMetadataImage::Png(decoded));
                        }
                        LNURL_OFFER_METADATA_ENTRY_TYPE_JPEG_IMAGE => {
                            let decoded = BASE64_STANDARD.decode(&entry[1]).map_err(|e| {
                                Error::custom(format!("Invalid base64 JPEG data: {e}"))
                            })?;
                            image = Some(OfferMetadataImage::Jpeg(decoded));
                        }
                        LNURL_OFFER_METADATA_ENTRY_TYPE_TEXT_IDENTIFIER => {
                            let email = entry[1].parse().map_err(|e| {
                                Error::custom(format!("Invalid email address: {e}"))
                            })?;
                            identifier = Some(OfferMetadataIdentifier::Text(email));
                        }
                        LNURL_OFFER_METADATA_ENTRY_TYPE_TEXT_EMAIL => {
                            let email = entry[1].parse().map_err(|e| {
                                Error::custom(format!("Invalid email address: {e}"))
                            })?;
                            identifier = Some(OfferMetadataIdentifier::Email(email));
                        }
                        _ => {
                            // Unknown metadata type, skip it
                        }
                    }
                }

                let text =
                    text.ok_or_else(|| Error::custom("Missing required 'text/plain' metadata"))?;

                let metadata = OfferMetadataSparse {
                    text,
                    long_text,
                    image,
                    identifier,
                };

                Ok(LnUrlOfferMetadata(metadata))
            }
        }

        deserializer.deserialize_seq(LnUrlOfferMetadataVisitor)
    }
}

const LNURL_OFFER_METADATA_ENTRY_TYPE_TEXT: &str = "text/plain";
const LNURL_OFFER_METADATA_ENTRY_TYPE_TEXT_LONG: &str = "text/long-desc";
const LNURL_OFFER_METADATA_ENTRY_TYPE_PNG_IMAGE: &str = "image/png;base64";
const LNURL_OFFER_METADATA_ENTRY_TYPE_JPEG_IMAGE: &str = "image/jpeg;base64";
const LNURL_OFFER_METADATA_ENTRY_TYPE_TEXT_IDENTIFIER: &str = "text/identifier";
const LNURL_OFFER_METADATA_ENTRY_TYPE_TEXT_EMAIL: &str = "text/email";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LnUrlOfferTag {
    PayRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LnUrlInvoice {
    pub pr: String,
    pub routes: Vec<EmptyJsonValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EmptyJsonValue {}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LnUrlError {
    pub status: LnUrlErrorStatus,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LnUrlErrorStatus {
    Error,
}

#[cfg(test)]
mod test {
    use crate::lnurl::{
        LnUrlError, LnUrlErrorStatus, LnUrlInvoice, LnUrlOffer, LnUrlOfferMetadata, LnUrlOfferTag,
    };
    use crate::offer::{OfferMetadataIdentifier, OfferMetadataImage, OfferMetadataSparse};
    use bitcoin_hashes::{sha256, Hash};
    use lightning_invoice::{Currency, InvoiceBuilder, PaymentSecret};
    use secp256k1_0_29::{Secp256k1, SecretKey};
    use std::time::SystemTime;
    use url::Url;

    #[test]
    fn serialize_lnurloffermetadata_metadata() {
        let metadata = serde_json::to_string(&LnUrlOfferMetadata(OfferMetadataSparse {
            text: "text".to_string(),
            long_text: Some("long text".to_string()),
            image: Some(OfferMetadataImage::Png(vec![0, 1])),
            identifier: Some(OfferMetadataIdentifier::Email(
                "email@example.com".parse().unwrap(),
            )),
        }))
        .unwrap();
        assert_eq!(
            r#"[["text/plain","text"],["text/long-desc","long text"],["image/png;base64","AAE="],["text/email","email@example.com"]]"#,
            metadata.as_str()
        );
    }

    #[test]
    fn deserialize_lnurloffermetadata_metadata() {
        let json = r#"[["text/plain","text"],["text/long-desc","long text"],["image/png;base64","AAE="],["text/email","email@example.com"]]"#;

        let metadata: LnUrlOfferMetadata = serde_json::from_str(json).unwrap();

        assert_eq!(metadata.0.text, "text");
        assert_eq!(metadata.0.long_text, Some("long text".to_string()));
        assert_eq!(metadata.0.image, Some(OfferMetadataImage::Png(vec![0, 1])));
        assert_eq!(
            metadata.0.identifier,
            Some(OfferMetadataIdentifier::Email(
                "email@example.com".parse().unwrap()
            ))
        );
    }

    #[test]
    fn roundtrip_lnurloffermetadata_serialization() {
        let original = LnUrlOfferMetadata(OfferMetadataSparse {
            text: "text".to_string(),
            long_text: Some("long text".to_string()),
            image: Some(OfferMetadataImage::Png(vec![0, 1])),
            identifier: Some(OfferMetadataIdentifier::Email(
                "email@example.com".parse().unwrap(),
            )),
        });

        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: LnUrlOfferMetadata = serde_json::from_str(&serialized).unwrap();

        assert_eq!(original, deserialized);
    }

    #[test]
    fn deserialize_lnurloffermetadata_minimal() {
        let json = r#"[["text/plain","minimal text"]]"#;

        let metadata: LnUrlOfferMetadata = serde_json::from_str(json).unwrap();

        assert_eq!(metadata.0.text, "minimal text");
        assert_eq!(metadata.0.long_text, None);
        assert_eq!(metadata.0.image, None);
        assert_eq!(metadata.0.identifier, None);
    }

    #[test]
    fn deserialize_lnurloffermetadata_missing_text_fails() {
        let json = r#"[["text/long-desc","long text only"]]"#;

        let result: Result<LnUrlOfferMetadata, _> = serde_json::from_str(json);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Missing required 'text/plain' metadata"));
    }

    #[test]
    fn deserialize_lnurloffermetadata_unknown_types_ignored() {
        let json =
            r#"[["text/plain","text"],["unknown/type","ignored"],["text/long-desc","long text"]]"#;

        let metadata: LnUrlOfferMetadata = serde_json::from_str(json).unwrap();

        assert_eq!(metadata.0.text, "text");
        assert_eq!(metadata.0.long_text, Some("long text".to_string()));
        assert_eq!(metadata.0.image, None);
        assert_eq!(metadata.0.identifier, None);
    }

    #[test]
    fn serialize_when_offer_with_metadata_then_returns_json_with_embedded_metadata() {
        let offer = LnUrlOffer {
            callback: Url::parse("https://example.com/callback").unwrap(),
            max_sendable: 0,
            min_sendable: 0,
            tag: LnUrlOfferTag::PayRequest,
            metadata: serde_json::to_string(&LnUrlOfferMetadata(OfferMetadataSparse {
                text: "text".to_string(),
                long_text: Some("long text".to_string()),
                image: Some(OfferMetadataImage::Png(vec![0, 1])),
                identifier: Some(OfferMetadataIdentifier::Email(
                    "email@example.com".parse().unwrap(),
                )),
            }))
            .unwrap(),
            comment_allowed: None,
        };

        let offer = serde_json::to_string(&offer).unwrap();
        assert_eq!(
            r#"{"callback":"https://example.com/callback","maxSendable":0,"minSendable":0,"tag":"payRequest","metadata":"[[\"text/plain\",\"text\"],[\"text/long-desc\",\"long text\"],[\"image/png;base64\",\"AAE=\"],[\"text/email\",\"email@example.com\"]]"}"#,
            offer.as_str()
        );
    }

    #[test]
    fn serialize_when_invoice_with_payment_request_then_returns_json_with_pr_field() {
        let private_key = SecretKey::from_slice(
            &[
                0xe1, 0x26, 0xf6, 0x8f, 0x7e, 0xaf, 0xcc, 0x8b, 0x74, 0xf5, 0x4d, 0x26, 0x9f, 0xe2,
                0x06, 0xbe, 0x71, 0x50, 0x00, 0xf9, 0x4d, 0xac, 0x06, 0x7d, 0x1c, 0x04, 0xa8, 0xca,
                0x3b, 0x2d, 0xb7, 0x34,
            ][..],
        )
        .unwrap();

        let payment_hash = sha256::Hash::from_byte_array([0; 32]);
        let payment_secret = PaymentSecret([42u8; 32]);

        let invoice = LnUrlInvoice {
            pr: InvoiceBuilder::new(Currency::Bitcoin)
                .description("desc".into())
                .payment_hash(payment_hash)
                .payment_secret(payment_secret)
                .timestamp(SystemTime::UNIX_EPOCH)
                .min_final_cltv_expiry_delta(144)
                .build_signed(|hash| Secp256k1::new().sign_ecdsa_recoverable(hash, &private_key))
                .unwrap()
                .to_string(),
            routes: vec![],
        };

        let invoice = serde_json::to_string(&invoice).unwrap();
        assert_eq!(
            r#"{"pr":"lnbc1qqqqqqqdq8v3jhxccpp5qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqsp59g4z52329g4z52329g4z52329g4z52329g4z52329g4z52329g4q9qrsgqcqzysvmeka2qvrqmwhjjh7tx333ssfzfw95432jvd3ne046fvtlzaq0zns05tgfvvfu9jjx9uv0xehscf709styuhzza5fvdqf2374dycxqgp3ym4t6","routes":[]}"#,
            invoice.as_str()
        );
    }

    #[test]
    fn serialize_when_error_with_status_reason_then_returns_json() {
        let error = LnUrlError {
            status: LnUrlErrorStatus::Error,
            reason: "reason".to_string(),
        };

        let error = serde_json::to_string(&error).unwrap();
        assert_eq!(r#"{"status":"ERROR","reason":"reason"}"#, error.as_str());
    }

    #[test]
    fn deserialize_lnurloffer_with_tag() {
        let json = r#"{
            "callback": "https://example.com/callback",
            "maxSendable": 1000000,
            "minSendable": 1000,
            "tag": "payRequest",
            "metadata": "test metadata"
        }"#;

        let offer: LnUrlOffer = serde_json::from_str(json).unwrap();
        assert_eq!(offer.tag, LnUrlOfferTag::PayRequest);
        assert_eq!(offer.callback.as_str(), "https://example.com/callback");
        assert_eq!(offer.max_sendable, 1000000);
        assert_eq!(offer.min_sendable, 1000);
        assert_eq!(offer.metadata, "test metadata");
    }
}
