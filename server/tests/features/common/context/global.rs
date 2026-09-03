use crate::common::client::{LnUrlTestClient, RawHttpClient, TcpProbe};
use crate::common::context::certs::gen_root_cert;
use crate::common::context::jaeger::JaegerClient;
use crate::common::context::pay::{OfferRequest, PayeeContext};
use crate::common::context::server::{CertificateLocation, ServerContext};
use crate::common::context::{Protocol, TestConfiguration, TestConfigurationServiceDomains};
use crate::common::context::{Service, ServiceProfile};
use anyhow::{Context, anyhow};
use rcgen::{Issuer, KeyPair};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use switchgear_components::discovery::http::HttpDiscoveryBackendStore;
use switchgear_components::offer::http::HttpOfferStore;
use switchgear_testing::credentials::lightning::{LnCredentials, RegTestLnNodes};
use switchgear_testing::credentials::otel::{OtelCollector, OtelCredentials};
use tempfile::TempDir;

pub struct GlobalContext {
    temp_dir: TempDir,
    servers: HashMap<String, ServerContext>,
    active_server: String,
    payees: HashMap<String, PayeeContext>,
    service_domains: TestConfigurationServiceDomains,
    pki_root_certificate_path: PathBuf,
    pki_root_cn: String,
    pki_root_issuer: Issuer<'static, KeyPair>,
    ln_nodes: RegTestLnNodes,
    otel_collector: OtelCollector,
    _credentials: LnCredentials,
    _otel_credentials: OtelCredentials,
}

impl GlobalContext {
    pub fn create(feature_test_config_path: &Path) -> anyhow::Result<Self> {
        let _ = rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .map_err(|_| anyhow!("failed to stand up rustls encryption platform"));

        let credentials = LnCredentials::create()?;
        let ln_nodes = credentials.get_backends()?;

        let otel_credentials = OtelCredentials::create()?;
        let otel_collector = otel_credentials.get_collector()?;

        let feature_test_config_path = Self::load_test_config(feature_test_config_path)?;
        let temp_dir = TempDir::new()?;
        let pki_dir = temp_dir.path().join("pki");
        std::fs::create_dir_all(&pki_dir)?;
        let (pki_root_cn, pki_root_issuer, pki_root_certificate_path) =
            gen_root_cert(pki_dir.as_path())?;

        Ok(Self {
            temp_dir,
            servers: HashMap::new(),
            active_server: "".to_string(),
            payees: HashMap::new(),
            service_domains: feature_test_config_path.service_domains,
            pki_root_certificate_path: pki_root_certificate_path.certificate_path,
            pki_root_cn,
            pki_root_issuer,
            ln_nodes,
            otel_collector,
            _credentials: credentials,
            _otel_credentials: otel_credentials,
        })
    }

    pub fn get_ln_nodes(&self) -> &RegTestLnNodes {
        &self.ln_nodes
    }

    fn load_test_config(config_path: &Path) -> anyhow::Result<TestConfiguration> {
        let config = std::fs::read_to_string(config_path)
            .with_context(|| format!("reading TOML file from {config_path:?}"))?;
        toml::from_str(&config).with_context(|| format!("deserializing TOML from {config_path:?}"))
    }

    pub fn activate_server(&mut self, server: &str) {
        self.active_server = server.to_string();
    }

    pub fn add_payee(&mut self, payee_id: &str, nodes: RegTestLnNodes, target_ln_node: &str) {
        self.payees.insert(
            payee_id.to_string(),
            PayeeContext::new(nodes, target_ln_node.to_string()),
        );
    }

    pub fn get_payee(&self, payee_id: &str) -> Option<&PayeeContext> {
        self.payees.get(payee_id)
    }

    pub fn get_payee_mut(&mut self, payee_id: &str) -> Option<&mut PayeeContext> {
        self.payees.get_mut(payee_id)
    }

    pub fn add_offer_request(
        &mut self,
        payee_id: &str,
        request_key: &str,
        offer_request: OfferRequest,
    ) -> anyhow::Result<()> {
        let payee = self
            .get_payee_mut(payee_id)
            .ok_or_else(|| anyhow!("Payee '{}' not found in context", payee_id))?;
        payee.add_offer_request(request_key, offer_request);
        Ok(())
    }

