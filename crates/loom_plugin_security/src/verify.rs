//! Signature-document validation and explicit publisher trust classification.

use std::path::Path;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier};
use loom_protocol::{
    PackageSignature, PackageSignatureDocument, PackageTrustStatus, PublisherIdentity,
};

use crate::atomic::read_bounded;
use crate::digest::{canonical_package_digest, checked_package_output_path};
use crate::signing::decode_verifying_key;
use crate::{
    PluginSecurityError, TrustStore, MAX_SIGNATURE_DOCUMENT_BYTES, PACKAGE_SIGNATURE_SCHEMA_VERSION,
};

pub fn verify_package_signature(
    package_dir: &Path,
    publisher: Option<&PublisherIdentity>,
    signature: Option<&PackageSignature>,
    trust_store: &TrustStore,
) -> Result<PackageTrustStatus, PluginSecurityError> {
    let Some(signature) = signature else {
        return Ok(PackageTrustStatus::Unsigned);
    };
    if signature.algorithm != "ed25519" {
        return Err(PluginSecurityError::UnsupportedAlgorithm(
            signature.algorithm.clone(),
        ));
    }
    trust_store.validate()?;
    let signature_path = checked_package_output_path(package_dir, &signature.file)?;
    let document_bytes = read_bounded(
        &signature_path,
        MAX_SIGNATURE_DOCUMENT_BYTES,
        "package signature document",
    )?;
    let document: PackageSignatureDocument = serde_json::from_slice(&document_bytes)?;
    if document.schema_version != PACKAGE_SIGNATURE_SCHEMA_VERSION
        || document.algorithm != signature.algorithm
        || document.key_id != signature.key_id
        || document.digest_algorithm != "sha256"
        || publisher.is_some_and(|identity| identity.key_id.as_deref() != Some(&document.key_id))
    {
        return Err(PluginSecurityError::SignatureMetadataMismatch);
    }
    let actual_digest = canonical_package_digest(package_dir, Some(&signature.file))?;
    if actual_digest != document.digest {
        return Err(PluginSecurityError::DigestMismatch {
            expected: document.digest,
            actual: actual_digest,
        });
    }
    let verifying_key = decode_verifying_key(&document.public_key)?;
    let signature_bytes = BASE64.decode(document.signature.as_bytes())?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|error| PluginSecurityError::InvalidKey(error.to_string()))?;
    verifying_key
        .verify(actual_digest.as_bytes(), &signature)
        .map_err(|_| PluginSecurityError::VerificationFailed)?;

    let Some(publisher) = publisher else {
        return Ok(PackageTrustStatus::Verified);
    };
    let Some(record) = trust_store
        .publishers
        .iter()
        .find(|record| record.publisher_id == publisher.id && record.key_id == document.key_id)
    else {
        return Ok(PackageTrustStatus::Verified);
    };
    if record.revoked {
        return Ok(PackageTrustStatus::Revoked);
    }
    if record.public_key != document.public_key {
        return Err(PluginSecurityError::VerificationFailed);
    }
    Ok(PackageTrustStatus::Trusted)
}
