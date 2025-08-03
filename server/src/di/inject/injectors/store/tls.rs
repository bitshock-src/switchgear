use anyhow::{anyhow, Context};
use reqwest::Certificate;
use rustls_pemfile::certs;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

pub fn load_server_certificate(
    server_certificate_paths: &[PathBuf],
) -> anyhow::Result<Vec<Certificate>> {
    let mut server_certificates = Vec::new();
    for server_certificate_path in server_certificate_paths {
        let server_certificate = certs(&mut BufReader::new(File::open(server_certificate_path)?))
            .filter_map(Result::ok)
            .next()
            .ok_or_else(|| {
                anyhow!(format!(
                    "no certificate found in {}",
                    server_certificate_path.display()
                ))
            })?;

        let server_certificate =
            reqwest::Certificate::from_der(&server_certificate).with_context(|| {
                format!(
                    "loading certificate from {}",
                    server_certificate_path.display()
                )
            })?;
        server_certificates.push(server_certificate);
    }
    Ok(server_certificates)
}
