use crate::common::client::{LnUrlTestClient, TcpProbe};
use crate::common::context::certs::gen_server_cert;
use crate::common::context::token::generate_service_token;
use crate::common::context::Protocol;
use crate::common::context::{
    DiscoveryServiceConfigOverride, LnUrlBalancerServiceConfigOverride, OfferServiceConfigOverride,
    ServerConfigOverrides, Service, ServiceProfile,
};
use rcgen::{Issuer, KeyPair};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use switchgear_server::config::TlsConfig;
use switchgear_service::components::discovery::http::HttpDiscoveryBackendStore;
use switchgear_service::components::offer::http::HttpOfferStore;
use uuid::Uuid;

pub struct ServerContext {
    id: Uuid,
    config_path: PathBuf,
    discovery_store_dir: PathBuf,
    offer_store_dir: PathBuf,
    pki_root_certificate_path: PathBuf,
    server_process: Option<Child>,

    exit_code: i32,

    stdout_buffer: Arc<Mutex<Vec<String>>>,
    stderr_buffer: Arc<Mutex<Vec<String>>>,

    offer_store_url: Option<String>,
    discovery_store_url: Option<String>,

    discovery_store_authorization: Option<PathBuf>,
    offer_store_authorization: Option<PathBuf>,

    lnurl_client: LnUrlTestClient,
    discovery_client: HttpDiscoveryBackendStore,
    offer_client: HttpOfferStore,

    discovery_authority: PathBuf,
    discovery_authorization: PathBuf,

    offer_authority: PathBuf,
    offer_authorization: PathBuf,

    lnurl_probe: TcpProbe,
    discovery_probe: TcpProbe,
    offer_probe: TcpProbe,

    server_config_overrides: ServerConfigOverrides,
}

