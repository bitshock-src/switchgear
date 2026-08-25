use crate::di::error::DiError;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use std::path::Path;
use switchgear_error::ForeignContext;

pub fn load_server_certificate(
    server_certificate_paths: Option<&Path>,
) -> Result<Vec<CertificateDer<'_>>, DiError> {
    let certificates = if let Some(server_certificate_paths) = server_certificate_paths {
        CertificateDer::pem_file_iter(server_certificate_paths)
            .with_foreign_context(
                || {
                    format!(
                        "parsing root certificate: {}",
                        server_certificate_paths.display()
                    )
                },
                None,
            )?
            .collect::<Result<Vec<_>, _>>()
            .with_foreign_context(
                || {
                    format!(
                        "parsing root certificate: {}",
                        server_certificate_paths.display()
                    )
                },
                None,
            )?
    } else {
        vec![]
    };

    Ok(certificates)
}
