use anyhow::Context;
use flate2::read::GzDecoder;
use std::fs;
use std::path::Path;
use tar::Archive;

pub mod db;
pub mod lightning;

pub fn download_credentials(credentials_dir: &Path, credentials_url: &str) -> anyhow::Result<()> {
    let download_path = credentials_dir.join("credentials.tar.gz");
    let response = ureq::get(credentials_url)
        .call()
        .with_context(|| format!("Downloading credentials from {}", credentials_url))?;

    let bytes = response
        .into_body()
        .read_to_vec()
        .with_context(|| format!("Downloading credentials from {}", credentials_url))?;

    fs::write(&download_path, &bytes)
        .with_context(|| format!("Downloading credentials from {}", credentials_url))?;

    let tar_gz = fs::File::open(&download_path)
        .with_context(|| format!("Downloading credentials from {}", credentials_url))?;

    let tar = GzDecoder::new(tar_gz);
    let mut archive = Archive::new(tar);
    archive
        .unpack(credentials_dir)
        .with_context(|| format!("Downloading credentials from {}", credentials_url))?;
    Ok(())
}
