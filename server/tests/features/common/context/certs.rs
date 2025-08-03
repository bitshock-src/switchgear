use rand::distributions::{Distribution, Uniform};
use rcgen::{
    CertificateParams, DistinguishedName, IsCa, Issuer, KeyPair, KeyUsagePurpose, SanType,
};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CertificateKeyPathPairPath {
    pub certificate_path: PathBuf,
    pub key_path: PathBuf,
}

pub fn gen_root_cert<'a>(
    base_path: &Path,
) -> anyhow::Result<(String, Issuer<'a, KeyPair>, CertificateKeyPathPairPath)> {
    let cn = gen_id(20);
    let cn = format!("{cn}.com");
    let mut params = CertificateParams::new(vec![cn.clone()])?;
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(rcgen::DnType::CommonName, cn.clone());
    distinguished_name.push(rcgen::DnType::OrganizationName, cn.clone());
    params.distinguished_name = distinguished_name;
    params.is_ca = IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let root_key = KeyPair::generate()?;
    let root_cert = params.self_signed(&root_key)?;
    let certificate_path = base_path.join(format!("{cn}-cert.pem"));
    let key_path = base_path.join(format!("{cn}-key.pem"));
    fs::write(&certificate_path, root_cert.pem())?;
    fs::write(&key_path, root_key.serialize_pem())?;
    let issuer = Issuer::new(params, root_key);
    Ok((
        cn,
        issuer,
        CertificateKeyPathPairPath {
            certificate_path,
            key_path,
        },
    ))
}

pub fn gen_server_cert(
    root_cn: &str,
    cn: &str,
    issuer: &Issuer<'_, impl rcgen::SigningKey>,
    base_path: &Path,
) -> anyhow::Result<CertificateKeyPathPairPath> {
    let mut leaf_params = CertificateParams::new(vec![cn.to_string()])?;
    leaf_params.subject_alt_names = vec![SanType::DnsName(cn.parse()?)];
    let mut leaf_distinguished_name = DistinguishedName::new();
    leaf_distinguished_name.push(rcgen::DnType::CommonName, cn);
    leaf_params.distinguished_name = leaf_distinguished_name;
    leaf_params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    let server_key = KeyPair::generate()?;
    let server_cert = leaf_params.signed_by(&server_key, issuer)?;
    let certificate_path = base_path.join(format!("{cn}-{root_cn}-cert.pem"));
    let key_path = base_path.join(format!("{cn}-{root_cn}-key.pem"));
    fs::write(&certificate_path, server_cert.pem())?;
    fs::write(&key_path, server_key.serialize_pem())?;
    Ok(CertificateKeyPathPairPath {
        certificate_path,
        key_path,
    })
}

fn gen_id(length: usize) -> String {
    const ALPHANUMERIC: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-";
    let mut rng = rand::thread_rng();
    let start_char_dist = Uniform::from(0..ALPHANUMERIC.len());
    let mut id = String::with_capacity(length);
    id.push(ALPHANUMERIC[start_char_dist.sample(&mut rng)] as char);
    if length > 1 {
        let charset_dist = Uniform::from(0..CHARSET.len());
        for _ in 1..length - 1 {
            id.push(CHARSET[charset_dist.sample(&mut rng)] as char);
        }

        let end_char_dist = Uniform::from(0..ALPHANUMERIC.len());
        id.push(ALPHANUMERIC[end_char_dist.sample(&mut rng)] as char);
    }
    id
}
