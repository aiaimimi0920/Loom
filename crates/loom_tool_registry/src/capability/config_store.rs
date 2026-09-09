use std::path::{Path, PathBuf};

use loom_protocol::{is_safe_capability_package_id, is_safe_capability_publisher_id};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::types::{CapabilityInstallError, CapabilityResult};
use crate::private_store::{
    lock_private_file, read_bounded_private_file, write_private_file_atomic,
};

const CONFIG_SCHEMA_VERSION: u32 = 1;
const CONFIG_MAX_BYTES: u64 = 256 * 1024;
const CONFIG_MAX_DEPTH: usize = 32;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityConfigDocument {
    pub schema_version: u32,
    pub qualified_id: String,
    pub revision: u64,
    pub values: Map<String, Value>,
}

#[derive(Clone, Debug)]
pub struct CapabilityConfigStore {
    root: PathBuf,
}

impl CapabilityConfigStore {
    #[must_use]
    pub fn new(control_plane_root: impl AsRef<Path>) -> Self {
        Self {
            root: control_plane_root
                .as_ref()
                .join("capabilities")
                .join("config"),
        }
    }

    pub fn read(&self, qualified_id: &str) -> CapabilityResult<CapabilityConfigDocument> {
        let path = self.path_for(qualified_id)?;
        let _lock = lock_private_file(&path)?;
        self.read_unlocked(&path, qualified_id)
    }

    /// Replaces a plugin's non-secret settings using optimistic concurrency.
    /// Secret-like keys are rejected; plugins must use the credential broker.
    pub fn write(
        &self,
        qualified_id: &str,
        expected_revision: u64,
        values: Map<String, Value>,
    ) -> CapabilityResult<CapabilityConfigDocument> {
        validate_values(&values)?;
        let path = self.path_for(qualified_id)?;
        let _lock = lock_private_file(&path)?;
        let current = self.read_unlocked(&path, qualified_id)?;
        if current.revision != expected_revision {
            return Err(CapabilityInstallError::Conflict(format!(
                "config revision is {}; expected {expected_revision}",
                current.revision
            )));
        }
        let document = CapabilityConfigDocument {
            schema_version: CONFIG_SCHEMA_VERSION,
            qualified_id: qualified_id.to_owned(),
            revision: current.revision.saturating_add(1),
            values,
        };
        let mut bytes = serde_json::to_vec_pretty(&document)?;
        bytes.push(b'\n');
        if bytes.len() as u64 > CONFIG_MAX_BYTES {
            return Err(CapabilityInstallError::InvalidState(
                "capability config exceeds 256 KiB".to_owned(),
            ));
        }
        write_private_file_atomic(&path, &bytes)?;
        Ok(document)
    }

    pub fn delete(&self, qualified_id: &str) -> CapabilityResult<()> {
        let path = self.path_for(qualified_id)?;
        let _lock = lock_private_file(&path)?;
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn read_unlocked(
        &self,
        path: &Path,
        qualified_id: &str,
    ) -> CapabilityResult<CapabilityConfigDocument> {
        if !path.exists() {
            return Ok(CapabilityConfigDocument {
                schema_version: CONFIG_SCHEMA_VERSION,
                qualified_id: qualified_id.to_owned(),
                revision: 0,
                values: Map::new(),
            });
        }
        let bytes = read_bounded_private_file(path, CONFIG_MAX_BYTES)?;
        let document: CapabilityConfigDocument = serde_json::from_slice(&bytes)?;
        if document.schema_version != CONFIG_SCHEMA_VERSION || document.qualified_id != qualified_id
        {
            return Err(CapabilityInstallError::InvalidRegistry(
                "capability config identity or schema mismatch".to_owned(),
            ));
        }
        validate_values(&document.values)?;
        Ok(document)
    }

    fn path_for(&self, qualified_id: &str) -> CapabilityResult<PathBuf> {
        let (publisher, package) = qualified_id.split_once('/').ok_or_else(|| {
            CapabilityInstallError::InvalidState("invalid capability id".to_owned())
        })?;
        if !is_safe_capability_publisher_id(publisher) || !is_safe_capability_package_id(package) {
            return Err(CapabilityInstallError::InvalidState(
                "invalid capability id".to_owned(),
            ));
        }
        Ok(self.root.join(publisher).join(format!("{package}.json")))
    }
}

fn validate_values(values: &Map<String, Value>) -> CapabilityResult<()> {
    // Walked in place rather than through a `Value::Object(values.clone())` wrapper: the wrapper
    // deep-copied the whole document, up to `CONFIG_MAX_BYTES`, on every settings read and write
    // purely to inspect it.
    if map_depth(values) > CONFIG_MAX_DEPTH || map_contains_secret_key(values) {
        return Err(CapabilityInstallError::InvalidState(
            "capability config is too deep or contains a secret-like key".to_owned(),
        ));
    }
    Ok(())
}

fn map_contains_secret_key(values: &Map<String, Value>) -> bool {
    values.iter().any(|(key, value)| {
        let key = key.to_ascii_lowercase().replace('-', "_");
        [
            "secret",
            "password",
            "token",
            "api_key",
            "apikey",
            "credential",
            "private_key",
        ]
        .iter()
        .any(|needle| key.contains(needle))
            || contains_secret_key(value)
    })
}

fn contains_secret_key(value: &Value) -> bool {
    match value {
        Value::Object(values) => map_contains_secret_key(values),
        Value::Array(values) => values.iter().any(contains_secret_key),
        _ => false,
    }
}

fn map_depth(values: &Map<String, Value>) -> usize {
    1 + values.values().map(json_depth).max().unwrap_or(0)
}

fn json_depth(value: &Value) -> usize {
    match value {
        Value::Array(values) => 1 + values.iter().map(json_depth).max().unwrap_or(0),
        Value::Object(values) => 1 + values.values().map(json_depth).max().unwrap_or(0),
        _ => 1,
    }
}
