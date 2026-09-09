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

/// A trust classification together with the canonical digest it was established against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedPackageSignature {
    pub trust_status: PackageTrustStatus,
    /// `None` only for an unsigned package, where no digest is computed.
    pub canonical_digest: Option<String>,
}

pub fn verify_package_signature(
    package_dir: &Path,
    publisher: Option<&PublisherIdentity>,
    signature: Option<&PackageSignature>,
    trust_store: &TrustStore,
) -> Result<PackageTrustStatus, PluginSecurityError> {
    verify_package_signature_with_digest(package_dir, publisher, signature, trust_store)
        .map(|verified| verified.trust_status)
}

/// Verifies a package signature and hands back the canonical digest it checked.
///
/// Establishing the trust status requires hashing every byte of the package, and callers that also
/// have to confirm the package still matches a digest they recorded elsewhere — the registry record,
/// the activated runtime package — were hashing the whole tree a second time to learn a value this
/// function already computed. That doubling landed on the hot paths: a runtime reverifies before
/// every spawn, including every lazy restart after an idle session is pruned.
pub fn verify_package_signature_with_digest(
    package_dir: &Path,
    publisher: Option<&PublisherIdentity>,
    signature: Option<&PackageSignature>,
    trust_store: &TrustStore,
) -> Result<VerifiedPackageSignature, PluginSecurityError> {
    let Some(signature) = signature else {
        return Ok(VerifiedPackageSignature {
            trust_status: PackageTrustStatus::Unsigned,
            canonical_digest: None,
        });
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
    let verified = |trust_status| VerifiedPackageSignature {
        trust_status,
        canonical_digest: Some(actual_digest.clone()),
    };

    // Revocation is a statement about key material, not about the label a package prints next
    // to it. `key_id` is attacker-controlled data inside the signature document, so keying the
    // lookup on `(publisher_id, key_id)` alone let the holder of a revoked private key re-sign
    // under a fresh label, miss the record, and downgrade `Revoked` to `Verified` - a status
    // both `allow-unsigned` and `require-signed` accept. Matching the public key itself, before
    // the unknown-publisher shortcut, keeps a revoked key revoked no matter what the package
    // calls it or whether it names a publisher at all.
    if trust_store
        .publishers
        .iter()
        .any(|record| record.revoked && record.public_key == document.public_key)
    {
        return Ok(verified(PackageTrustStatus::Revoked));
    }

    let Some(publisher) = publisher else {
        return Ok(verified(PackageTrustStatus::Verified));
    };
    // A valid signature only proves the package is signed, not that the publisher it names
    // signed it. Once this machine records any key for that publisher, a signature under some
    // other key is impersonation rather than an unknown publisher, and reporting it as
    // `Verified` would let it install under `require-signed` while presenting the pinned
    // publisher's name. A publisher with no records at all is left alone: there is no pinned
    // key to contradict, so the policy alone decides whether `Verified` is enough.
    let mut recorded = trust_store
        .publishers
        .iter()
        .filter(|record| record.publisher_id == publisher.id)
        .peekable();
    if recorded.peek().is_none() {
        return Ok(verified(PackageTrustStatus::Verified));
    }
    let Some(record) = recorded.find(|record| record.key_id == document.key_id) else {
        return Err(PluginSecurityError::PublisherKeyMismatch {
            publisher_id: publisher.id.clone(),
            key_id: document.key_id,
        });
    };
    if record.public_key != document.public_key {
        return Err(PluginSecurityError::VerificationFailed);
    }
    Ok(verified(PackageTrustStatus::Trusted))
}
