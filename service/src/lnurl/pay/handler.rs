use crate::axum::extract::host::ValidatedHost;
use crate::axum::extract::scheme::Scheme;
use crate::axum::header::no_cache_headers;
use crate::lnurl::pay::error::LnUrlPayServiceError;
use crate::lnurl::pay::state::LnUrlPayState;
use crate::lnurl::pay::{Query, UuidParam};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use axum::{extract::State, response::IntoResponse};
use bech32::{Bech32, Hrp};
use image::Luma;
use qrcode::QrCode;
use serde::Deserialize;
use sqlx::types::JsonValue;
use std::io::{self, Cursor};
use switchgear_error::{ChainedContext, ErrorOrigin, ForeignContext};
use switchgear_service_api::balance::LnBalancer;
use switchgear_service_api::lnurl::{LnUrlInvoice, LnUrlOffer, LnUrlOfferTag};
use switchgear_service_api::offer::{Offer, OfferProvider};
use url::Url;
use uuid::Uuid;

pub struct LnUrlPayHandlers;

impl LnUrlPayHandlers {
    #[tracing::instrument(skip_all)]
    pub async fn offer<O, B>(
        ValidatedHost(hostname): ValidatedHost,
        Scheme(scheme): Scheme,
        UuidParam { partition, id, .. }: UuidParam,
        State(state): State<LnUrlPayState<O, B>>,
    ) -> Result<LnUrlPayResponse<LnUrlOffer>, LnUrlPayServiceError>
    where
        O: OfferProvider + Clone,
        B: LnBalancer,
    {
        let offer = Self::get_offer(&hostname, &partition, &id, &state).await?;

        let callback = format!("{scheme}://{hostname}/offers/{partition}/{id}/invoice");
        let callback = Url::parse(&callback)
            .with_foreign_context(|| format!("parsing callback url {callback}"), None)?;

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

    #[tracing::instrument(skip_all)]
    pub async fn invoice<O, B>(
        ValidatedHost(hostname): ValidatedHost,
        UuidParam { partition, id, .. }: UuidParam,
        Query { value: params, .. }: Query<InvoiceParameters>,
        State(state): State<LnUrlPayState<O, B>>,
    ) -> Result<LnUrlPayResponse<LnUrlInvoice>, LnUrlPayServiceError>
    where
        O: OfferProvider + Clone,
        B: LnBalancer,
    {
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
            .chained_context("fetching offer", None)?;
        let offer = offer
            .ok_or_else(|| LnUrlPayServiceError::not_found(format!("offer not found: {}", id)))?;

        if offer.is_expired() {
            return Err(LnUrlPayServiceError::not_found(format!(
                "offer not found: {}",
                id
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
            .chained_context("generating invoice", None)?;

        let invoice = LnUrlInvoice { pr, routes: vec![] };
        let headers = no_cache_headers();
        Ok(LnUrlPayResponse::ok(invoice, headers))
    }

    #[tracing::instrument(skip_all)]
    pub async fn bech32<O, B>(
        ValidatedHost(hostname): ValidatedHost,
        Scheme(scheme): Scheme,
        UuidParam { partition, id, .. }: UuidParam,
        State(state): State<LnUrlPayState<O, B>>,
    ) -> Result<(HeaderMap, String), LnUrlPayServiceError>
    where
        O: OfferProvider + Clone,
        B: LnBalancer,
    {
        let offer = Self::get_offer(&hostname, &partition, &id, &state).await?;

        let callback = format!("{scheme}://{hostname}/offers/{partition}/{id}");
        let callback = Self::gen_bech32(&callback)
            .with_foreign_context(|| format!("encoding bech32 callback {callback}"), None)?;

        let mut headers = Self::expires_headers(offer.expires)?;
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );

        Ok((headers, callback))
    }

    #[tracing::instrument(skip_all)]
    pub async fn bech32_qr<O, B>(
        ValidatedHost(hostname): ValidatedHost,
        Scheme(scheme): Scheme,
        UuidParam { partition, id, .. }: UuidParam,
        State(state): State<LnUrlPayState<O, B>>,
    ) -> Result<(HeaderMap, Vec<u8>), LnUrlPayServiceError>
    where
        O: OfferProvider + Clone,
        B: LnBalancer,
    {
        let offer = Self::get_offer(&hostname, &partition, &id, &state).await?;

        let callback = format!("{scheme}://{hostname}/offers/{partition}/{id}");
        let callback = Self::gen_bech32(&callback)
            .with_foreign_context(|| format!("encoding bech32 callback {callback}"), None)?;
        let qr = QrCode::new(callback.as_bytes())
            .with_foreign_context(|| format!("generating qr code for {callback}"), None)?;

        let scale = state.bech32_qr_scale();
        let dark = state.bech32_qr_dark();
        let light = state.bech32_qr_light();

        let img = qr
            .render::<Luma<u8>>()
            .dark_color(Luma([dark]))
            .light_color(Luma([light]))
            .module_dimensions(scale as u32, scale as u32)
            .build();

        let mut png_bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
            .foreign_context("encoding QR code to PNG", None)?;

        let mut headers = Self::expires_headers(offer.expires)?;
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));

        Ok((headers, png_bytes))
    }

    #[tracing::instrument(skip_all)]
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
            .chained_context("checking balancer health", ErrorOrigin::Upstream)?;
        Ok(LnUrlPayResponse::ok(
            JsonValue::Array(vec![]),
            HeaderMap::new(),
        ))
    }

    #[tracing::instrument(skip_all)]
    fn gen_bech32(callback: &str) -> io::Result<String> {
        let callback =
            Url::parse(callback).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let hrp = Hrp::parse("LNURL").map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let callback = bech32::encode_upper::<Bech32>(hrp, callback.as_str().as_bytes())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(callback)
    }

    #[tracing::instrument(skip_all)]
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
            .chained_context("fetching offer", None)?;
        let offer = offer
            .ok_or_else(|| LnUrlPayServiceError::not_found(format!("offer not found: {}", id)))?;

        if offer.is_expired() {
            return Err(LnUrlPayServiceError::not_found(format!(
                "offer not found: {}",
                id
            )));
        }

        Ok(offer)
    }

    #[tracing::instrument(skip_all)]
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
                    HeaderValue::from_str(&cache_control_value)
                        .foreign_context("building cache-control header", None)?,
                ),
                (
                    header::EXPIRES,
                    HeaderValue::from_str(&expires_header)
                        .foreign_context("building expires header", None)?,
                ),
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