    pub fn get_offer_request(&self, payee_id: &str, request_key: &str) -> Option<&OfferRequest> {
        self.get_payee(payee_id)?.get_offer_request(request_key)
    }

    pub fn get_offer_request_mut(
        &mut self,
        payee_id: &str,
        request_key: &str,
    ) -> Option<&mut OfferRequest> {
        self.get_payee_mut(payee_id)?
            .get_offer_request_mut(request_key)
    }

    pub fn stop_all_servers(&mut self) -> anyhow::Result<()> {
        self.signal_all_servers(sysinfo::Signal::Term)
    }

    pub fn signal_all_servers(&mut self, signal: sysinfo::Signal) -> anyhow::Result<()> {
        for server in self.servers.values_mut() {
            server.signal_server(signal)?;
        }
        Ok(())
    }

    pub fn add_server(
        &mut self,
        server_key: &str,
        config_path: PathBuf,
        lnurl_protocol: Protocol,
        discovery_protocol: Protocol,
        offer_protocol: Protocol,
    ) -> anyhow::Result<()> {
        let lnurl_port = Self::allocate_service_port()?;
        let discovery_port = Self::allocate_service_port()?;
        let offer_port = Self::allocate_service_port()?;

        self.servers.insert(
            server_key.to_string(),
            ServerContext::create(
                config_path,
                self.temp_dir.path(),
                self.pki_root_certificate_path.clone(),
                &self.pki_root_issuer,
                &self.pki_root_cn,
                ServiceProfile {
                    domain: self.service_domains.lnurl.clone(),
                    protocol: lnurl_protocol,
                    address: format!("127.0.0.1:{lnurl_port}").parse()?,
                },
                ServiceProfile {
                    domain: self.service_domains.discovery.clone(),
                    protocol: discovery_protocol,
                    address: format!("127.0.0.1:{discovery_port}").parse()?,
                },
                ServiceProfile {
                    domain: self.service_domains.offer.clone(),
                    protocol: offer_protocol,
                    address: format!("127.0.0.1:{offer_port}").parse()?,
                },
                self.otel_collector.clone(),
            )?,
        );

        Ok(())
    }

    fn allocate_service_port() -> anyhow::Result<u16> {
        let ports_path = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        let port = switchgear_testing::ports::PortAllocator::find_available_port(&ports_path)?;
        Ok(port)
    }

    pub fn get_server_mut(&mut self, server_key: &str) -> Option<&mut ServerContext> {
        self.servers.get_mut(server_key)
    }

    fn get_active_server(&self) -> anyhow::Result<&ServerContext> {
        let active_server = &self.active_server;
        let server = self.servers.get(active_server).ok_or_else(|| {
            anyhow!(format!(
                "active server {active_server} not found in context"
            ))
        })?;
        Ok(server)
    }

    fn get_active_server_mut(&mut self) -> anyhow::Result<&mut ServerContext> {
        let active_server = &self.active_server;
        let server = self.servers.get_mut(active_server).ok_or_else(|| {
            anyhow!(format!(
                "active server {active_server} not found in context"
            ))
        })?;
        Ok(server)
    }

    pub async fn start_active_server(
        &mut self,
        start_services: &[Service],
        log_level: switchgear_server::level::Level,
    ) -> anyhow::Result<u32> {
        self.get_active_server_mut()?
            .start_server(start_services, log_level)
            .await
    }

    pub fn get_active_discovery_store_dir(&self) -> anyhow::Result<&Path> {
        Ok(self.get_active_server()?.discovery_store_dir())
    }

    pub fn get_active_offer_store_dir(&self) -> anyhow::Result<&Path> {
        Ok(self.get_active_server()?.offer_store_dir())
    }

    pub fn get_active_server_config_path(&self) -> anyhow::Result<&Path> {
        Ok(self.get_active_server()?.config_path())
    }

    pub fn get_active_lnurl_client(&self) -> anyhow::Result<&LnUrlTestClient> {
        Ok(self.get_active_server()?.lnurl_client())
    }

