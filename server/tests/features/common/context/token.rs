use crate::common::context::Service;
use anyhow::Context;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn generate_service_token(
    service: Service,
    expires: u64,
    public_key_output_path: &Path,
    private_key_output_path: &Path,
    token_output_path: &Path,
) -> anyhow::Result<String> {
    match service {
        Service::Discovery | Service::Offer => {}
        _ => return Err(anyhow::anyhow!("invalid service")),
    }

    let binary_path = PathBuf::from(env!("CARGO_BIN_EXE_swgr"));

    let status = Command::new(&binary_path)
        .arg(service.to_string())
        .arg("token")
        .arg("mint-all")
        .arg("-e")
        .arg(expires.to_string())
        .arg("-p")
        .arg(public_key_output_path)
        .arg("-k")
        .arg(private_key_output_path)
        .arg("-o")
        .arg(token_output_path)
        .status()?;

    if !status.success() {
        anyhow::bail!("failed to get token: {status:?}");
    }

    let token = std::fs::read(token_output_path).with_context(|| {
        format!(
            "reading generated token from {}",
            token_output_path.to_string_lossy()
        )
    })?;

    let token = String::from_utf8(token).with_context(|| {
        format!(
            "parsing generated token into string from {}",
            token_output_path.to_string_lossy()
        )
    })?;

    Ok(token)
}
