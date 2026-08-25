use crate::commands::error::CliError;
use crate::commands::token::{ecdsa_prime256v1_pkcs8_pem_keypair, key_pair_io};
use crate::commands::{cli_read_to_string, cli_write_all};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, encode};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use switchgear_error::{ErrorOrigin, ForeignContext};
use switchgear_service::{DiscoveryAudience, DiscoveryBearerTokenValidator, DiscoveryClaims};

fn token(expires: u64) -> Result<DiscoveryClaims, CliError> {
    Ok(DiscoveryClaims {
        aud: DiscoveryAudience::Discovery,
        exp: (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .foreign_context("system clock has invalid time", None)?
            .as_secs()
            + expires) as usize,
    })
}

pub fn mint(encoding_key_path: &Path, expires: u64, output: Option<&Path>) -> Result<(), CliError> {
    crate::commands::token::mint(encoding_key_path, output, token(expires)?)
}

pub fn mint_all(
    public: &Path,
    private: &Path,
    expires: u64,
    output: Option<&Path>,
) -> Result<(), CliError> {
    let (public_key, private_key) = ecdsa_prime256v1_pkcs8_pem_keypair()?;

    let encoding_key = EncodingKey::from_ec_pem(private_key.as_bytes())
        .foreign_context("parsing minted private key", ErrorOrigin::Internal)?;
    let header = Header::new(Algorithm::ES256);
    let token = encode(&header, &token(expires)?, &encoding_key).foreign_context(
        "encoding token with minted private key",
        ErrorOrigin::Internal,
    )?;

    key_pair_io(&public_key, &private_key, public, private)?;
    cli_write_all(output, token.as_bytes()).with_foreign_context(
        || {
            format!(
                "writing token to: {}",
                output.map_or_else(|| "stdout".to_string(), |p| p.to_string_lossy().to_string())
            )
        },
        None,
    )?;

    Ok(())
}

pub fn verify(
    public_key_path: &Path,
    token_path: Option<&Path>,
    output: Option<&Path>,
) -> Result<(), CliError> {
    let public_key = std::fs::read(public_key_path).with_foreign_context(
        || format!("reading public key: {}", public_key_path.to_string_lossy()),
        None,
    )?;

    let public_key = DecodingKey::from_ec_pem(&public_key).with_foreign_context(
        || format!("decoding public key: {}", public_key_path.to_string_lossy()),
        None,
    )?;

    let mut token = String::new();
    cli_read_to_string(token_path, &mut token).with_foreign_context(
        || {
            format!(
                "reading token: {}",
                token_path.map_or_else(|| "stdin".to_string(), |p| p.to_string_lossy().to_string())
            )
        },
        None,
    )?;

    let validator = DiscoveryBearerTokenValidator::new(public_key);
    let token = validator
        .validate_token(&token)
        .foreign_context("validating discovery token", None)?;
    let token = serde_json::to_string_pretty(&token)
        .foreign_context("serializing discovery token", ErrorOrigin::Internal)?;

    cli_write_all(output, token.as_bytes()).with_foreign_context(
        || {
            format!(
                "writing token to: {}",
                output.map_or_else(|| "stdout".to_string(), |p| p.to_string_lossy().to_string())
            )
        },
        None,
    )?;

    Ok(())
}
