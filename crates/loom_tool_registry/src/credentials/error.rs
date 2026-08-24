use super::types::CredentialValueType;

#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("credential name is not a safe id: {0}")]
    UnsafeName(String),
    #[error("credential scope contains an unsafe package id: {0}")]
    UnsafeScope(String),
    #[error("credential value is empty")]
    EmptyValue,
    #[error("credential value is invalid for type `{value_type:?}`: {reason}")]
    InvalidValue {
        value_type: CredentialValueType,
        reason: String,
    },
    #[error("credential expiration is invalid: {0}")]
    InvalidExpiration(String),
    #[error("credential store schema version {actual} is unsupported; expected {expected}")]
    UnsupportedSchemaVersion { actual: u32, expected: u32 },
    #[error("credential store exceeds the {max_bytes}-byte limit")]
    StoreTooLarge { max_bytes: u64 },
    #[error("credential `{credential}` referenced by Art field `{alias}` is missing, expired, or outside its scope")]
    MissingBinding { alias: String, credential: String },
    #[error("credential `{credential}` referenced by secret Art field `{alias}` must be a string, got `{actual:?}`")]
    NonStringSecretBinding {
        alias: String,
        credential: String,
        actual: CredentialValueType,
    },
    #[error("credential protection failed: {0}")]
    Protection(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("base64 error: {0}")]
    Base64(#[from] base64::DecodeError),
}