    pub fn get_active_lnurl_probe(&self) -> anyhow::Result<&TcpProbe> {
        Ok(self.get_active_server()?.lnurl_probe())
    }

    pub fn get_active_discovery_client(&self) -> anyhow::Result<&HttpDiscoveryBackendStore> {
        Ok(self.get_active_server()?.discovery_client())
    }

    pub fn get_active_discovery_probe(&self) -> anyhow::Result<&TcpProbe> {
        Ok(self.get_active_server()?.discovery_probe())
    }

    pub fn get_active_offer_client(&self) -> anyhow::Result<&HttpOfferStore> {
        Ok(self.get_active_server()?.offer_client())
    }

    pub fn get_active_offer_probe(&self) -> anyhow::Result<&TcpProbe> {
        Ok(self.get_active_server()?.offer_probe())
    }

    pub fn get_active_stderr_buffer(&self) -> anyhow::Result<Arc<Mutex<Vec<String>>>> {
        Ok(self.get_active_server()?.stderr_buffer())
    }

    pub fn get_active_stdout_buffer(&self) -> anyhow::Result<Arc<Mutex<Vec<String>>>> {
        Ok(self.get_active_server()?.stdout_buffer())
    }

    pub fn get_pki_root_certificate_path(&self) -> &Path {
        &self.pki_root_certificate_path
    }

    pub fn get_otel_client_cert_path(&self) -> Option<&Path> {
        self.otel_collector.client_cert_path.as_deref()
    }

    pub fn get_otel_client_key_path(&self) -> Option<&Path> {
        self.otel_collector.client_key_path.as_deref()
    }

    pub fn get_jaeger_query_endpoint(&self) -> &str {
        &self.otel_collector.jaeger_query_endpoint
    }

    /// Returns a Jaeger client bound to the collector's query endpoint,
    /// pre-configured with the test defaults (30s timeout, 500ms interval).
    pub fn jaeger(&self) -> JaegerClient {
        JaegerClient::new(&self.otel_collector.jaeger_query_endpoint)
    }

    /// Set the active server's OTLP resource attributes (`k=v,k=v`). Used by
    /// metrics tests to tag the child's telemetry with a value they can select
    /// on. Call before starting the server.
    pub fn set_active_otlp_resource_attributes(
        &mut self,
        otlp_resource_attributes: Option<String>,
    ) -> anyhow::Result<()> {
        self.get_active_server_mut()?
            .set_otlp_resource_attributes(otlp_resource_attributes);
        Ok(())
    }

    /// Build a raw HTTP client aimed at the active server's given service.
    pub fn active_raw_client_for(&self, service: Service) -> anyhow::Result<RawHttpClient> {
        self.get_active_server()?.raw_client_for(service)
    }

    /// Read the bearer token that the given service's authenticated clients
    /// send, useful for constructing signed raw requests.
    pub fn active_service_token(&self, service: Service) -> anyhow::Result<String> {
        let path = match service {
            Service::Discovery => self.get_active_server()?.discovery_authorization(),
            Service::Offer => self.get_active_server()?.offer_authorization(),
            Service::LnUrl => return Err(anyhow!("lnurl service does not authenticate requests")),
        };
        let s = std::fs::read_to_string(path)
            .with_context(|| format!("reading {service} bearer token from {}", path.display()))?;
        Ok(s.trim().to_string())
    }

    /// Directly override the active server's HTTP offer-store URL. Use a dead
    /// address (e.g. `https://127.0.0.1:1`) to provoke a reqwest transport
    /// failure at the offer-fetch path.
    pub fn set_active_offer_store_url_raw(&mut self, url: Option<String>) -> anyhow::Result<()> {
        self.get_active_server_mut()?.set_offer_store_url(url);
        Ok(())
    }

    /// Directly override the active server's HTTP discovery-store URL.
    pub fn set_active_discovery_store_url_raw(
        &mut self,
        url: Option<String>,
    ) -> anyhow::Result<()> {
        self.get_active_server_mut()?.set_discovery_store_url(url);
        Ok(())
    }

