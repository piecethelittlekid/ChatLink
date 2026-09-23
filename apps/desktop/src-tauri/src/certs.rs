use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, SanType,
};
use sha2::{Digest, Sha256};
use std::{
    net::{IpAddr, Ipv4Addr},
    path::Path,
};
use time::{Duration, OffsetDateTime};

pub struct CertificateBundle {
    pub certificate_der: Vec<u8>,
    pub private_key_der: Vec<u8>,
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
    anyhow::ensure!(
        cert_path.exists() == key_path.exists(),
        "CA certificate and protected key are incomplete; restore both files from the same backup"
    );
    if cert_path.exists() {
        let cert_der = std::fs::read(cert_path).context("read local root certificate")?;
        let protected = std::fs::read(key_path).context("read protected CA key")?;
        let plain = unprotect_key(&protected)?;
        let key_text = std::str::from_utf8(&plain).context("decode CA private key")?;
        let key = KeyPair::from_pem(key_text).context("load CA private key")?;
        validate_ca_pair(&cert_der, &key)?;
        return Ok(cert_der);
    }

    let key = KeyPair::generate().context("generate local CA key")?;
    let params = ca_params();
    let cert = params.self_signed(&key).context("self-sign local CA")?;
    validate_ca_pair(cert.der().as_ref(), &key)?;
    let key_pem = key.serialize_pem();
    let protected_key = protect_key(key_pem.as_bytes())?;
    write_private_file(&key_path, &protected_key)?;
    std::fs::write(&cert_path, cert.der().as_ref()).context("write local root certificate")?;
    Ok(cert.der().as_ref().to_vec())
}

fn validate_ca_pair(cert_der: &[u8], key: &KeyPair) -> Result<()> {
    let (remaining, certificate) = x509_parser::parse_x509_certificate(cert_der)
        .map_err(|_| anyhow::anyhow!("CA certificate is invalid"))?;
    anyhow::ensure!(
        remaining.is_empty(),
        "CA certificate contains unexpected data"
    );
    anyhow::ensure!(
        certificate.tbs_certificate.is_ca(),
        "CA certificate is not a CA"
    );
    anyhow::ensure!(
        certificate
            .tbs_certificate
            .subject_pki
            .subject_public_key
            .data
            .as_ref()
            == key.public_key_raw(),
        "CA certificate does not match its protected key"
    );
    Ok(())
}

pub fn issue_server_certificate(data_dir: &Path, ip: Ipv4Addr) -> Result<CertificateBundle> {
    let _ = create_or_load_ca(data_dir)?;
    let protected_key = std::fs::read(data_dir.join("chatlink-root-ca.key.dpapi"))
        .context("read protected CA key")?;
    let key_pem = unprotect_key(&protected_key)?;
    let key_text = std::str::from_utf8(&key_pem).context("decode CA private key")?;
    let ca_key = KeyPair::from_pem(key_text).context("load CA private key")?;
    let issuer = Issuer::new(ca_params(), ca_key);

    let leaf_key = KeyPair::generate().context("generate server key")?;
    let mut leaf_params = CertificateParams::new(Vec::<String>::new())
        .context("create server certificate parameters")?;
    leaf_params
        .subject_alt_names
        .push(SanType::IpAddress(IpAddr::V4(ip)));
    let now = OffsetDateTime::now_utc();
    leaf_params.not_before = now - Duration::days(1);
    leaf_params.not_after = now + Duration::days(397);
    leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    leaf_params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    let leaf = leaf_params
        .signed_by(&leaf_key, &issuer)
        .context("sign server certificate")?;

    Ok(CertificateBundle {
        certificate_der: leaf.der().as_ref().to_vec(),
        private_key_der: leaf_key.serialize_der(),
    })
}

fn ca_params() -> CertificateParams {
    let mut params = CertificateParams::default();
    let now = OffsetDateTime::now_utc();
    params.not_before = now - Duration::days(1);
    params.not_after = now + Duration::days(3650);
    params
        .distinguished_name
        .push(DnType::CommonName, "ChatLink Local CA");
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
        use std::fs::OpenOptions;
        use std::os::windows::fs::OpenOptionsExt;
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

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use x509_parser::extensions::GeneralName;

    #[test]
    fn leaf_contains_ip_san_and_corrupt_ca_is_rejected() -> Result<()> {
        let directory =
            std::env::temp_dir().join(format!("chatlink-cert-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory)?;
        let result = (|| -> Result<()> {
            let root = create_or_load_ca(&directory)?;
            assert_eq!(create_or_load_ca(&directory)?, root);
            let ip = Ipv4Addr::new(192, 168, 50, 7);
            let leaf = issue_server_certificate(&directory, ip)?;
            let (_, parsed) = x509_parser::parse_x509_certificate(&leaf.certificate_der)
                .map_err(|error| anyhow::anyhow!("parse leaf certificate: {error:?}"))?;
            let san = parsed
                .subject_alternative_name()?
                .context("leaf missing SAN")?;
            assert!(san.value.general_names.iter().any(|name| {
                matches!(name, GeneralName::IPAddress(bytes) if bytes.iter().copied().eq(ip.octets()))
            }));
            std::fs::write(directory.join("chatlink-root-ca.der"), [0_u8, 1, 2])?;
            assert!(create_or_load_ca(&directory).is_err());
            Ok(())
        })();
        if let (Ok(resolved), Ok(root)) = (
            directory.canonicalize(),
            std::env::temp_dir().canonicalize(),
        ) {
            if resolved.starts_with(root)
                && resolved
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("chatlink-cert-test-"))
            {
                let _ = std::fs::remove_dir_all(&resolved);
            }
        }
        result
    }
}
