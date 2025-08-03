use crate::config::TlsConfig;
use axum_server::tls_rustls::RustlsConfig;
use rustls_pemfile::{certs, private_key};
use std::fs::File;
use std::io::BufReader;

pub fn load_server_x509_credentials(tls_config: &TlsConfig) -> anyhow::Result<RustlsConfig> {
    let cert_chain = certs(&mut BufReader::new(File::open(&tls_config.cert_path)?))
        .filter_map(Result::ok)
        .collect();
    let key_der = private_key(&mut BufReader::new(File::open(&tls_config.key_path)?))?
        .ok_or_else(|| anyhow::anyhow!("no private key found in key file"))?;

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key_der)?;

    Ok(RustlsConfig::from_config(config.into()))
}
