//! 为已授权的精确 Art 执行合同签名；不读取包、不授设备权限、不把发布者密钥默认为授权签发者。
use crate::{sign_message, verify_message, PluginSecurityError, SigningKeyDocument};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use loom_protocol::local_art::{Envelope, Grant, DOMAIN, MAX_ENVELOPE, MAX_PAYLOAD};

fn invalid() -> PluginSecurityError {
    PluginSecurityError::InvalidKey("Invalid local Art execution grant".into())
}

pub fn issue(key: &SigningKeyDocument, grant: &Grant) -> Result<Vec<u8>, PluginSecurityError> {
    grant.validate().map_err(|_| invalid())?;
    issue_payload(key, grant, DOMAIN)
}
fn issue_payload(
    key: &SigningKeyDocument,
    grant: &impl serde::Serialize,
    domain: &[u8],
) -> Result<Vec<u8>, PluginSecurityError> {
    if key.key_id.is_empty() || key.key_id.len() > 128 {
        return Err(invalid());
    }
    let payload = serde_json::to_vec(grant)?;
    if payload.len() > MAX_PAYLOAD {
        return Err(invalid());
    }
    let message = [domain, payload.as_slice()].concat();
    let envelope = Envelope {
        algorithm: "ed25519".into(),
        key_id: key.key_id.clone(),
        payload: BASE64.encode(&payload),
        signature: sign_message(key, &message)?,
    };
    let bytes = serde_json::to_vec(&envelope)?;
    if bytes.len() > MAX_ENVELOPE {
        return Err(invalid());
    }
    Ok(bytes)
}

/// issuer key 来自调用者的独立信任配置；绝不使用 envelope 中附送的公钥建立信任。
pub fn verify(bytes: &[u8], key_id: &str, public_key: &str) -> Result<Grant, PluginSecurityError> {
    let payload = verify_payload(bytes, key_id, public_key, DOMAIN)?;
    let grant: Grant = serde_json::from_slice(&payload)?;
    grant.validate().map_err(|_| invalid())?;
    Ok(grant)
}
fn verify_payload(
    bytes: &[u8],
    key_id: &str,
    public_key: &str,
    domain: &[u8],
) -> Result<Vec<u8>, PluginSecurityError> {
    if bytes.len() > MAX_ENVELOPE {
        return Err(invalid());
    }
    let envelope: Envelope = serde_json::from_slice(bytes)?;
    if envelope.algorithm != "ed25519" || envelope.key_id != key_id {
        return Err(invalid());
    }
    let payload = BASE64.decode(&envelope.payload)?;
    if payload.len() > MAX_PAYLOAD {
        return Err(invalid());
    }
    verify_message(
        public_key,
        &[domain, &payload].concat(),
        &envelope.signature,
    )?;
    Ok(payload)
}

pub fn issue_resume(
    key: &SigningKeyDocument,
    resume: &loom_protocol::local_art::resume::Resume,
) -> Result<Vec<u8>, PluginSecurityError> {
    resume.validate().map_err(|_| invalid())?;
    issue_payload(key, resume, loom_protocol::local_art::resume::DOMAIN)
}
pub fn issue_package(
    key: &SigningKeyDocument,
    package: &loom_protocol::local_art::package::Package,
) -> Result<Vec<u8>, PluginSecurityError> {
    package.validate().map_err(|_| invalid())?;
    issue_payload(key, package, loom_protocol::local_art::package::DOMAIN)
}
pub fn verify_package(
    bytes: &[u8],
    key_id: &str,
    public_key: &str,
) -> Result<loom_protocol::local_art::package::Package, PluginSecurityError> {
    let payload = verify_payload(
        bytes,
        key_id,
        public_key,
        loom_protocol::local_art::package::DOMAIN,
    )?;
    let package: loom_protocol::local_art::package::Package = serde_json::from_slice(&payload)?;
    package.validate().map_err(|_| invalid())?;
    Ok(package)
}
pub fn issue_port_package(
    key: &SigningKeyDocument,
    package: &loom_protocol::local_art::package::ports::Package,
) -> Result<Vec<u8>, PluginSecurityError> {
    package.validate().map_err(|_| invalid())?;
    issue_payload(
        key,
        package,
        loom_protocol::local_art::package::ports::DOMAIN,
    )
}
pub fn issue_visibility_package(
    key: &SigningKeyDocument,
    package: &loom_protocol::local_art::package::visibility::Package,
) -> Result<Vec<u8>, PluginSecurityError> {
    package.validate().map_err(|_| invalid())?;
    issue_payload(
        key,
        package,
        loom_protocol::local_art::package::visibility::DOMAIN,
    )
}
pub fn verify_visibility_package(
    bytes: &[u8],
    key_id: &str,
    public_key: &str,
) -> Result<loom_protocol::local_art::package::visibility::Package, PluginSecurityError> {
    let payload = verify_payload(
        bytes,
        key_id,
        public_key,
        loom_protocol::local_art::package::visibility::DOMAIN,
    )?;
    let package: loom_protocol::local_art::package::visibility::Package =
        serde_json::from_slice(&payload)?;
    package.validate().map_err(|_| invalid())?;
    Ok(package)
}
pub fn verify_port_package(
    bytes: &[u8],
    key_id: &str,
    public_key: &str,
) -> Result<loom_protocol::local_art::package::ports::Package, PluginSecurityError> {
    let payload = verify_payload(
        bytes,
        key_id,
        public_key,
        loom_protocol::local_art::package::ports::DOMAIN,
    )?;
    let package: loom_protocol::local_art::package::ports::Package =
        serde_json::from_slice(&payload)?;
    package.validate().map_err(|_| invalid())?;
    Ok(package)
}
pub fn verify_resume(
    bytes: &[u8],
    key_id: &str,
    public_key: &str,
) -> Result<loom_protocol::local_art::resume::Resume, PluginSecurityError> {
    let payload = verify_payload(
        bytes,
        key_id,
        public_key,
        loom_protocol::local_art::resume::DOMAIN,
    )?;
    let resume: loom_protocol::local_art::resume::Resume = serde_json::from_slice(&payload)?;
    resume.validate().map_err(|_| invalid())?;
    Ok(resume)
}

#[cfg(test)]
#[path = "local_art_grant_tests.rs"]
mod tests;
