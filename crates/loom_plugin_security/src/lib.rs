use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use loom_protocol::{
    PackageSignature, PackageSignatureDocument, PackageTrustStatus, PublisherIdentity,
    PublisherTrustRecord,
};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const TRUST_STORE_SCHEMA_VERSION: u32 = 1;
const SIGNING_KEY_SCHEMA_VERSION: u32 = 1;
const MAX_SIGNED_PACKAGE_FILES: usize = 4096;
const MAX_SIGNED_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum PluginSecurityError {
    #[error("unsafe package-relative path: {0}")]
    UnsafePath(String),
    #[error("symbolic links are not allowed in signed packages: {0}")]
    SymbolicLink(String),
    #[error("package contains a duplicate or case-colliding path: {0}")]
    DuplicatePath(String),
    #[error("package exceeds signing limit of {MAX_SIGNED_PACKAGE_FILES} files")]
    FileCount,
    #[error("package exceeds signing limit of {MAX_SIGNED_PACKAGE_BYTES} bytes")]
    PackageSize,
    #[error("unsupported signature algorithm: {0}")]
    UnsupportedAlgorithm(String),
    #[error("signature document does not match manifest metadata")]
    SignatureMetadataMismatch,
    #[error("package digest mismatch: expected {expected}, got {actual}")]
    DigestMismatch { expected: String, actual: String },
    #[error("invalid Ed25519 key or signature: {0}")]
    InvalidKey(String),
    #[error("signature verification failed")]
    VerificationFailed,
    #[error("plugin trust policy rejected package status {0:?}")]
    TrustPolicyRejected(PackageTrustStatus),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("base64 error: {0}")]
    Base64(#[from] base64::DecodeError),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SigningKeyDocument {
    #[serde(default = "default_signing_key_schema_version")]
    pub schema_version: u32,
    pub key_id: String,
    pub private_key: String,
    pub public_key: String,
}

const fn default_signing_key_schema_version() -> u32 {
    SIGNING_KEY_SCHEMA_VERSION
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustStore {
    #[serde(default = "default_trust_store_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub publishers: Vec<PublisherTrustRecord>,
}

impl Default for TrustStore {
    fn default() -> Self {
        Self {
            schema_version: TRUST_STORE_SCHEMA_VERSION,
            publishers: Vec::new(),
        }
    }
}

const fn default_trust_store_schema_version() -> u32 {
    TRUST_STORE_SCHEMA_VERSION
}

impl TrustStore {
    pub fn load(path: &Path) -> Result<Self, PluginSecurityError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }

    pub fn write_atomic(&self, path: &Path) -> Result<(), PluginSecurityError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("json.tmp");
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        fs::write(&temporary, bytes)?;
        if path.exists() {
            fs::remove_file(path)?;
        }
        fs::rename(temporary, path)?;
        Ok(())
    }

    pub fn trust(&mut self, record: PublisherTrustRecord) {
        self.publishers.retain(|existing| {
            existing.publisher_id != record.publisher_id || existing.key_id != record.key_id
        });
        self.publishers.push(record);
        self.publishers.sort_by(|left, right| {
            (&left.publisher_id, &left.key_id).cmp(&(&right.publisher_id, &right.key_id))
        });
    }

    pub fn revoke(&mut self, publisher_id: &str, key_id: &str) -> bool {
        let Some(record) = self
            .publishers
            .iter_mut()
            .find(|record| record.publisher_id == publisher_id && record.key_id == key_id)
        else {
            return false;
        };
        record.revoked = true;
        true
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrustPolicy {
    #[default]
    AllowUnsigned,
    RequireSigned,
    RequireTrusted,
}

impl TrustPolicy {
    pub fn from_env() -> Self {
        match std::env::var("LOOM_PLUGIN_TRUST_POLICY")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "require-signed" | "require_signed" => Self::RequireSigned,
            "require-trusted" | "require_trusted" => Self::RequireTrusted,
            _ => Self::AllowUnsigned,
        }
    }

    pub fn enforce(self, status: PackageTrustStatus) -> Result<(), PluginSecurityError> {
        let accepted = match self {
            Self::AllowUnsigned => matches!(
                status,
                PackageTrustStatus::Unsigned
                    | PackageTrustStatus::Verified
                    | PackageTrustStatus::Trusted
            ),
            Self::RequireSigned => matches!(
                status,
                PackageTrustStatus::Verified | PackageTrustStatus::Trusted
            ),
            Self::RequireTrusted => status == PackageTrustStatus::Trusted,
        };
        if accepted {
            Ok(())
        } else {
            Err(PluginSecurityError::TrustPolicyRejected(status))
        }
    }
}

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
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(document)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