    /// Directly override the active server's discovery-store DB URI (e.g. a
    /// SQLite path that can't be created, to provoke sqlx::Error at connect).
    pub fn set_active_discovery_store_database_uri(&mut self, uri: String) -> anyhow::Result<()> {
        self.get_active_server_mut()?
            .set_discovery_store_database_uri(uri);
        Ok(())
    }

    /// Directly override the active server's offer-store DB URI.
    pub fn set_active_offer_store_database_uri(&mut self, uri: String) -> anyhow::Result<()> {
        self.get_active_server_mut()?
            .set_offer_store_database_uri(uri);
        Ok(())
    }

    pub fn get_active_discovery_service_profile(&self) -> anyhow::Result<ServiceProfile> {
        self.get_active_server()?
            .get_service_profile(Service::Discovery)
    }

    pub fn get_active_discovery_authorization(&self) -> anyhow::Result<&Path> {
        Ok(self.get_active_server()?.discovery_authorization())
    }

    pub fn get_active_offer_service_profile(&self) -> anyhow::Result<ServiceProfile> {
        self.get_active_server()?
            .get_service_profile(Service::Offer)
    }

    pub fn get_active_offer_authorization(&self) -> anyhow::Result<&Path> {
        Ok(self.get_active_server()?.offer_authorization())
    }

    pub fn wait_active_exit_code(&mut self) -> anyhow::Result<i32> {
        self.get_active_server_mut()?.wait_exit_code()
    }

    /// Every server's exit code, keyed by the name it was added under.
    pub fn wait_all_exit_codes(&mut self) -> anyhow::Result<Vec<(String, i32)>> {
        self.servers
            .iter_mut()
            .map(|(name, server)| Ok((name.clone(), server.wait_exit_code()?)))
            .collect()
    }

    pub fn has_active_server_process(&self) -> anyhow::Result<bool> {
        Ok(self.get_active_server()?.has_server_process())
    }

    pub fn set_discovery_store_url(
        &mut self,
        service_server_key_id: &str,
        client_server_key_id: &str,
    ) -> anyhow::Result<()> {
        let service_server_profile = self
            .servers
            .get(service_server_key_id)
            .ok_or_else(|| anyhow!("src server not found"))?;
        let service_server_profile =
            service_server_profile.get_service_profile(Service::Discovery)?;

        let client_server = self
            .servers
            .get_mut(client_server_key_id)
            .ok_or_else(|| anyhow!("dest server not found"))?;

        client_server.set_discovery_store_url(Self::get_service_url(service_server_profile).into());

        Ok(())
    }

    pub fn set_discovery_store_database_uri(
        &mut self,
        server_key_id: &str,
        database_url: String,
    ) -> anyhow::Result<()> {
        let server = self
            .servers
            .get_mut(server_key_id)
            .ok_or_else(|| anyhow!("server not found"))?;

        server.set_discovery_store_database_uri(database_url);

        Ok(())
    }

    pub fn set_discovery_store_authorization(
        &mut self,
        service_server_key_id: &str,
        client_server_key_id: &str,
    ) -> anyhow::Result<()> {
        let service_discovery_authorization = self
            .servers
            .get(service_server_key_id)
            .ok_or_else(|| anyhow!("src server not found"))?;
        let service_discovery_authorization = service_discovery_authorization
            .discovery_authorization()
            .to_path_buf();

        let client_server = self
            .servers
            .get_mut(client_server_key_id)
            .ok_or_else(|| anyhow!("dest server not found"))?;

        client_server.set_discovery_store_authorization(Some(service_discovery_authorization));

        Ok(())
    }

    pub fn set_offer_store_authorization(
        &mut self,
        service_server_key_id: &str,
        client_server_key_id: &str,
    ) -> anyhow::Result<()> {
        let service_offer_authorization = self
            .servers
            .get(service_server_key_id)
            .ok_or_else(|| anyhow!("src server not found"))?;
        let service_offer_authorization = service_offer_authorization
            .offer_authorization()
            .to_path_buf();

        let client_server = self
            .servers
            .get_mut(client_server_key_id)
            .ok_or_else(|| anyhow!("dest server not found"))?;

        client_server.set_offer_store_authorization(Some(service_offer_authorization));

        Ok(())
    }

