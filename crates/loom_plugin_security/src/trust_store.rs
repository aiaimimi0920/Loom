//! Versioned trust-store persistence and trust record mutation.

use std::collections::BTreeSet;
use std::path::Path;

use loom_protocol::PublisherTrustRecord;
use serde::{Deserialize, Serialize};

use crate::atomic::{read_bounded, write_bytes_atomic};
use crate::error::invalid_data;
use crate::{
    restrict_private_path_permissions, PluginSecurityError, TrustPolicy, MAX_TRUST_STORE_BYTES,
    TRUST_STORE_SCHEMA_VERSION,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustStore {
    #[serde(default = "default_trust_store_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub publishers: Vec<PublisherTrustRecord>,
    #[serde(default)]
    pub policy: TrustPolicy,
    #[serde(default)]
    pub trusted_publishers: Vec<String>,
}

impl Default for TrustStore {
    fn default() -> Self {
        Self {
            schema_version: TRUST_STORE_SCHEMA_VERSION,
            publishers: Vec::new(),
            policy: TrustPolicy::default(),
            trusted_publishers: Vec::new(),
        }
    }
}

const fn default_trust_store_schema_version() -> u32 {
    TRUST_STORE_SCHEMA_VERSION
}

impl TrustStore {
    pub fn load(path: &Path) -> Result<Self, PluginSecurityError> {
        // Repair legacy ACLs before opening a trust store created by older Loom builds.
        match restrict_private_path_permissions(path, false) {
            Ok(()) => {
                let bytes = read_bounded(path, MAX_TRUST_STORE_BYTES, "trust store")?;
                let store: Self = serde_json::from_slice(&bytes)?;
                store.validate()?;
                Ok(store)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(PluginSecurityError::Io(error)),
        }
    }

    pub fn write_atomic(&self, path: &Path) -> Result<(), PluginSecurityError> {
        self.validate()?;
        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        write_bytes_atomic(path, &bytes, true, true)
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

    pub fn set_policy(&mut self, policy: TrustPolicy) {
        self.policy = policy;
    }

    pub fn trust_publisher_id(&mut self, publisher_id: impl Into<String>) {
        let publisher_id = publisher_id.into();
        if !self
            .trusted_publishers
            .iter()
            .any(|existing| existing == &publisher_id)
        {
            self.trusted_publishers.push(publisher_id);
            self.trusted_publishers.sort();
        }
    }

    pub fn untrust_publisher_id(&mut self, publisher_id: &str) -> bool {
        let before_ids = self.trusted_publishers.len();
        self.trusted_publishers
            .retain(|existing| existing != publisher_id);
        let before_keys = self.publishers.len();
        self.publishers
            .retain(|record| record.publisher_id != publisher_id);
        before_ids != self.trusted_publishers.len() || before_keys != self.publishers.len()
    }

    pub fn effective_policy(&self) -> TrustPolicy {
        TrustPolicy::from_env_override().unwrap_or(self.policy)
    }

    pub(crate) fn validate(&self) -> Result<(), PluginSecurityError> {
        if self.schema_version != TRUST_STORE_SCHEMA_VERSION {
            return Err(invalid_data(format!(
                "unsupported trust-store schema version {}; expected {}",
                self.schema_version, TRUST_STORE_SCHEMA_VERSION
            )));
        }
        let mut seen = BTreeSet::new();
        for record in &self.publishers {
            if !seen.insert((&record.publisher_id, &record.key_id)) {
                return Err(invalid_data(format!(
                    "duplicate trust record for publisher {} and key {}",
                    record.publisher_id, record.key_id
                )));
            }
        }
        Ok(())
    }
}