pub fn read_signing_key(path: &Path) -> Result<SigningKeyDocument, PluginSecurityError> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn sign_package(
    package_dir: &Path,
    signature_path: &str,
    key: &SigningKeyDocument,
) -> Result<PackageSignatureDocument, PluginSecurityError> {
    validate_relative_path(signature_path)?;
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
        schema_version: 1,
        algorithm: "ed25519".to_owned(),
        key_id: key.key_id.clone(),
        digest_algorithm: "sha256".to_owned(),
        digest,
        signature: BASE64.encode(signature.to_bytes()),
        public_key,
    };
    let output = package_dir.join(signature_path);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(&document)?;
    bytes.push(b'\n');
    fs::write(output, bytes)?;
    Ok(document)
}

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
    validate_relative_path(&signature.file)?;
    let document: PackageSignatureDocument =
        serde_json::from_slice(&fs::read(package_dir.join(&signature.file))?)?;
    if document.algorithm != signature.algorithm
        || document.key_id != signature.key_id
        || document.digest_algorithm != "sha256"
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

pub fn canonical_package_digest(
    package_dir: &Path,
    excluded_relative_path: Option<&str>,
) -> Result<String, PluginSecurityError> {
    let excluded = excluded_relative_path.map(|path| path.replace('\\', "/").to_ascii_lowercase());
    let files = collect_files(package_dir, excluded.as_deref())?;
    let mut hasher = Sha256::new();
    for (relative, path) in files {
        let bytes = fs::read(path)?;
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update([0]);
        hasher.update(bytes);
    }
    Ok(hex_encode(&hasher.finalize()))
}

fn collect_files(
    root: &Path,
    excluded: Option<&str>,
) -> Result<Vec<(String, PathBuf)>, PluginSecurityError> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    let mut seen = BTreeSet::new();
    let mut total = 0u64;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(PluginSecurityError::SymbolicLink(
                    entry.path().display().to_string(),
                ));
            }
            if file_type.is_dir() {
                pending.push(entry.path());
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|_| PluginSecurityError::UnsafePath(entry.path().display().to_string()))?
                .to_string_lossy()
                .replace('\\', "/");
            validate_relative_path(&relative)?;
            let folded = relative.to_ascii_lowercase();
            if excluded == Some(folded.as_str()) {
                continue;
            }
            if !seen.insert(folded) {
                return Err(PluginSecurityError::DuplicatePath(relative));
            }
            total = total.saturating_add(entry.metadata()?.len());
            if total > MAX_SIGNED_PACKAGE_BYTES {
                return Err(PluginSecurityError::PackageSize);
            }
            files.push((relative, entry.path()));
            if files.len() > MAX_SIGNED_PACKAGE_FILES {
                return Err(PluginSecurityError::FileCount);
            }
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

fn validate_relative_path(value: &str) -> Result<(), PluginSecurityError> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(PluginSecurityError::UnsafePath(value.to_owned()));
    }
    Ok(())
}

fn decode_signing_key(value: &str) -> Result<SigningKey, PluginSecurityError> {
    let bytes = BASE64.decode(value.as_bytes())?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| PluginSecurityError::InvalidKey("private key must be 32 bytes".to_owned()))?;
    Ok(SigningKey::from_bytes(&bytes))
}

fn decode_verifying_key(value: &str) -> Result<VerifyingKey, PluginSecurityError> {
    let bytes = BASE64.decode(value.as_bytes())?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| PluginSecurityError::InvalidKey("public key must be 32 bytes".to_owned()))?;
    VerifyingKey::from_bytes(&bytes)
        .map_err(|error| PluginSecurityError::InvalidKey(error.to_string()))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "loom-plugin-security-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("directory");
        path
    }

    #[test]
    fn package_signature_roundtrip_and_trust_status() {
        let package = temp_dir("roundtrip");
        fs::write(package.join("manifest.json"), b"{}\n").unwrap();
        let key = generate_signing_key("test-key");
        sign_package(&package, "signature.json", &key).expect("sign");
        let publisher = PublisherIdentity {
            id: "example.vendor".to_owned(),
            key_id: Some(key.key_id.clone()),
            ..PublisherIdentity::default()
        };
        let signature = PackageSignature {
            algorithm: "ed25519".to_owned(),
            key_id: key.key_id.clone(),
            file: "signature.json".to_owned(),
        };
        let mut trust = TrustStore::default();
        trust.trust(PublisherTrustRecord {
            publisher_id: publisher.id.clone(),
            key_id: key.key_id.clone(),
            public_key: key.public_key.clone(),
            revoked: false,
        });
        assert_eq!(
            verify_package_signature(&package, Some(&publisher), Some(&signature), &trust)
                .expect("verify"),
            PackageTrustStatus::Trusted
        );
        fs::write(package.join("manifest.json"), b"tampered\n").unwrap();
        assert!(
            verify_package_signature(&package, Some(&publisher), Some(&signature), &trust).is_err()
        );
        let _ = fs::remove_dir_all(package);
    }

    #[test]
    fn trust_policy_is_explicit() {
        assert!(TrustPolicy::AllowUnsigned
            .enforce(PackageTrustStatus::Unsigned)
            .is_ok());
        assert!(TrustPolicy::RequireSigned
            .enforce(PackageTrustStatus::Unsigned)
            .is_err());
        assert!(TrustPolicy::RequireTrusted
            .enforce(PackageTrustStatus::Verified)
            .is_err());
    }
}
