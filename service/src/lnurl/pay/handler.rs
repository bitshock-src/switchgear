use crate::api::balance::LnBalancer;
use crate::api::lnurl::{LnUrlInvoice, LnUrlOffer, LnUrlOfferTag};
use crate::api::offer::{Offer, OfferProvider};
use crate::axum::extract::host::ValidatedHost;
use crate::axum::extract::scheme::Scheme;
use crate::axum::extract::uuid::UuidParam;
use crate::axum::header::no_cache_headers;
use crate::lnurl::pay::error::LnUrlPayServiceError;
use crate::lnurl::pay::state::LnUrlPayState;
use axum::extract::Query;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use axum::{extract::State, response::IntoResponse};
use bech32::{Bech32, Hrp};
use image::{ImageFormat, Luma};
use qrcode::QrCode;
use serde::Deserialize;
use sqlx::types::JsonValue;
use std::io;
use url::Url;
use uuid::Uuid;

pub struct LnUrlPayHandlers;

impl LnUrlPayHandlers {
    pub async fn offer<O, B>(
        ValidatedHost(hostname): ValidatedHost,
        Scheme(scheme): Scheme,
        UuidParam { partition, id }: UuidParam,
        State(state): State<LnUrlPayState<O, B>>,
    ) -> Result<LnUrlPayResponse<LnUrlOffer>, LnUrlPayServiceError>
    where
        O: OfferProvider + Clone,
        B: LnBalancer,
    {
        if !state.partitions().contains(&partition) {
            return Err(LnUrlPayServiceError::not_found(format!(
                "offer not found: {}",
                &id
            )));
        }

        let offer = Self::get_offer(&hostname, &partition, &id, &state).await?;

        let callback = format!("{scheme}://{hostname}/offers/{partition}/{id}/invoice");
        let callback = Url::parse(&callback).map_err(|e| {
            LnUrlPayServiceError::internal_error(
                module_path!(),
                &format!("{}:{}", file!(), line!()),
                format!("{e} : when parsing {callback}"),
            )
        })?;

        let lnurl_offer = LnUrlOffer {
            callback,
            max_sendable: offer.max_sendable,
            min_sendable: offer.min_sendable,
            tag: LnUrlOfferTag::PayRequest,
            metadata: offer.metadata_json_string,
            comment_allowed: state.comment_allowed(),
        };

        let headers = Self::expires_headers(offer.expires)?;
        Ok(LnUrlPayResponse::ok(lnurl_offer, headers))
    }

    pub async fn invoice<O, B>(
        ValidatedHost(hostname): ValidatedHost,
        UuidParam { partition, id }: UuidParam,
        Query(params): Query<InvoiceParameters>,
        State(state): State<LnUrlPayState<O, B>>,
    ) -> Result<LnUrlPayResponse<LnUrlInvoice>, LnUrlPayServiceError>
    where
        O: OfferProvider + Clone,
        B: LnBalancer,
    {
        if !state.partitions().contains(&partition) {
            return Err(LnUrlPayServiceError::not_found(format!(
                "offer not found: {}",
                &id
            )));
        }

        let comment_allowed = state.comment_allowed().unwrap_or(0);

        let key = match params.comment {
            None => params.comment.map_or_else(Vec::new, |c| c.into_bytes()),
            Some(comment) => {
                if comment.len() > comment_allowed as usize {
                    return Err(LnUrlPayServiceError::bad_request("invalid comment"));
                }
                comment.into_bytes()
            }
        };

        let offer = state
            .offer_provider()
            .offer(&hostname, &partition, &id)
            .await
            .map_err(|e| crate::lnurl_pay_error_from_service!(e))?;
        let offer = offer
            .ok_or_else(|| LnUrlPayServiceError::not_found(format!("offer not found: {}", &id)))?;

        if offer.is_expired() {
            return Err(LnUrlPayServiceError::not_found(format!(
                "offer not found: {}",
                &id
            )));
        }

        // Validate amount is within the offer's range
        if params.amount < offer.min_sendable || params.amount > offer.max_sendable {
            return Err(LnUrlPayServiceError::bad_request(format!(
                "Amount {} is outside valid range [{}, {}]",
                params.amount, offer.min_sendable, offer.max_sendable
            )));
        }

        let pr = state
            .balancer()
            .get_invoice(&offer, params.amount, state.invoice_expiry(), &key)
            .await
            .map_err(|e| crate::lnurl_pay_error_from_service!(e))?;

        let invoice = LnUrlInvoice { pr, routes: vec![] };
        let headers = no_cache_headers();
        Ok(LnUrlPayResponse::ok(invoice, headers))
    }

    pub async fn bech32<O, B>(
        ValidatedHost(hostname): ValidatedHost,
        Scheme(scheme): Scheme,
        UuidParam { partition, id }: UuidParam,
        State(state): State<LnUrlPayState<O, B>>,
    ) -> Result<(HeaderMap, String), LnUrlPayServiceError>
    where
        O: OfferProvider + Clone,
        B: LnBalancer,
    {
        if !state.partitions().contains(&partition) {
            return Err(LnUrlPayServiceError::not_found(format!(
                "offer not found: {}",
                &id
            )));
        }

        let offer = Self::get_offer(&hostname, &partition, &id, &state).await?;

        let callback = format!("{scheme}://{hostname}/offers/{partition}/{id}");
        let callback = Self::gen_bech32(&callback).map_err(|e| {
            LnUrlPayServiceError::internal_error(
                module_path!(),
                &format!("{}:{}", file!(), line!()),
                format!("{e} : when parsing {callback}"),
            )
        })?;

        let mut headers = Self::expires_headers(offer.expires)?;
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );

