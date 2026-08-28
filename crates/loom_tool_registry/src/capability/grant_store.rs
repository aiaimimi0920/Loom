use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use chrono::Utc;
use loom_protocol::is_valid_capability_permission;
use serde::{Deserialize, Serialize};

use super::types::{CapabilityInstallError, CapabilityResult};
use crate::private_store::{
    lock_private_file, read_bounded_private_file, write_private_file_atomic,
};

const GRANT_SCHEMA_VERSION: u32 = 1;
const GRANT_MAX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityPermissionGrant {
    pub qualified_id: String,
    pub package_digest: String,
    pub permissions: Vec<String>,
    pub granted_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GrantDocument {
    schema_version: u32,
    #[serde(default)]
    grants: Vec<CapabilityPermissionGrant>,
}

impl Default for GrantDocument {
    fn default() -> Self {
        Self {
            schema_version: GRANT_SCHEMA_VERSION,
            grants: Vec::new(),
        }
    }
}

/// Persistent grants are separate from Art audit permissions and are always
/// bound to both a qualified plugin identity and an immutable package digest.
#[derive(Clone, Debug)]
pub struct CapabilityGrantStore {
    path: PathBuf,
}

impl CapabilityGrantStore {
    #[must_use]
    pub fn new(control_plane_root: impl AsRef<Path>) -> Self {
        Self {
            path: control_plane_root
                .as_ref()
                .join("capabilities")
                .join("grants.json"),
        }
    }

    pub fn list(&self) -> CapabilityResult<Vec<CapabilityPermissionGrant>> {
        let _lock = lock_private_file(&self.path)?;
        Ok(self.read_document()?.grants)
    }

    pub fn grant(
        &self,
        qualified_id: &str,
        package_digest: &str,
        permissions: &[String],
    ) -> CapabilityResult<CapabilityPermissionGrant> {
        validate_identity(qualified_id, package_digest)?;
        let permissions = normalize_permissions(permissions)?;
        let _lock = lock_private_file(&self.path)?;
        let mut document = self.read_document()?;
        document.grants.retain(|grant| {
            grant.qualified_id != qualified_id || grant.package_digest != package_digest
        });
        let grant = CapabilityPermissionGrant {
            qualified_id: qualified_id.to_owned(),
            package_digest: package_digest.to_owned(),
            permissions,
            granted_at: Utc::now().to_rfc3339(),
        };
        document.grants.push(grant.clone());
        sort_grants(&mut document.grants);
        self.write_document(&document)?;
        Ok(grant)
    }

    pub fn allows(
        &self,
        qualified_id: &str,
        package_digest: &str,
        required: &[String],
    ) -> CapabilityResult<bool> {
        if required.is_empty() {
            return Ok(true);
        }
        let required = normalize_permissions(required)?;
        let granted = self
            .list()?
            .into_iter()
            .find(|grant| {
                grant.qualified_id == qualified_id && grant.package_digest == package_digest
            })
            .map(|grant| grant.permissions)
            .unwrap_or_default();
        Ok(required
            .iter()
            .all(|permission| granted.contains(permission)))
    }

    pub fn revoke_plugin(&self, qualified_id: &str) -> CapabilityResult<usize> {
        let _lock = lock_private_file(&self.path)?;
        let mut document = self.read_document()?;
        let before = document.grants.len();
        document
            .grants
            .retain(|grant| grant.qualified_id != qualified_id);
        let removed = before - document.grants.len();
        if removed > 0 {
            self.write_document(&document)?;
        }
        Ok(removed)
    }

    fn read_document(&self) -> CapabilityResult<GrantDocument> {
        if !self.path.exists() {
            return Ok(GrantDocument::default());
        }
        let bytes = read_bounded_private_file(&self.path, GRANT_MAX_BYTES)?;
        let document: GrantDocument = serde_json::from_slice(&bytes)?;
        validate_document(&document)?;
        Ok(document)
    }

    fn write_document(&self, document: &GrantDocument) -> CapabilityResult<()> {
        validate_document(document)?;
        let mut bytes = serde_json::to_vec_pretty(document)?;
        bytes.push(b'\n');
        if bytes.len() as u64 > GRANT_MAX_BYTES {
            return Err(CapabilityInstallError::InvalidRegistry(
                "capability grant store exceeds 4 MiB".to_owned(),
            ));
        }
        write_private_file_atomic(&self.path, &bytes)?;
        Ok(())
    }
}

fn validate_document(document: &GrantDocument) -> CapabilityResult<()> {
    if document.schema_version != GRANT_SCHEMA_VERSION {
        return Err(CapabilityInstallError::InvalidRegistry(
            "unsupported capability grant schema".to_owned(),
        ));
    }
    let mut identities = BTreeSet::new();
    for grant in &document.grants {
        validate_identity(&grant.qualified_id, &grant.package_digest)?;
        if !identities.insert((&grant.qualified_id, &grant.package_digest)) {
            return Err(CapabilityInstallError::InvalidRegistry(
                "duplicate capability grant".to_owned(),
            ));
        }
        let normalized = normalize_permissions(&grant.permissions)?;
        if normalized != grant.permissions {
            return Err(CapabilityInstallError::InvalidRegistry(
                "capability permissions are not canonical".to_owned(),
            ));
        }
    }
    Ok(())
}

fn normalize_permissions(permissions: &[String]) -> CapabilityResult<Vec<String>> {
    if permissions.len() > 64
        || permissions
            .iter()
            .any(|permission| !is_valid_capability_permission(permission))
    {
        return Err(CapabilityInstallError::InvalidState(
            "invalid capability permission set".to_owned(),
        ));
    }
    let mut normalized = permissions.to_vec();
    normalized.sort();
    normalized.dedup();
    Ok(normalized)
}

fn validate_identity(qualified_id: &str, digest: &str) -> CapabilityResult<()> {
    let valid_id = qualified_id
        .split_once('/')
        .is_some_and(|(publisher, package)| !publisher.is_empty() && !package.is_empty());
    let valid_digest = digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'));
    if !valid_id || !valid_digest {
        return Err(CapabilityInstallError::InvalidState(
            "invalid capability grant identity".to_owned(),
        ));
    }
    Ok(())
}

fn sort_grants(grants: &mut [CapabilityPermissionGrant]) {
    grants.sort_by(|left, right| {
        (&left.qualified_id, &left.package_digest)
            .cmp(&(&right.qualified_id, &right.package_digest))
    });
}
