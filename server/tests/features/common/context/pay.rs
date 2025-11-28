use std::collections::HashMap;
use switchgear_service::api::lnurl::LnUrlOffer;
use switchgear_testing::credentials::lightning::RegTestLnNode;
use uuid::Uuid;

#[derive(Clone)]
pub struct OfferRequest {
    pub offer_id: Option<Uuid>,
    pub metadata_id: Option<Uuid>,
    pub lnurl_offer: Option<LnUrlOffer>,
    pub received_invoice: Option<String>,
}

impl Default for OfferRequest {
    fn default() -> Self {
        Self::new()
    }
}

impl OfferRequest {
    pub fn new() -> Self {
        Self {
            offer_id: None,
            metadata_id: None,
            lnurl_offer: None,
            received_invoice: None,
        }
    }
}

#[derive(Clone)]
pub struct PayeeContext {
    pub node: RegTestLnNode,
    pub offer_requests: HashMap<String, OfferRequest>,
}

impl PayeeContext {
    pub fn new(node: RegTestLnNode) -> Self {
        Self {
            node,
            offer_requests: HashMap::new(),
        }
    }

    pub fn add_offer_request(&mut self, request_key: &str, offer_request: OfferRequest) {
        self.offer_requests
            .insert(request_key.to_string(), offer_request);
    }

    pub fn get_offer_request(&self, request_key: &str) -> Option<&OfferRequest> {
        self.offer_requests.get(request_key)
    }

    pub fn get_offer_request_mut(&mut self, request_key: &str) -> Option<&mut OfferRequest> {
        self.offer_requests.get_mut(request_key)
    }
}
