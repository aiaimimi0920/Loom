use std::path::{Path, PathBuf};

use super::error::CredentialError;
use super::protection::{protect_value, unprotect_value};
use super::types::{
    CredentialDetails, CredentialFile, CredentialInput, CredentialScope, CredentialSummary,
    StoredCredential, CREDENTIALS_FILE, CREDENTIAL_STORE_SCHEMA_VERSION, MAX_CREDENTIAL_FILE_BYTES,
};
use super::values::{canonicalize_value, validate_input};

#[derive(Clone, Debug)]
pub struct CredentialStore {
    path: PathBuf,
}

impl CredentialStore {
    pub fn new(control_plane_root: impl AsRef<Path>) -> Self {
        Self {
            path: control_plane_root.as_ref().join(CREDENTIALS_FILE),
        }
    }

    pub fn summaries(&self) -> Result<Vec<CredentialSummary>, CredentialError> {
        let mut summaries = self
            .read_file()?
            .credentials
            .into_iter()
            .map(|credential| CredentialSummary {
                name: credential.name,
                value_type: credential.value_type,
                scope: credential.scope,
                expires_at: credential.expires_at,
                protection: credential.protection,
            })
            .collect::<Vec<_>>();
        summaries.sort_by(|left, right| {
            (
                &left.name,
                &left.scope.framework_id,
                &left.scope.art_id,
                &left.scope.mcp_server_id,
            )
                .cmp(&(
                    &right.name,
                    &right.scope.framework_id,
                    &right.scope.art_id,
                    &right.scope.mcp_server_id,
                ))
        });
        Ok(summaries)
    }

    pub fn reveal(
        &self,
        name: &str,
        scope: &CredentialScope,
    ) -> Result<Option<CredentialDetails>, CredentialError> {
        let Some(credential) = self
            .read_file()?
            .credentials
            .into_iter()
            .find(|credential| credential.name == name && &credential.scope == scope)
        else {
            return Ok(None);
        };
        let bytes = unprotect_value(&credential.protected_value, &credential.protection)?;
        let value = String::from_utf8(bytes)
            .map_err(|error| CredentialError::Protection(error.to_string()))?;
        Ok(Some(CredentialDetails {
            name: credential.name,
            value,
            value_type: credential.value_type,
            scope: credential.scope,
            expires_at: credential.expires_at,
            protection: credential.protection,
        }))
    }

    pub fn upsert(&self, input: CredentialInput) -> Result<CredentialSummary, CredentialError> {
        validate_input(&input)?;
        let canonical_value = canonicalize_value(input.value_type, &input.value)?;
        let (protected_value, protection) = protect_value(canonical_value.as_bytes())?;
        let _lock = crate::private_store::lock_private_file(&self.path)?;
        let mut file = self.read_file()?;
        file.schema_version = CREDENTIAL_STORE_SCHEMA_VERSION;
        file.credentials
            .retain(|credential| credential.name != input.name || credential.scope != input.scope);
        let summary = CredentialSummary {
            name: input.name.clone(),
            value_type: input.value_type,
            scope: input.scope.clone(),
            expires_at: input.expires_at.clone(),
            protection: protection.clone(),
        };
        file.credentials.push(StoredCredential {
            name: input.name,
            protected_value,
            protection,
            value_type: input.value_type,
            scope: input.scope,
            expires_at: input.expires_at,
        });
        self.write_file(&file)?;
        Ok(summary)
    }

    pub fn delete(&self, name: &str, scope: &CredentialScope) -> Result<bool, CredentialError> {
        let _lock = crate::private_store::lock_private_file(&self.path)?;
        let mut file = self.read_file()?;
        let before = file.credentials.len();
        file.credentials
            .retain(|credential| credential.name != name || &credential.scope != scope);
        let changed = file.credentials.len() != before;
        if changed {
            file.schema_version = CREDENTIAL_STORE_SCHEMA_VERSION;
            self.write_file(&file)?;
        }
        Ok(changed)
    }

    pub(super) fn read_file(&self) -> Result<CredentialFile, CredentialError> {
        match crate::private_store::read_bounded_private_file(&self.path, MAX_CREDENTIAL_FILE_BYTES)
        {
            Ok(bytes) => {
                let file: CredentialFile = serde_json::from_slice(&bytes)?;
                if file.schema_version != CREDENTIAL_STORE_SCHEMA_VERSION {
                    return Err(CredentialError::UnsupportedSchemaVersion {
                        actual: file.schema_version,
                        expected: CREDENTIAL_STORE_SCHEMA_VERSION,
                    });
                }
                Ok(file)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(CredentialFile::default())
            }
            Err(error) => Err(CredentialError::Io(error)),
        }
    }

    fn write_file(&self, file: &CredentialFile) -> Result<(), CredentialError> {
        let mut bytes = serde_json::to_vec_pretty(file)?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_CREDENTIAL_FILE_BYTES {
            return Err(CredentialError::StoreTooLarge {
                max_bytes: MAX_CREDENTIAL_FILE_BYTES,
            });
        }
        crate::private_store::write_private_file_atomic(&self.path, &bytes)
            .map_err(CredentialError::Io)
    }
}
