// Canonical package digest and Ed25519 verification against the publisher directory.
use std::path::Path;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use loom_protocol::{PackageSignature, PackageSignatureDocument, PublisherIdentity};
use sha2::{Digest, Sha256};

use crate::archive::{
    hash_entry_bounded, open_bounded_archive, read_entry_bounded, MAX_SIGNATURE_DOCUMENT_BYTES,
};
use crate::manifest::read_manifest_bytes;
use crate::model::{PublisherKeyStatus, StoreError};
use crate::publisher::{is_platform_publisher_id, read_publisher};
use crate::validation::is_safe_resource_name;

pub(crate) fn verify_published_package_signature(
    root: &Path,
    zip_bytes: &[u8],
) -> Result<(), StoreError> {
    let manifest_bytes = read_manifest_bytes(zip_bytes)?;
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
    let security = manifest
        .get("metadata")
        .and_then(|metadata| metadata.get("packageSecurity"))
        .ok_or(StoreError::MissingPublisherSignature)?;
    let publisher: PublisherIdentity = security
        .get("publisher")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .ok_or(StoreError::MissingPublisherSignature)?;
    let signature: PackageSignature = security
        .get("signature")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?
        .ok_or(StoreError::MissingPublisherSignature)?;
    if !is_platform_publisher_id(&publisher.id)
        || publisher.key_id.as_deref() != Some(signature.key_id.as_str())
        || signature.algorithm != "ed25519"
        || !is_safe_resource_name(&signature.file)
    {
        return Err(StoreError::InvalidPublisherSignatureMetadata);
    }
    let platform_publisher = read_publisher(root, &publisher.id)?
        .ok_or_else(|| StoreError::PublisherNotFound(publisher.id.clone()))?;
    let active_key = platform_publisher
        .keys
        .iter()
        .find(|key| key.key_id == signature.key_id && key.status == PublisherKeyStatus::Active)
        .ok_or_else(|| StoreError::PublisherActiveKeyMissing(publisher.id.clone()))?;

    let mut archive = open_bounded_archive(zip_bytes)?;
    let mut files = Vec::<(String, usize, u64)>::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut signature_document = None;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        if file.is_dir() {
            continue;
        }
        let raw_name = file.name().to_owned();
        let name = raw_name.replace('\\', "/");
        if raw_name.contains('\\')
            || file.enclosed_name().is_none()
            || !is_safe_resource_name(&name)
        {
            return Err(StoreError::InvalidResourceName(name));
        }
        let folded = name.to_ascii_lowercase();
        if !seen.insert(folded.clone()) {
            return Err(StoreError::InvalidResourceName(name));
        }
        if folded == signature.file.to_ascii_lowercase() {
            let size = file.size();
            let bytes = read_entry_bounded(&mut file, &name, size, MAX_SIGNATURE_DOCUMENT_BYTES)?;
            signature_document = Some(serde_json::from_slice::<PackageSignatureDocument>(&bytes)?);
        } else {
            files.push((name, index, file.size()));
        }
    }
    let document = signature_document.ok_or(StoreError::MissingPublisherSignature)?;
    if document.algorithm != signature.algorithm
        || document.key_id != signature.key_id
        || document.digest_algorithm != "sha256"
        || document.public_key != active_key.public_key
    {
        return Err(StoreError::InvalidPublisherSignatureMetadata);
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = Sha256::new();
    for (name, index, size) in files {
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update(size.to_le_bytes());
        hasher.update([0]);
        let mut file = archive.by_index(index)?;
        hash_entry_bounded(&mut file, &mut hasher, &name, size)?;
    }
    let digest = format!("{:x}", hasher.finalize());
    if digest != document.digest {
        return Err(StoreError::PublisherSignatureVerification);
    }
    let public_key = BASE64
        .decode(document.public_key.as_bytes())
        .map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    let public_key: [u8; 32] = public_key
        .try_into()
        .map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    let verifying_key =
        VerifyingKey::from_bytes(&public_key).map_err(|_| StoreError::InvalidPublisherPublicKey)?;
    let signature_bytes = BASE64
        .decode(document.signature.as_bytes())
        .map_err(|_| StoreError::PublisherSignatureVerification)?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| StoreError::PublisherSignatureVerification)?;
    verifying_key
        .verify(digest.as_bytes(), &signature)
        .map_err(|_| StoreError::PublisherSignatureVerification)
}
