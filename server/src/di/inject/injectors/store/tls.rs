use anyhow::Context;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::CertificateDer;
use std::path::Path;

pub fn load_server_certificate(
    server_certificate_paths: Option<&Path>,
) -> anyhow::Result<Vec<CertificateDer<'_>>> {
    let certificates = if let Some(server_certificate_paths) = server_certificate_paths {
        CertificateDer::pem_file_iter(server_certificate_paths)
            .with_context(|| {
                format!(
                    "parsing root certificate: {}",
                    server_certificate_paths.display()
                )
            })?
            .collect::<Result<Vec<_>, _>>()
            .with_context(|| {
                format!(
                    "parsing root certificate: {}",
                    server_certificate_paths.display()
                )
            })?
    } else {
        vec![]
    };

    Ok(certificates)
}
