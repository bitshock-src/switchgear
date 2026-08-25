use crate::config::TlsConfig;
use crate::di::error::DiError;
use axum_server::tls_rustls::RustlsConfig;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use rustls_pemfile::private_key;
use std::fs::File;
use std::io::BufReader;
use switchgear_error::ForeignContext;

pub fn load_server_x509_credentials(tls_config: &TlsConfig) -> Result<RustlsConfig, DiError> {
    let cert_chain = CertificateDer::pem_file_iter(&tls_config.cert_path)
        .with_foreign_context(
            || {
                format!(
                    "parsing root certificate: {}",
                    tls_config.cert_path.display()
                )
            },
            None,
        )?
        .collect::<Result<Vec<_>, _>>()
        .with_foreign_context(
            || {
                format!(
                    "parsing root certificate: {}",
                    tls_config.cert_path.display()
                )
            },
            None,
        )?;
    let key_file = File::open(&tls_config.key_path).with_foreign_context(
        || format!("opening key file: {}", tls_config.key_path.display()),
        None,
    )?;
    let key_der = private_key(&mut BufReader::new(key_file))
        .with_foreign_context(
            || format!("parsing key file: {}", tls_config.key_path.display()),
            None,
        )?
        .ok_or_else(|| DiError::internal("no private key found in key file"))?;

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, key_der)
        .foreign_context("building rustls server config", None)?;

    Ok(RustlsConfig::from_config(config.into()))
}
