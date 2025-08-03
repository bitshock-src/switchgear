use crate::api::balance::LnBalancer;
use crate::api::offer::OfferProvider;
use crate::axum::extract::host::AllowedHosts;
use crate::axum::extract::scheme::Scheme;
use axum::extract::FromRef;
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct LnUrlPayState<O, B> {
    partitions: HashSet<String>,
    scheme: Scheme,
    offer_provider: O,
    balancer: B,
    invoice_expiry: u64,
    allowed_hosts: AllowedHosts,
    comment_allowed: Option<u32>,
}

impl<O, B> FromRef<LnUrlPayState<O, B>> for Scheme {
    fn from_ref(input: &LnUrlPayState<O, B>) -> Self {
        input.scheme.clone()
    }
}

impl<O, B> FromRef<LnUrlPayState<O, B>> for AllowedHosts {
    fn from_ref(input: &LnUrlPayState<O, B>) -> Self {
        input.allowed_hosts.clone()
    }
}

impl<O, B> LnUrlPayState<O, B>
where
    O: OfferProvider + Clone,
    B: LnBalancer,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        partitions: HashSet<String>,
        offer_provider: O,
        balancer: B,
        invoice_expiry: u64,
        scheme: Scheme,
        allowed_hosts: HashSet<String>,
        comment_allowed: Option<u32>,
    ) -> Self {
        Self {
            partitions,
            offer_provider,
            balancer,
            invoice_expiry,
            scheme,
            allowed_hosts: AllowedHosts(allowed_hosts),
            comment_allowed,
        }
    }

    pub fn offer_provider(&self) -> &O {
        &self.offer_provider
    }

    pub fn balancer(&self) -> &B {
        &self.balancer
    }

    pub fn invoice_expiry(&self) -> u64 {
        self.invoice_expiry
    }

    pub fn partitions(&self) -> &HashSet<String> {
        &self.partitions
    }

    pub fn comment_allowed(&self) -> Option<u32> {
        self.comment_allowed
    }
}
