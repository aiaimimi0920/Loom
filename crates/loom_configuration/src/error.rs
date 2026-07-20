use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedConfigErrorCode {
    UnknownApp,
    AppNotManaged,
    InvalidConfiguration,
    RevisionConflict,
    StorageError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl ValidationError {
    #[must_use]
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Error)]
#[error("{code:?}: {message}")]
pub struct ManagedConfigError {
    code: ManagedConfigErrorCode,
    message: String,
    validation_errors: Vec<ValidationError>,
}

impl ManagedConfigError {
    #[must_use]
    pub fn new(code: ManagedConfigErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            validation_errors: Vec::new(),
        }
    }

    #[must_use]
    pub fn invalid(errors: Vec<ValidationError>) -> Self {
        Self {
            code: ManagedConfigErrorCode::InvalidConfiguration,
            message: "configuration validation failed".to_string(),
            validation_errors: errors,
        }
    }

    #[must_use]
    pub fn code(&self) -> ManagedConfigErrorCode {
        self.code
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn validation_errors(&self) -> &[ValidationError] {
        &self.validation_errors
    }
}
