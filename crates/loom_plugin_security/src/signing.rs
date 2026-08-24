//! Ed25519 key persistence, message signing, and atomic package signatures.

use std::path::Path;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use loom_protocol::PackageSignatureDocument;
use rand_core::OsRng;

use crate::atomic::{read_bounded, write_bytes_atomic};
use crate::digest::{canonical_package_digest, checked_package_output_path};
use crate::{
    PluginSecurityError, SigningKeyDocument, MAX_SIGNING_KEY_BYTES,
    PACKAGE_SIGNATURE_SCHEMA_VERSION, SIGNING_KEY_SCHEMA_VERSION,
};

pub fn generate_signing_key(key_id: impl Into<String>) -> SigningKeyDocument {
    let signing_key = SigningKey::generate(&mut OsRng);
    SigningKeyDocument {
        schema_version: SIGNING_KEY_SCHEMA_VERSION,
        key_id: key_id.into(),
        private_key: BASE64.encode(signing_key.to_bytes()),
        public_key: BASE64.encode(signing_key.verifying_key().to_bytes()),
    }
}

pub fn write_signing_key(
    path: &Path,
    document: &SigningKeyDocument,
) -> Result<(), PluginSecurityError> {
    document.validate_schema()?;
    let mut bytes = serde_json::to_vec_pretty(document)?;
    bytes.push(b'\n');
    write_bytes_atomic(path, &bytes, true, true)
}

pub fn read_signing_key(path: &Path) -> Result<SigningKeyDocument, PluginSecurityError> {
    let bytes = read_bounded(path, MAX_SIGNING_KEY_BYTES, "signing key")?;
    let document: SigningKeyDocument = serde_json::from_slice(&bytes)?;
    document.validate_schema()?;
    Ok(document)
}

pub fn sign_package(
    package_dir: &Path,
    signature_path: &str,
    key: &SigningKeyDocument,
) -> Result<PackageSignatureDocument, PluginSecurityError> {
    key.validate_schema()?;
    checked_package_output_path(package_dir, signature_path)?;
    let signing_key = decode_signing_key(&key.private_key)?;
    let public_key = BASE64.encode(signing_key.verifying_key().to_bytes());
    if public_key != key.public_key {
        return Err(PluginSecurityError::InvalidKey(
            "private/public key mismatch".to_owned(),
        ));
    }
    let digest = canonical_package_digest(package_dir, Some(signature_path))?;
    let signature = signing_key.sign(digest.as_bytes());
    let document = PackageSignatureDocument {
        schema_version: PACKAGE_SIGNATURE_SCHEMA_VERSION,
        algorithm: "ed25519".to_owned(),
        key_id: key.key_id.clone(),
        digest_algorithm: "sha256".to_owned(),
        digest,
        signature: BASE64.encode(signature.to_bytes()),
        public_key,
    };
    let mut bytes = serde_json::to_vec_pretty(&document)?;
    bytes.push(b'\n');
    // Recheck after hashing closes the broad mutation window. The atomic helper
    // still treats a concurrently writable package directory as untrusted input.
    let output = checked_package_output_path(package_dir, signature_path)?;
    write_bytes_atomic(&output, &bytes, false, false)?;
    Ok(document)
}

pub fn sign_message(
    key: &SigningKeyDocument,
    message: &[u8],
) -> Result<String, PluginSecurityError> {
    key.validate_schema()?;
    let signing_key = decode_signing_key(&key.private_key)?;
    let public_key = BASE64.encode(signing_key.verifying_key().to_bytes());
    if public_key != key.public_key {
        return Err(PluginSecurityError::InvalidKey(
            "private/public key mismatch".to_owned(),
        ));
    }
    Ok(BASE64.encode(signing_key.sign(message).to_bytes()))
}

pub(crate) fn decode_signing_key(value: &str) -> Result<SigningKey, PluginSecurityError> {
    let bytes = BASE64.decode(value.as_bytes())?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| PluginSecurityError::InvalidKey("private key must be 32 bytes".to_owned()))?;
    Ok(SigningKey::from_bytes(&bytes))
}

pub(crate) fn decode_verifying_key(value: &str) -> Result<VerifyingKey, PluginSecurityError> {
    let bytes = BASE64.decode(value.as_bytes())?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| PluginSecurityError::InvalidKey("public key must be 32 bytes".to_owned()))?;
    VerifyingKey::from_bytes(&bytes)
        .map_err(|error| PluginSecurityError::InvalidKey(error.to_string()))
}
