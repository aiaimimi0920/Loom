use loom_protocol::PackageTrustStatus;

use crate::{MAX_SIGNED_PACKAGE_BYTES, MAX_SIGNED_PACKAGE_FILES};

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

pub(crate) fn invalid_data(message: impl Into<String>) -> PluginSecurityError {
    PluginSecurityError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message.into(),
    ))
}
