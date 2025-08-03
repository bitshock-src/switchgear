use crate::commands::cli_write_all;
use anyhow::Context;
use clap::Subcommand;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use p256::ecdsa::SigningKey;
use pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};
use rand::thread_rng;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Subcommand, Debug)]
pub enum TokenCommands {
    /// Mint service token
    Mint {
        /// Path to ECDSA prime256v1 private key in pkcs8 format, pem encoded
        #[clap(short, long, value_parser)]
        key: PathBuf,

        /// Expires, seconds from now
        #[arg(short, long)]
        expires: u64,

        /// Optional token output path, defaults to stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Generate ECDSA prime256v1 key pair in pkcs8 format, pem encoded
    Key {
        /// Public key output path
        #[clap(short, long, value_parser)]
        public: PathBuf,

        /// Private key output path
        #[clap(short = 'k', long, value_parser)]
        private: PathBuf,
    },
    /// Mint service token with a new ECDSA prime256v1 key. Key will be pkcs8 format, pem encoded
    MintAll {
        /// Public key output path
        #[clap(short, long, value_parser)]
        public: PathBuf,

        /// Private key output path
        #[clap(short = 'k', long, value_parser)]
        private: PathBuf,

        /// Expires, seconds from now
        #[arg(short, long)]
        expires: u64,

        /// Optional token output path, defaults to stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Verify service token
    Verify {
        /// Path to ECDSA prime256v1 public key in pkcs8 format, pem encoded
        #[clap(short, long, value_parser)]
        public: PathBuf,

        /// Optional token input path, defaults to stdin
        #[clap(short, long)]
        token: Option<PathBuf>,

        /// Optional token output path, defaults to stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

pub fn mint<T: Serialize>(
    encoding_key_path: &Path,
    output: Option<&Path>,
    token: T,
) -> anyhow::Result<()> {
    let encoding_key = std::fs::read(encoding_key_path).with_context(|| {
        format!(
            "reading private key: {}",
            encoding_key_path.to_string_lossy()
        )
    })?;
    let encoding_key = EncodingKey::from_ec_pem(&encoding_key).with_context(|| {
        format!(
            "parsing private key: {}",
            encoding_key_path.to_string_lossy()
        )
    })?;

    let header = Header::new(Algorithm::ES256);
    let token = encode(&header, &token, &encoding_key).with_context(|| {
        format!(
            "encoding token with private key: {}",
            encoding_key_path.to_string_lossy()
        )
    })?;

    cli_write_all(output, token.as_bytes()).with_context(|| {
        format!(
            "writing token to: {}",
            output.map_or_else(|| "stdout".to_string(), |p| p.to_string_lossy().to_string())
        )
    })?;

    Ok(())
}

pub fn ecdsa_prime256v1_pkcs8_pem_keypair() -> anyhow::Result<(String, String)> {
    let mut rng = thread_rng();
    let discovery_key_pair = SigningKey::random(&mut rng);

    let public_key = *discovery_key_pair.verifying_key();
    let public_key = public_key.to_public_key_pem(LineEnding::default())?;

    let private_key = discovery_key_pair;
    let private_key = private_key.to_pkcs8_pem(LineEnding::default())?;

    Ok((public_key, private_key.to_string()))
}

pub fn key_pair_io(
    public_key: &str,
    private_key: &str,
    public_path: &Path,
    private_path: &Path,
) -> anyhow::Result<()> {
    fs::write(public_path, public_key.as_bytes())
        .with_context(|| format!("writing public key to: {}", public_path.to_string_lossy()))?;
    fs::write(private_path, private_key.as_bytes())
        .with_context(|| format!("writing private key to: {}", private_path.to_string_lossy()))?;
    Ok(())
}

pub fn key(public_path: &Path, private_path: &Path) -> anyhow::Result<()> {
    let (public_key, private_key) = ecdsa_prime256v1_pkcs8_pem_keypair()?;
    key_pair_io(&public_key, &private_key, public_path, private_path)?;
    Ok(())
}