    pub fn set_certificate_location(
        &mut self,
        server_key_id: &str,
        certificate_location: CertificateLocation,
    ) -> anyhow::Result<()> {
        let server = self
            .servers
            .get_mut(server_key_id)
            .ok_or_else(|| anyhow!("server not found"))?;

        server.set_certificate_location(certificate_location);

        Ok(())
    }

    pub fn set_offer_store_url(
        &mut self,
        service_server_key_id: &str,
        client_server_key_id: &str,
    ) -> anyhow::Result<()> {
        let service_server_profile = self
            .servers
            .get(service_server_key_id)
            .ok_or_else(|| anyhow!("src server not found"))?;
        let service_server_profile = service_server_profile.get_service_profile(Service::Offer)?;

        let client_server = self
            .servers
            .get_mut(client_server_key_id)
            .ok_or_else(|| anyhow!("dest server not found"))?;

        client_server.set_offer_store_url(Self::get_service_url(service_server_profile).into());

        Ok(())
    }

    pub fn set_offer_store_database_uri(
        &mut self,
        server_key_id: &str,
        database_uri: String,
    ) -> anyhow::Result<()> {
        let server = self
            .servers
            .get_mut(server_key_id)
            .ok_or_else(|| anyhow!("server not found"))?;

        server.set_offer_store_database_uri(database_uri);

        Ok(())
    }

    pub fn set_ln_trusted_roots_path(
        &mut self,
        server_key_id: &str,
        ln_trusted_roots_path: Option<PathBuf>,
    ) -> anyhow::Result<()> {
        let server = self
            .servers
            .get_mut(server_key_id)
            .ok_or_else(|| anyhow!("server not found"))?;

        server.set_ln_trusted_roots_path(ln_trusted_roots_path);

        Ok(())
    }

    fn get_service_url(profile: ServiceProfile) -> String {
        format!(
            "{}://{}:{}",
            profile.protocol,
            profile.domain,
            profile.address.port(),
        )
    }

    pub fn set_mysql_username_file(
        &mut self,
        server_key_id: &str,
        path: Option<PathBuf>,
    ) -> anyhow::Result<()> {
        let server = self
            .servers
            .get_mut(server_key_id)
            .ok_or_else(|| anyhow!("server not found"))?;

        server.set_mysql_username_file(path);
        Ok(())
    }

    pub fn set_mysql_password_file(
        &mut self,
        server_key_id: &str,
        path: Option<PathBuf>,
    ) -> anyhow::Result<()> {
        let server = self
            .servers
            .get_mut(server_key_id)
            .ok_or_else(|| anyhow!("server not found"))?;

        server.set_mysql_password_file(path);
        Ok(())
    }

    pub fn set_postgres_username_file(
        &mut self,
        server_key_id: &str,
        path: Option<PathBuf>,
    ) -> anyhow::Result<()> {
        let server = self
            .servers
            .get_mut(server_key_id)
            .ok_or_else(|| anyhow!("server not found"))?;

        server.set_postgres_username_file(path);
        Ok(())
    }

    pub fn set_postgres_password_file(
        &mut self,
        server_key_id: &str,
        path: Option<PathBuf>,
    ) -> anyhow::Result<()> {
        let server = self
            .servers
            .get_mut(server_key_id)
            .ok_or_else(|| anyhow!("server not found"))?;

        server.set_postgres_password_file(path);
        Ok(())
    }

    /// Replace the active server's OTLP collector before start. Used by the
    /// OTLP-compliance tests to swap the shared container collector for a
    /// per-test in-process `TestOtlpCollector`.
    pub fn set_active_otel_collector(
        &mut self,
        otel_collector: OtelCollector,
    ) -> anyhow::Result<()> {
        self.get_active_server_mut()?
            .set_otel_collector_override(otel_collector);
        Ok(())
    }
}

impl Drop for GlobalContext {
    fn drop(&mut self) {
        let _ = self.stop_all_servers();
    }
}
