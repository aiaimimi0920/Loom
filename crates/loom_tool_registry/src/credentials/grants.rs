use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Utc};
use loom_protocol::CredentialGrant;

use super::error::CredentialError;
use super::protection::unprotect_value;
use super::store::CredentialStore;
use super::types::{CredentialValueType, ResolvedCredentialValue};
use super::values::decode_canonical_value;

impl CredentialStore {
    pub fn grants_for(
        &self,
        framework_id: &str,
        art_id: &str,
        requested: &[String],
    ) -> Result<Vec<CredentialGrant>, CredentialError> {
        let now = Utc::now();
        let requested = requested.iter().map(String::as_str).collect::<HashSet<_>>();
        let mut grants = Vec::new();
        for credential in self.read_file()?.credentials {
            if !requested.contains(credential.name.as_str())
                || credential
                    .scope
                    .framework_id
                    .as_deref()
                    .is_some_and(|scope| scope != framework_id)
                || credential
                    .scope
                    .art_id
                    .as_deref()
                    .is_some_and(|scope| scope != art_id)
                || credential.scope.mcp_server_id.is_some()
            {
                continue;
            }
            if is_expired_or_invalid(credential.expires_at.as_deref(), now) {
                continue;
            }
            let bytes = unprotect_value(&credential.protected_value, &credential.protection)?;
            let value = String::from_utf8(bytes)
                .map_err(|error| CredentialError::Protection(error.to_string()))?;
            grants.push(CredentialGrant {
                name: credential.name,
                value,
                expires_at: credential.expires_at,
            });
        }
        Ok(grants)
    }

    pub fn grants_for_bindings(
        &self,
        framework_id: &str,
        art_id: &str,
        bindings: &BTreeMap<String, String>,
    ) -> Result<Vec<CredentialGrant>, CredentialError> {
        let now = Utc::now();
        let credentials = self.read_file()?.credentials;
        let mut grants = Vec::with_capacity(bindings.len());
        for (alias, credential_name) in bindings {
            let credential = credentials
                .iter()
                .filter(|credential| {
                    credential.name == *credential_name
                        && credential
                            .scope
                            .framework_id
                            .as_deref()
                            .is_none_or(|scope| scope == framework_id)
                        && credential
                            .scope
                            .art_id
                            .as_deref()
                            .is_none_or(|scope| scope == art_id)
                        && credential.scope.mcp_server_id.is_none()
                        && !is_expired_or_invalid(credential.expires_at.as_deref(), now)
                })
                .max_by_key(|credential| {
                    usize::from(credential.scope.framework_id.is_some())
                        + usize::from(credential.scope.art_id.is_some())
                })
                .ok_or_else(|| CredentialError::MissingBinding {
                    alias: alias.clone(),
                    credential: credential_name.clone(),
                })?;
            if credential.value_type != CredentialValueType::String {
                return Err(CredentialError::NonStringSecretBinding {
                    alias: alias.clone(),
                    credential: credential_name.clone(),
                    actual: credential.value_type,
                });
            }
            let bytes = unprotect_value(&credential.protected_value, &credential.protection)?;
            grants.push(CredentialGrant {
                name: alias.clone(),
                value: String::from_utf8(bytes)
                    .map_err(|error| CredentialError::Protection(error.to_string()))?,
                expires_at: credential.expires_at.clone(),
            });
        }
        Ok(grants)
    }

    pub fn grants_for_mcp_bindings(
        &self,
        mcp_server_id: &str,
        bindings: &BTreeMap<String, String>,
    ) -> Result<Vec<CredentialGrant>, CredentialError> {
        let now = Utc::now();
        let credentials = self.read_file()?.credentials;
        let mut grants = Vec::with_capacity(bindings.len());
        for (alias, credential_name) in bindings {
            let credential = credentials
                .iter()
                .filter(|credential| {
                    credential.name == *credential_name
                        && credential.scope.framework_id.is_none()
                        && credential.scope.art_id.is_none()
                        && credential
                            .scope
                            .mcp_server_id
                            .as_deref()
                            .is_none_or(|scope| scope == mcp_server_id)
                        && !is_expired_or_invalid(credential.expires_at.as_deref(), now)
                })
                .max_by_key(|credential| usize::from(credential.scope.mcp_server_id.is_some()))
                .ok_or_else(|| CredentialError::MissingBinding {
                    alias: alias.clone(),
                    credential: credential_name.clone(),
                })?;
            if credential.value_type != CredentialValueType::String {
                return Err(CredentialError::NonStringSecretBinding {
                    alias: alias.clone(),
                    credential: credential_name.clone(),
                    actual: credential.value_type,
                });
            }
            let bytes = unprotect_value(&credential.protected_value, &credential.protection)?;
            grants.push(CredentialGrant {
                name: alias.clone(),
                value: String::from_utf8(bytes)
                    .map_err(|error| CredentialError::Protection(error.to_string()))?,
                expires_at: credential.expires_at.clone(),
            });
        }
        Ok(grants)
    }

    pub fn global_values_for_bindings(
        &self,
        bindings: &BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, ResolvedCredentialValue>, CredentialError> {
        let now = Utc::now();
        let credentials = self.read_file()?.credentials;
        let mut values = BTreeMap::new();
        for (alias, credential_name) in bindings {
            let credential = credentials
                .iter()
                .find(|credential| {
                    credential.name == *credential_name
                        && credential.scope.framework_id.is_none()
                        && credential.scope.art_id.is_none()
                        && credential.scope.mcp_server_id.is_none()
                        && !is_expired_or_invalid(credential.expires_at.as_deref(), now)
                })
                .ok_or_else(|| CredentialError::MissingBinding {
                    alias: alias.clone(),
                    credential: credential_name.clone(),
                })?;
            let bytes = unprotect_value(&credential.protected_value, &credential.protection)?;
            let raw = String::from_utf8(bytes)
                .map_err(|error| CredentialError::Protection(error.to_string()))?;
            values.insert(
                alias.clone(),
                ResolvedCredentialValue {
                    value_type: credential.value_type,
                    value: decode_canonical_value(credential.value_type, &raw)?,
                },
            );
        }
        Ok(values)
    }
}

fn is_expired_or_invalid(expires_at: Option<&str>, now: DateTime<Utc>) -> bool {
    let Some(expires_at) = expires_at else {
        return false;
    };
    DateTime::parse_from_rfc3339(expires_at)
        .map(|expires| expires.with_timezone(&Utc) <= now)
        .unwrap_or(true)
}
