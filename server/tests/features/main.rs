#[path = "common/mod.rs"]
pub mod common;

pub const FEATURE_TEST_CONFIG_PATH: &str = "tests/features/feature-test-config.toml";

mod backend_create_delete;
mod backend_enable_disable;
mod cli_discovery_manage;
mod cli_offer_manage;
mod cli_token;
mod http_remote_stores;
mod invalid_configuration_rejection;
mod lnurl_pay_invoice_generation;
mod lnurl_pay_multi_backend_invoice_generation;
mod server_lifecycle_with_graceful_shutdown;
mod server_persistence;
mod service_enablement;
mod service_logs;