        Ok((headers, callback))
    }

    pub async fn bech32_qr<O, B>(
        ValidatedHost(hostname): ValidatedHost,
        Scheme(scheme): Scheme,
        UuidParam { partition, id }: UuidParam,
        State(state): State<LnUrlPayState<O, B>>,
    ) -> Result<(HeaderMap, Vec<u8>), LnUrlPayServiceError>
    where
        O: OfferProvider + Clone,
        B: LnBalancer,
    {
        if !state.partitions().contains(&partition) {
            return Err(LnUrlPayServiceError::not_found(format!(
                "offer not found: {}",
                &id
            )));
        }

        let offer = Self::get_offer(&hostname, &partition, &id, &state).await?;

        let callback = format!("{scheme}://{hostname}/offers/{partition}/{id}");
        let callback = Self::gen_bech32(&callback).map_err(|e| {
            LnUrlPayServiceError::internal_error(
                module_path!(),
                &format!("{}:{}", file!(), line!()),
                format!("{e} : when parsing {callback}"),
            )
        })?;
        let callback = QrCode::new(callback.as_bytes()).map_err(|e| {
            LnUrlPayServiceError::internal_error(
                module_path!(),
                &format!("{}:{}", file!(), line!()),
                format!("{e} : while generating qr code for {callback}"),
            )
        })?;
        let img = callback.render::<Luma<u8>>().build();
        let mut png_bytes = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png_bytes), ImageFormat::Png)
            .map_err(|e| {
                LnUrlPayServiceError::internal_error(
                    module_path!(),
                    &format!("{}:{}", file!(), line!()),
                    format!("{e} : while encoding QR code to PNG"),
                )
            })?;
        let callback = png_bytes;

        let mut headers = Self::expires_headers(offer.expires)?;
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));

        Ok((headers, callback))
    }

    pub async fn health_full<O, B>(
        State(state): State<LnUrlPayState<O, B>>,
    ) -> Result<LnUrlPayResponse<JsonValue>, LnUrlPayServiceError>
    where
        O: OfferProvider + Clone,
        B: LnBalancer,
    {
        state
            .balancer()
            .health()
            .await
            .map_err(|e| crate::lnurl_pay_error_from_service!(e))?;
        Ok(LnUrlPayResponse::ok(
            JsonValue::Array(vec![]),
            HeaderMap::new(),
        ))
    }

    fn gen_bech32(callback: &str) -> io::Result<String> {
        let callback =
            Url::parse(callback).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let hrp = Hrp::parse("LNURL").map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let callback = bech32::encode_upper::<Bech32>(hrp, callback.as_str().as_bytes())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(callback)
    }

    async fn get_offer<O, B>(
        hostname: &str,
        partition: &str,
        id: &Uuid,
        state: &LnUrlPayState<O, B>,
    ) -> Result<Offer, LnUrlPayServiceError>
    where
        O: OfferProvider + Clone,
        B: LnBalancer,
    {
        let offer = state
            .offer_provider()
            .offer(hostname, partition, id)
            .await
            .map_err(|e| crate::lnurl_pay_error_from_service!(e))?;
        let offer = offer
            .ok_or_else(|| LnUrlPayServiceError::not_found(format!("offer not found: {}", &id)))?;

        if offer.is_expired() {
            return Err(LnUrlPayServiceError::not_found(format!(
                "offer not found: {}",
                &id
            )));
        }

        Ok(offer)
    }

    fn expires_headers(
        expires: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<HeaderMap, LnUrlPayServiceError> {
        let headers = if let Some(expires) = expires {
            let now = chrono::Utc::now();
            let expires_in_seconds = (expires - now).num_seconds().max(0) as u64;
            let cache_control_value = format!("public, max-age={expires_in_seconds}");
            let expires_header = expires.format("%a, %d %b %Y %H:%M:%S GMT").to_string();

            HeaderMap::from_iter(vec![
                (
                    header::CACHE_CONTROL,
                    HeaderValue::from_str(&cache_control_value)?,
                ),
                (header::EXPIRES, HeaderValue::from_str(&expires_header)?),
            ])
        } else {
            no_cache_headers()
        };
        Ok(headers)
    }
}

#[derive(Deserialize, Debug)]
pub struct InvoiceParameters {
    pub amount: u64,
    pub comment: Option<String>,
}

#[derive(Debug)]
pub struct LnUrlPayResponse<T> {
    body: T,
    status: StatusCode,
    headers: HeaderMap,
}

impl<T> LnUrlPayResponse<T> {
    pub fn ok(body: T, headers: HeaderMap) -> Self {
        Self {
            body,
            status: StatusCode::OK,
            headers,
        }
    }
}

impl<T> IntoResponse for LnUrlPayResponse<T>
where
    axum::Json<T>: IntoResponse,
{
    fn into_response(self) -> Response {
        (self.status, self.headers, axum::Json(self.body)).into_response()
    }
}
