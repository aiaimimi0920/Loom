use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ManagedAppId, ManagedConfigError, ManagedConfigErrorCode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedDocumentMetadata {
    pub document_version: u32,
    pub schema_version: u32,
    pub revision: u64,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManagedConfigDocument {
    pub document_version: u32,
    pub app: ManagedAppId,
    pub schema_version: u32,
    pub revision: u64,
    pub updated_at: String,
    pub source_of_truth: String,
    pub config: Value,
}

impl ManagedConfigDocument {
    #[must_use]
    pub fn new(app: ManagedAppId, schema_version: u32, config: Value) -> Self {
        Self {
            document_version: 1,
            app,
            schema_version,
            revision: 1,
            updated_at: now_utc_string(),
            source_of_truth: "loom".to_string(),
            config,
        }
    }

    #[must_use]
    pub fn metadata(&self) -> ManagedDocumentMetadata {
        ManagedDocumentMetadata {
            document_version: self.document_version,
            schema_version: self.schema_version,
            revision: self.revision,
            updated_at: self.updated_at.clone(),
        }
    }

    pub fn replace_config(
        &mut self,
        expected_revision: u64,
        config: Value,
    ) -> Result<(), ManagedConfigError> {
        if self.revision != expected_revision {
            return Err(ManagedConfigError::new(
                ManagedConfigErrorCode::RevisionConflict,
                "configuration was updated by another writer",
            ));
        }
        self.revision += 1;
        self.updated_at = now_utc_string();
        self.config = config;
        Ok(())
    }
}

fn now_utc_string() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}
