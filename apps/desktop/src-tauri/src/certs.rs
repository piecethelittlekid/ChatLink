use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use sha2::{Digest, Sha256};
use std::{net::Ipv4Addr, path::Path};

pub struct CertificateBundle {
    pub certificate_der: Vec<u8>,
    pub private_key_der: Vec<u8>,
    pub root_certificate_der: Vec<u8>,
    pub fingerprint: String,
}

pub fn root_fingerprint(root_der: &[u8]) -> String {
    Sha256::digest(root_der)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

pub fn create_or_load_ca(data_dir: &Path) -> Result<Vec<u8>> {
    std::fs::create_dir_all(data_dir).context("create certificate directory")?;
    let cert_path = data_dir.join("chatlink-root-ca.der");
    let key_path = data_dir.join("chatlink-root-ca.key.dpapi");
    if cert_path.exists() && key_path.exists() {
        return Ok(std::fs::read(cert_path).context("read local root certificate")?);
    }

    let key = KeyPair::generate().context("generate local CA key")?;
    let params = ca_params();
    let cert = params.self_signed(&key).context("self-sign local CA")?;
    let key_pem = key.serialize_pem();
    let protected_key = protect_key(key_pem.as_bytes())?;
    write_private_file(&key_path, &protected_key)?;
    std::fs::write(&cert_path, cert.der().as_ref()).context("write local root certificate")?;
    Ok(cert.der().as_ref().to_vec())
}

pub fn issue_server_certificate(data_dir: &Path, ip: Ipv4Addr) -> Result<CertificateBundle> {
    let root_der = create_or_load_ca(data_dir)?;
    let protected_key = std::fs::read(data_dir.join("chatlink-root-ca.key.dpapi"))
        .context("read protected CA key")?;
    let key_pem = unprotect_key(&protected_key)?;
    let key_text = std::str::from_utf8(&key_pem).context("decode CA private key")?;
    let ca_key = KeyPair::from_pem(key_text).context("load CA private key")?;
    let issuer = Issuer::new(ca_params(), ca_key);

    let leaf_key = KeyPair::generate().context("generate server key")?;
    let mut leaf_params = CertificateParams::new(vec![ip.to_string()])
        .context("create server certificate parameters")?;
    leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    leaf_params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    let leaf = leaf_params
        .signed_by(&leaf_key, &issuer)
        .context("sign server certificate")?;

    let fingerprint = root_fingerprint(&root_der);
    Ok(CertificateBundle {
        certificate_der: leaf.der().as_ref().to_vec(),
        private_key_der: leaf_key.serialize_der(),
        root_certificate_der: root_der,
        fingerprint,
    })
}

fn ca_params() -> CertificateParams {
    let mut params = CertificateParams::default();
    params.distinguished_name.push(DnType::CommonName, "ChatLink Local CA");
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params
}

pub fn mobileconfig(root_der: &[u8]) -> String {
    let payload = STANDARD.encode(root_der);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>PayloadContent</key><array><dict>
    <key>PayloadCertificateFileName</key><string>ChatLink-Root-CA.cer</string>
    <key>PayloadContent</key><data>{payload}</data>
    <key>PayloadDescription</key><string>Trust the ChatLink local HTTPS certificate authority.</string>
    <key>PayloadDisplayName</key><string>ChatLink Local CA</string>
    <key>PayloadIdentifier</key><string>com.chatlink.local-ca</string>
    <key>PayloadType</key><string>com.apple.security.root</string>
    <key>PayloadUUID</key><string>6f5c6d53-2e6f-4e8a-8f76-15830e64cc11</string>
    <key>PayloadVersion</key><integer>1</integer>
  </dict></array>
  <key>PayloadDisplayName</key><string>ChatLink Certificate</string>
  <key>PayloadIdentifier</key><string>com.chatlink.certificate-profile</string>
  <key>PayloadRemovalDisallowed</key><false/>
  <key>PayloadType</key><string>Configuration</string>
  <key>PayloadUUID</key><string>e1144b7e-dd95-4f6f-954a-f13f5b65b2ca</string>
  <key>PayloadVersion</key><integer>1</integer>
</dict></plist>"#
    )
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use std::fs::OpenOptions;
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .share_mode(0)
            .open(path)
            .with_context(|| format!("create protected key at {}", path.display()))?;
        std::io::Write::write_all(&mut file, bytes).context("write protected key")?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::fs::write(path, bytes).context("write protected key")
    }
}

#[cfg(target_os = "windows")]
fn protect_key(bytes: &[u8]) -> Result<Vec<u8>> {
    use windows_dpapi::{encrypt_data, Scope};
    encrypt_data(bytes, Scope::User, None).context("protect CA private key with Windows DPAPI")
}

#[cfg(not(target_os = "windows"))]
fn protect_key(_bytes: &[u8]) -> Result<Vec<u8>> {
    anyhow::bail!("ChatLink certificate storage is supported only on Windows")
}

#[cfg(target_os = "windows")]
fn unprotect_key(bytes: &[u8]) -> Result<Vec<u8>> {
    use windows_dpapi::{decrypt_data, Scope};
    Ok(decrypt_data(bytes, Scope::User, None)
        .context("unprotect CA private key with Windows DPAPI")?
        .to_vec())
}

#[cfg(not(target_os = "windows"))]
fn unprotect_key(_bytes: &[u8]) -> Result<Vec<u8>> {
    anyhow::bail!("ChatLink certificate storage is supported only on Windows")
}
