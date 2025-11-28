use crate::config::TlsConfig;
use anyhow::Context;
use axum_server::tls_rustls::RustlsConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::CertificateDer;
use rustls_pemfile::private_key;
use std::fs::File;
use std::io::BufReader;

pub fn load_server_x509_credentials(tls_config: &TlsConfig) -> anyhow::Result<RustlsConfig> {
    let cert_chain = CertificateDer::pem_file_iter(&tls_config.cert_path)
        .with_context(|| {
            format!(
                "parsing root certificate: {}",
                tls_config.cert_path.display()
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| {
            format!(
                "parsing root certificate: {}",
                tls_config.cert_path.display()
            )
        })?;
    let key_der = private_key(&mut BufReader::new(File::open(&tls_config.key_path)?))?
        .ok_or_else(|| anyhow::anyhow!("no private key found in key file"))?;

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key_der)?;

    Ok(RustlsConfig::from_config(config.into()))
}
