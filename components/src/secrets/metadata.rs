use crate::secrets::SecretStore;
use secrecy::ExposeSecret;
use tonic::metadata::MetadataValue;
use tonic::service::Interceptor;

#[derive(Clone, Copy, Debug)]
enum PayloadEncoding {
    Utf8,
    HexOfBytes,
}

#[derive(Clone)]
pub struct SecretHeaderInterceptor {
    secrets: SecretStore,
    header_name: &'static str,
    secret_name: String,
    value_prefix: &'static str,
    encoding: PayloadEncoding,
}

impl SecretHeaderInterceptor {
    fn new_utf8(
        secrets: SecretStore,
        header_name: &'static str,
        secret_name: impl Into<String>,
        value_prefix: &'static str,
    ) -> Self {
        Self {
            secrets,
            header_name,
            secret_name: secret_name.into(),
            value_prefix,
            encoding: PayloadEncoding::Utf8,
        }
    }

    pub fn bearer(secrets: SecretStore, secret_name: impl Into<String>) -> Self {
        Self::new_utf8(secrets, "authorization", secret_name, "Bearer ")
    }

    pub fn macaroon_hex(secrets: SecretStore, secret_name: impl Into<String>) -> Self {
        Self {
            secrets,
            header_name: "macaroon",
            secret_name: secret_name.into(),
            value_prefix: "",
            encoding: PayloadEncoding::HexOfBytes,
        }
    }
}

impl Interceptor for SecretHeaderInterceptor {
    fn call(&mut self, mut req: tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> {
        let raw = match self.encoding {
            PayloadEncoding::Utf8 => {
                let secret = self.secrets.get_str(&self.secret_name).map_err(|e| {
                    tonic::Status::internal(format!(
                        "resolving {} secret {}: {e}",
                        self.header_name, self.secret_name
                    ))
                })?;
                let plain = secret.expose_secret();
                if self.value_prefix.is_empty() {
                    plain.to_owned()
                } else {
                    let mut buf = String::with_capacity(self.value_prefix.len() + plain.len());
                    buf.push_str(self.value_prefix);
                    buf.push_str(plain);
                    buf
                }
            }
            PayloadEncoding::HexOfBytes => {
                let secret = self.secrets.get_bytes(&self.secret_name).map_err(|e| {
                    tonic::Status::internal(format!(
                        "resolving {} secret {}: {e}",
                        self.header_name, self.secret_name
                    ))
                })?;
                hex::encode(secret.expose_secret())
            }
        };

        let mut value: MetadataValue<_> = raw.parse().map_err(|e| {
            tonic::Status::internal(format!("parsing {} metadata value: {e}", self.header_name))
        })?;
        value.set_sensitive(true);
        req.metadata_mut().insert(self.header_name, value);
        Ok(req)
    }
}
