use crate::secrets::SecretStore;
use rustls::SignatureScheme;
use rustls::client::ResolvesClientCert;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::sign::CertifiedKey;
use secrecy::ExposeSecret;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

pub struct ClientCertResolver {
    secrets: SecretStore,
    cert_secret_name: String,
    key_secret_name: String,
    crypto: Arc<CryptoProvider>,
    cache: Mutex<Option<CachedCertKey>>,
}

struct CachedCertKey {
    cert_hash: u64,
    key_hash: u64,
    certified: Arc<CertifiedKey>,
}

impl fmt::Debug for ClientCertResolver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientCertResolver")
            .field("cert_secret_name", &self.cert_secret_name)
            .field("key_secret_name", &self.key_secret_name)
            .finish_non_exhaustive()
    }
}

impl ClientCertResolver {
    pub fn new(
        secrets: SecretStore,
        cert_secret_name: impl Into<String>,
        key_secret_name: impl Into<String>,
        crypto: Arc<CryptoProvider>,
    ) -> Self {
        Self {
            secrets,
            cert_secret_name: cert_secret_name.into(),
            key_secret_name: key_secret_name.into(),
            crypto,
            cache: Mutex::new(None),
        }
    }

    fn load(&self) -> Option<Arc<CertifiedKey>> {
        let cert_secret = match self.secrets.get_bytes(&self.cert_secret_name) {
            Ok(c) => c,
            Err(err) => {
                tracing::warn!(
                    "client cert resolve failed for secret {}: {err}",
                    self.cert_secret_name
                );
                return None;
            }
        };
        let cert_bytes = cert_secret.expose_secret();

        let key_secret = match self.secrets.get_bytes(&self.key_secret_name) {
            Ok(k) => k,
            Err(err) => {
                tracing::warn!(
                    "client key resolve failed for secret {}: {err}",
                    self.key_secret_name
                );
                return None;
            }
        };
        let key_bytes = key_secret.expose_secret();

        let cert_hash = hash_bytes(cert_bytes);
        let key_hash = hash_bytes(key_bytes);

        {
            let cache = self.cache.lock().unwrap();
            if let Some(cached) = cache.as_ref()
                && cached.cert_hash == cert_hash
                && cached.key_hash == key_hash
            {
                return Some(cached.certified.clone());
            }
        }

        let cert_chain: Vec<CertificateDer<'static>> =
            match CertificateDer::pem_slice_iter(cert_bytes).collect::<Result<_, _>>() {
                Ok(v) => v,
                Err(err) => {
                    tracing::warn!(
                        "parsing client cert PEM from secret {}: {err}",
                        self.cert_secret_name
                    );
                    return None;
                }
            };
        if cert_chain.is_empty() {
            tracing::warn!(
                "client cert PEM from secret {} contained no certificates",
                self.cert_secret_name
            );
            return None;
        }
        let key_der = match PrivateKeyDer::from_pem_slice(key_bytes) {
            Ok(k) => k,
            Err(err) => {
                tracing::warn!(
                    "parsing client key PEM from secret {}: {err}",
                    self.key_secret_name
                );
                return None;
            }
        };
        let certified = match CertifiedKey::from_der(cert_chain, key_der, &self.crypto) {
            Ok(c) => Arc::new(c),
            Err(err) => {
                tracing::warn!("building client CertifiedKey: {err}");
                return None;
            }
        };

        let mut cache = self.cache.lock().unwrap();
        *cache = Some(CachedCertKey {
            cert_hash,
            key_hash,
            certified: certified.clone(),
        });
        Some(certified)
    }
}

impl ResolvesClientCert for ClientCertResolver {
    fn resolve(
        &self,
        _root_hint_subjects: &[&[u8]],
        _sigschemes: &[SignatureScheme],
    ) -> Option<Arc<CertifiedKey>> {
        self.load()
    }

    fn has_certs(&self) -> bool {
        true
    }
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}