impl ServerContext {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        config_path: PathBuf,
        base_path: &Path,
        pki_root_certificate_path: PathBuf,
        pki_root_issuer: &Issuer<'_, KeyPair>,
        pki_cn: &str,
        lnurl_service_profile: ServiceProfile,
        discovery_service_profile: ServiceProfile,
        offer_service_profile: ServiceProfile,
    ) -> anyhow::Result<Self> {
        let id = Uuid::new_v4();

        let discovery_store_dir = base_path.join(id.to_string()).join("discovery_store");
        std::fs::create_dir_all(&discovery_store_dir)?;

        let offer_store_dir = base_path.join(id.to_string()).join("offer_store");
        std::fs::create_dir_all(&offer_store_dir)?;

        let certs_dir = base_path.join(id.to_string()).join("certs");
        std::fs::create_dir_all(&certs_dir)?;

        let authority_dir = base_path.join(id.to_string()).join("authority");
        std::fs::create_dir_all(&authority_dir)?;

        let discovery_authority_path = authority_dir.join("discovery_authority.pem");
        let discovery_authority_private_key =
            authority_dir.join("discovery_authority_private_key.pem");
        let discovery_authorization = authority_dir.join("discovery_authorization.txt");
        let discovery_authorization_token = generate_service_token(
            Service::Discovery,
            3600,
            &discovery_authority_path,
            &discovery_authority_private_key,
            &discovery_authorization,
        )?;

        let offer_authority_path = authority_dir.join("offer_authority.pem");
        let offer_authority_path_private_key =
            authority_dir.join("offer_authority_private_key.pem");
        let offer_authorization = authority_dir.join("offer_authorization.txt");
        let offer_authorization_token = generate_service_token(
            Service::Offer,
            3600,
            &offer_authority_path,
            &offer_authority_path_private_key,
            &offer_authorization,
        )?;

        let lnurl_certs_paths = gen_server_cert(
            pki_cn,
            &lnurl_service_profile.domain,
            pki_root_issuer,
            certs_dir.as_path(),
        )?;

        let discovery_certs_paths = gen_server_cert(
            pki_cn,
            &discovery_service_profile.domain,
            pki_root_issuer,
            certs_dir.as_path(),
        )?;

        let offer_certs_paths = gen_server_cert(
            pki_cn,
            &offer_service_profile.domain,
            pki_root_issuer,
            certs_dir.as_path(),
        )?;

        let lnurl_client = Self::create_lnurl_client(
            lnurl_service_profile.clone(),
            pki_root_certificate_path.as_path(),
        )?;

        let discovery_client = Self::create_discovery_client(
            discovery_service_profile.clone(),
            pki_root_certificate_path.as_path(),
            discovery_authorization_token,
        )?;

        let offer_client = Self::create_offer_client(
            offer_service_profile.clone(),
            pki_root_certificate_path.as_path(),
            offer_authorization_token,
        )?;

        Ok(Self {
            id,
            config_path,
            discovery_store_dir,
            offer_store_dir,
            pki_root_certificate_path,
            server_process: None,
            exit_code: -1,
            stdout_buffer: Arc::new(Mutex::new(Vec::new())),
            stderr_buffer: Arc::new(Mutex::new(Vec::new())),
            offer_store_url: None,
            discovery_store_url: None,

            discovery_store_authorization: None,
            offer_store_authorization: None,

            lnurl_client,
            lnurl_probe: TcpProbe::new(lnurl_service_profile.address, Duration::from_millis(500)),

            discovery_client,
            discovery_probe: TcpProbe::new(
                discovery_service_profile.address,
                Duration::from_millis(500),
            ),

            offer_client,

            discovery_authority: discovery_authority_path,
            discovery_authorization,

            offer_authority: offer_authority_path,
            offer_authorization,

            offer_probe: TcpProbe::new(offer_service_profile.address, Duration::from_millis(500)),

            server_config_overrides: ServerConfigOverrides {
                lnurl_service: LnUrlBalancerServiceConfigOverride {
                    address: lnurl_service_profile.address,
                    tls: match lnurl_service_profile.protocol {
                        Protocol::Https => Some(TlsConfig {
                            cert_path: lnurl_certs_paths.certificate_path.clone(),
                            key_path: lnurl_certs_paths.key_path.clone(),
                        }),
                        Protocol::Http => None,
                    },
                    domain: lnurl_service_profile.domain,
                },
                discovery_service: DiscoveryServiceConfigOverride {
                    address: discovery_service_profile.address,
                    tls: match discovery_service_profile.protocol {
                        Protocol::Https => Some(TlsConfig {
                            cert_path: discovery_certs_paths.certificate_path.clone(),
                            key_path: discovery_certs_paths.key_path.clone(),
                        }),
                        Protocol::Http => None,
                    },
                    domain: discovery_service_profile.domain,
                },
                offers_service: OfferServiceConfigOverride {
                    address: offer_service_profile.address,
                    tls: match offer_service_profile.protocol {
                        Protocol::Https => Some(TlsConfig {
                            cert_path: offer_certs_paths.certificate_path.clone(),
                            key_path: offer_certs_paths.key_path.clone(),
                        }),
                        Protocol::Http => None,
                    },
                    domain: offer_service_profile.domain,
                },
            },
        })
    }

    fn create_lnurl_client(
        service_profile: ServiceProfile,
        root_certificate: &Path,
    ) -> anyhow::Result<LnUrlTestClient> {
        let url = Self::get_service_url(service_profile.clone());

        match service_profile.protocol {
            Protocol::Https => {
                let cert_data = std::fs::read(root_certificate)?;
                let cert = reqwest::Certificate::from_pem(&cert_data)?;

                Ok(LnUrlTestClient::create(
                    url.to_string(),
                    Duration::from_secs(10),
                    Duration::from_secs(10),
                    vec![cert],
                )?)
            }
            Protocol::Http => Ok(LnUrlTestClient::create(
                url.to_string(),
                Duration::from_secs(10),
                Duration::from_secs(10),
                vec![],
            )?),
        }
    }

    fn create_discovery_client(
        service_profile: ServiceProfile,
        root_certificate: &Path,
        authorization: String,
    ) -> anyhow::Result<HttpDiscoveryBackendStore> {
        let url = Self::get_service_url(service_profile.clone());

        match service_profile.protocol {
            Protocol::Https => {
                let cert_data = std::fs::read(root_certificate)?;
                let cert = reqwest::Certificate::from_pem(&cert_data)?;

                Ok(HttpDiscoveryBackendStore::create(
                    url.parse()?,
                    Duration::from_secs(10),
                    Duration::from_secs(10),
                    vec![cert],
                    authorization,
                )?)
            }
            Protocol::Http => Ok(HttpDiscoveryBackendStore::create(
                url.parse()?,
                Duration::from_secs(10),
                Duration::from_secs(10),
                vec![],
                authorization,
            )?),
        }
    }

    fn create_offer_client(
        service_profile: ServiceProfile,
        root_certificate: &Path,
        authorization: String,
    ) -> anyhow::Result<HttpOfferStore> {
        let url = Self::get_service_url(service_profile.clone());

        match service_profile.protocol {
            Protocol::Https => {
                let cert_data = std::fs::read(root_certificate)?;
                let cert = reqwest::Certificate::from_pem(&cert_data)?;

                Ok(HttpOfferStore::create(
                    url.parse()?,
                    Duration::from_secs(10),
                    Duration::from_secs(10),
                    vec![cert],
                    authorization,
                )?)
            }
            Protocol::Http => Ok(HttpOfferStore::create(
                url.parse()?,
                Duration::from_secs(10),
                Duration::from_secs(10),
                vec![],
                authorization,
            )?),
        }
    }

    pub fn set_discovery_store_url(&mut self, discovery_store_url: Option<String>) {
        self.discovery_store_url = discovery_store_url;
    }

    pub fn set_offer_store_url(&mut self, offer_store_url: Option<String>) {
        self.offer_store_url = offer_store_url;
    }

    pub fn get_service_profile(&self, service: Service) -> anyhow::Result<ServiceProfile> {
        let profile = match service {
            Service::LnUrl => {
                let config = &self.server_config_overrides.lnurl_service;

                ServiceProfile {
                    domain: config.domain.clone(),
                    protocol: if config.tls.is_some() {
                        Protocol::Https
                    } else {
                        Protocol::Http
                    },
                    address: config.address,
                }
            }
            Service::Discovery => {
                let config = &self.server_config_overrides.discovery_service;

                ServiceProfile {
                    domain: config.domain.clone(),
                    protocol: if config.tls.is_some() {
                        Protocol::Https
                    } else {
                        Protocol::Http
                    },
                    address: config.address,
                }
            }
            Service::Offer => {
                let config = &self.server_config_overrides.offers_service;

                ServiceProfile {
                    domain: config.domain.clone(),
                    protocol: if config.tls.is_some() {
                        Protocol::Https
                    } else {
                        Protocol::Http
                    },
                    address: config.address,
                }
            }
        };

        Ok(profile)
    }

    pub async fn start_server(
        &mut self,
        start_services: &[Service],
        log_level: log::Level,
    ) -> anyhow::Result<u32> {
        let rust_log = std::env::var("RUST_LOG")
            .unwrap_or_else(|_| "".to_string())
            .to_lowercase();
        let has_rust_log = !rust_log.is_empty();
        let start_services: Vec<String> = start_services.iter().map(|s| s.to_string()).collect();
        let start_services: Vec<&str> = start_services.iter().map(|s| s.as_str()).collect();

        if has_rust_log {
            println!("[STDOUT] Starting server with services: {start_services:?}",);
        }

        let config_path = &self.config_path;
        let binary_path = Self::get_binary_path();

        let rust_log = if has_rust_log {
            rust_log
        } else {
            log_level.to_string()
        };

        let lnurl_svc = &self.server_config_overrides.lnurl_service;
        let discovery_svc = &self.server_config_overrides.discovery_service;
        let offers_svc = &self.server_config_overrides.offers_service;

        let mut command = Command::new(&binary_path);
        command
            .arg("service")
            .arg("--config")
            .arg(config_path)
            .env("RUST_LOG", rust_log)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .env("OFFER_SERVICE_ADDRESS", offers_svc.address.to_string())
            .env(
                "DISCOVERY_STORE_HTTP_TRUSTED_ROOTS",
                &self.pki_root_certificate_path,
            )
            .env(
                "OFFER_STORE_HTTP_TRUSTED_ROOTS",
                &self.pki_root_certificate_path,
            )
            .env(
                "DISCOVERY_STORE_DATABASE_URL",
                format!(
                    "sqlite://{}?mode=rwc",
                    self.discovery_store_dir
                        .join("discovery.db")
                        .to_string_lossy()
                ),
            )
            .env(
                "OFFER_STORE_DATABASE_URL",
                format!(
                    "sqlite://{}?mode=rwc",
                    self.offer_store_dir.join("offers.db").to_string_lossy()
                ),
            )
            .env(
                "DISCOVERY_SERVICE_AUTH_AUTHORITY_PATH",
                &self.discovery_authority,
            )
            .env("OFFER_SERVICE_AUTH_AUTHORITY_PATH", &self.offer_authority)
            .env("LNURL_SERVICE_ADDRESS", lnurl_svc.address.to_string())
            .env("LNURL_SERVICE_ALLOWED_HOSTS", &lnurl_svc.domain)
            .env(
                "DISCOVERY_SERVICE_ADDRESS",
                discovery_svc.address.to_string(),
            );

        for arg in start_services {
            command.arg(arg);
        }

        if let Some(tls) = &lnurl_svc.tls {
            command.env("LNURL_SERVICE_TLS_CERT_PATH", &tls.cert_path);
            command.env("LNURL_SERVICE_TLS_KEY_PATH", &tls.key_path);
        }

        if let Some(tls) = &discovery_svc.tls {
            command.env("DISCOVERY_SERVICE_TLS_CERT_PATH", &tls.cert_path);
            command.env("DISCOVERY_SERVICE_TLS_KEY_PATH", &tls.key_path);
        }

        if let Some(tls) = &offers_svc.tls {
            command.env("OFFER_SERVICE_TLS_CERT_PATH", &tls.cert_path);
            command.env("OFFER_SERVICE_TLS_KEY_PATH", &tls.key_path);
        }

        if let Some(offer_url) = &self.offer_store_url {
            command.env("OFFER_STORE_HTTP_BASE_URL", offer_url);
        }
        if let Some(discovery_url) = &self.discovery_store_url {
            command.env("DISCOVERY_STORE_HTTP_BASE_URL", discovery_url);
        }
        if let Some(discovery_store_authorization) = &self.discovery_store_authorization {
            command.env(
                "DISCOVERY_STORE_HTTP_AUTHORIZATION",
                discovery_store_authorization,
            );
        }
        if let Some(offer_store_authorization) = &self.offer_store_authorization {
            command.env("OFFER_STORE_HTTP_AUTHORIZATION", offer_store_authorization);
        }
        if has_rust_log {
            println!("[STDOUT] Executing command: {command:?}");
            let lnurl_profile = self.get_service_profile(Service::LnUrl)?;
            let discovery_profile = self.get_service_profile(Service::Discovery)?;
            let offer_profile = self.get_service_profile(Service::Offer)?;

            println!(
                "[STDOUT] Ports - LNURL: {}, Discovery: {}, Offers: {}",
                lnurl_profile.address.port(),
                discovery_profile.address.port(),
                offer_profile.address.port(),
            );
        }

        let mut child = command.spawn()?;

        let pid = child.id();

        if has_rust_log {
            println!("[STDOUT] Server process started with PID: {pid}");
        }

        if let Some(stdout) = child.stdout.take() {
            use std::io::{BufRead, BufReader};
            use std::thread;

            let stdout_buffer = self.stdout_buffer.clone();

            thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines().map_while(Result::ok) {
                    if has_rust_log {
                        println!("[STDOUT:{pid}] {line}");
                    }
                    if let Ok(mut buffer) = stdout_buffer.lock() {
                        buffer.push(line);
                    }
                }
            });
        }

        if let Some(stderr) = child.stderr.take() {
            use std::io::{BufRead, BufReader};
            use std::thread;

            let stderr_buffer = self.stderr_buffer.clone();

            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    if has_rust_log {
                        println!("[STDERR:{pid}] {line}");
                    }
                    if let Ok(mut buffer) = stderr_buffer.lock() {
                        buffer.push(line);
                    }
                }
            });
        }

        self.server_process = Some(child);

        Ok(pid)
    }

    pub fn stop_server(&mut self) -> anyhow::Result<()> {
        self.signal_server(sysinfo::Signal::Term)
    }

    pub fn signal_server(&mut self, signal: sysinfo::Signal) -> anyhow::Result<()> {
        if let Some(ref mut process) = self.server_process {
            let pid = process.id();
            let mut system = sysinfo::System::new();
            system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
            if let Some(sys_process) = system.process(sysinfo::Pid::from_u32(pid)) {
                let _ = sys_process.kill_with(signal);
            }
            self.exit_code = process.wait()?.code().unwrap_or(-1);
        }
        self.server_process = None;
        Ok(())
    }

    fn get_binary_path() -> PathBuf {
        PathBuf::from(env!("CARGO_BIN_EXE_swgr"))
    }

    pub fn wait_exit_code(&mut self) -> anyhow::Result<i32> {
        let code = match &mut self.server_process {
            None => self.exit_code,
            Some(process) => process.wait()?.code().unwrap_or(-1),
        };
        Ok(code)
    }

    pub fn discovery_store_dir(&self) -> &PathBuf {
        &self.discovery_store_dir
    }

    pub fn offer_store_dir(&self) -> &PathBuf {
        &self.offer_store_dir
    }

    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    pub fn lnurl_client(&self) -> &LnUrlTestClient {
        &self.lnurl_client
    }

    pub fn discovery_client(&self) -> &HttpDiscoveryBackendStore {
        &self.discovery_client
    }

    pub fn offer_client(&self) -> &HttpOfferStore {
        &self.offer_client
    }

    pub fn lnurl_probe(&self) -> &TcpProbe {
        &self.lnurl_probe
    }

    pub fn discovery_probe(&self) -> &TcpProbe {
        &self.discovery_probe
    }

    pub fn offer_probe(&self) -> &TcpProbe {
        &self.offer_probe
    }

    pub fn stdout_buffer(&self) -> Arc<Mutex<Vec<String>>> {
        self.stdout_buffer.clone()
    }

    pub fn stderr_buffer(&self) -> Arc<Mutex<Vec<String>>> {
        self.stderr_buffer.clone()
    }

    pub fn has_server_process(&self) -> bool {
        self.server_process.is_some()
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    fn get_service_url(profile: ServiceProfile) -> String {
        format!(
            "{}://{}:{}",
            profile.protocol,
            profile.domain,
            profile.address.port(),
        )
    }

    pub fn set_discovery_store_authorization(
        &mut self,
        discovery_store_authorization: Option<PathBuf>,
    ) {
        self.discovery_store_authorization = discovery_store_authorization;
    }

    pub fn discovery_authorization(&self) -> &Path {
        &self.discovery_authorization
    }

    pub fn set_offer_store_authorization(&mut self, offer_store_authorization: Option<PathBuf>) {
        self.offer_store_authorization = offer_store_authorization;
    }

    pub fn offer_authorization(&self) -> &Path {
        &self.offer_authorization
    }
}
