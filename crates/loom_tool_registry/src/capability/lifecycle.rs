use std::fs;
use std::path::{Path, PathBuf};

use semver::Version;
use serde::{Deserialize, Serialize};

use super::config_store::CapabilityConfigStore;
use super::grant_store::CapabilityGrantStore;
use super::registry::ensure_capability_root;
use super::types::{
    CapabilityInstallError, CapabilityLifecycleStatus, CapabilityPluginRecord, CapabilityResult,
};
use super::CapabilityPluginRegistry;
use crate::private_store::{
    lock_private_file, read_bounded_private_file, write_private_file_atomic,
};

const JOURNAL_SCHEMA_VERSION: u32 = 1;
const JOURNAL_MAX_BYTES: u64 = 512 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapabilityLifecycleJournal {
    schema_version: u32,
    operation: String,
    old_record: CapabilityPluginRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    next_record: Option<CapabilityPluginRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tombstone: Option<String>,
}

impl CapabilityPluginRegistry {
    pub fn approve_permissions(
        &self,
        grants: &CapabilityGrantStore,
        qualified_id: &str,
        digest: &str,
        permissions: &[String],
    ) -> CapabilityResult<()> {
        let record = self
            .get(qualified_id)?
            .ok_or_else(|| CapabilityInstallError::NotFound(qualified_id.to_owned()))?;
        let version = installed_version(&record, digest)?;
        let mut expected = version.requested_permissions.clone();
        let mut actual = permissions.to_vec();
        expected.sort();
        expected.dedup();
        actual.sort();
        actual.dedup();
        if actual != expected {
            return Err(CapabilityInstallError::InvalidState(
                "approval must exactly match the installed package request".to_owned(),
            ));
        }
        grants.grant(qualified_id, digest, &actual)?;
        Ok(())
    }

    pub fn enable(
        &self,
        grants: &CapabilityGrantStore,
        qualified_id: &str,
        digest: Option<&str>,
    ) -> CapabilityResult<CapabilityPluginRecord> {
        let old = required_record(self, qualified_id)?;
        let digest = digest
            .map(str::to_owned)
            .or_else(|| latest_digest(&old))
            .ok_or_else(|| {
                CapabilityInstallError::InvalidState("no installed version".to_owned())
            })?;
        ensure_activation_allowed(self, grants, &old, &digest)?;
        transition(
            self,
            "enable",
            old,
            CapabilityLifecycleStatus::Activating,
            |record| {
                record.previous_digest = record
                    .active_digest
                    .take()
                    .filter(|previous| previous != &digest);
                record.active_digest = Some(digest);
                record.enabled_intent = true;
                record.status = CapabilityLifecycleStatus::Active;
            },
        )
    }

    pub fn disable(&self, qualified_id: &str) -> CapabilityResult<CapabilityPluginRecord> {
        let old = required_record(self, qualified_id)?;
        if old.status == CapabilityLifecycleStatus::InstalledDisabled && !old.enabled_intent {
            return Ok(old);
        }
        transition(
            self,
            "disable",
            old,
            CapabilityLifecycleStatus::Disabling,
            |record| {
                if let Some(active) = record.active_digest.take() {
                    record.previous_digest = Some(active);
                }
                record.enabled_intent = false;
                record.status = CapabilityLifecycleStatus::InstalledDisabled;
            },
        )
    }

    pub fn upgrade(
        &self,
        grants: &CapabilityGrantStore,
        qualified_id: &str,
        digest: &str,
    ) -> CapabilityResult<CapabilityPluginRecord> {
        let old = required_record(self, qualified_id)?;
        if old.active_digest.as_deref() == Some(digest) {
            return Ok(old);
        }
        ensure_activation_allowed(self, grants, &old, digest)?;
        let digest = digest.to_owned();
        transition(
            self,
            "upgrade",
            old,
            CapabilityLifecycleStatus::Upgrading,
            |record| {
                record.previous_digest = record.active_digest.replace(digest);
                record.enabled_intent = true;
                record.status = CapabilityLifecycleStatus::Active;
            },
        )
    }

    pub fn rollback(
        &self,
        grants: &CapabilityGrantStore,
        qualified_id: &str,
    ) -> CapabilityResult<CapabilityPluginRecord> {
        let old = required_record(self, qualified_id)?;
        let digest = old.previous_digest.clone().ok_or_else(|| {
            CapabilityInstallError::InvalidState("no previous capability version".to_owned())
        })?;
        ensure_activation_allowed(self, grants, &old, &digest)?;
        transition(
            self,
            "rollback",
            old,
            CapabilityLifecycleStatus::RollingBack,
            |record| {
                let current = record.active_digest.replace(digest);
                record.previous_digest = current;
                record.enabled_intent = true;
                record.status = CapabilityLifecycleStatus::Active;
            },
        )
    }

    pub fn uninstall(
        &self,
        grants: &CapabilityGrantStore,
        config: &CapabilityConfigStore,
        qualified_id: &str,
    ) -> CapabilityResult<CapabilityPluginRecord> {
        let mut old = required_record(self, qualified_id)?;
        if old.enabled_intent || old.status == CapabilityLifecycleStatus::Active {
            self.disable(qualified_id)?;
            old = required_record(self, qualified_id)?;
        }
        grants.revoke_plugin(qualified_id)?;
        config.delete(qualified_id)?;

        let live = plugin_root(self, qualified_id)?;
        let tombstone_name = format!("{}--{}", qualified_id.replace('/', "--"), unique_nonce());
        let trash = self.packages_root().join(".trash");
        ensure_capability_root(&trash)?;
        let tombstone = trash.join(&tombstone_name);
        let journal = CapabilityLifecycleJournal {
            schema_version: JOURNAL_SCHEMA_VERSION,
            operation: "uninstall".to_owned(),
            old_record: old.clone(),
            next_record: None,
            tombstone: Some(tombstone_name),
        };
        let journal_path = journal_path(self, qualified_id)?;
        let _lock = lock_private_file(&journal_path)?;
        write_journal(&journal_path, &journal)?;
        let result = (|| {
            if live.exists() {
                fs::rename(&live, &tombstone)?;
            }
            self.remove_record(qualified_id)?;
            clear_journal(&journal_path)?;
            if tombstone.exists() {
                remove_private_tree(&tombstone)?;
            }
            Ok(old.clone())
        })();
        if result.is_err() {
            let _ = self.restore_record(old);
            if tombstone.exists() && !live.exists() {
                let _ = fs::rename(&tombstone, &live);
            }
        }
        result
    }

    /// Rolls interrupted lifecycle operations back to their last durable record.
    pub fn recover_capability_lifecycle(&self) -> CapabilityResult<usize> {
        let root = lifecycle_root(self);
        ensure_capability_root(&root)?;
        let mut recovered = 0usize;
        for entry in fs::read_dir(&root)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let bytes = read_bounded_private_file(&path, JOURNAL_MAX_BYTES)?;
            let journal: CapabilityLifecycleJournal = serde_json::from_slice(&bytes)?;
            validate_journal(&journal)?;
            self.restore_record(journal.old_record.clone())?;
            if let Some(name) = &journal.tombstone {
                let tombstone = self.packages_root().join(".trash").join(name);
                let live = plugin_root(self, &journal.old_record.qualified_id)?;
                if tombstone.exists() && !live.exists() {
                    if let Some(parent) = live.parent() {
                        ensure_capability_root(parent)?;
                    }
                    fs::rename(tombstone, live)?;
                }
            }
            clear_journal(&path)?;
            recovered += 1;
        }
        cleanup_orphan_tombstones(self)?;
        Ok(recovered)
    }
}

fn transition(
    registry: &CapabilityPluginRegistry,
    operation: &str,
    old: CapabilityPluginRecord,
    intermediate_status: CapabilityLifecycleStatus,
    finalize: impl FnOnce(&mut CapabilityPluginRecord),
) -> CapabilityResult<CapabilityPluginRecord> {
    let mut intermediate = old.clone();
    intermediate.status = intermediate_status;
    let mut next = intermediate.clone();
    finalize(&mut next);
    let path = journal_path(registry, &old.qualified_id)?;
    let _lock = lock_private_file(&path)?;
    write_journal(
        &path,
        &CapabilityLifecycleJournal {
            schema_version: JOURNAL_SCHEMA_VERSION,
            operation: operation.to_owned(),
            old_record: old.clone(),
            next_record: Some(next.clone()),
            tombstone: None,
        },
    )?;
    if let Err(error) = registry.restore_record(intermediate) {
        let _ = registry.restore_record(old);
        return Err(error);
    }
    if let Err(error) = registry.restore_record(next.clone()) {
        let _ = registry.restore_record(old);
        return Err(error);
    }
    if let Err(error) = clear_journal(&path) {
        let _ = registry.restore_record(old);
        return Err(error);
    }
    Ok(next)
}

fn ensure_activation_allowed(
    registry: &CapabilityPluginRegistry,
    grants: &CapabilityGrantStore,
    record: &CapabilityPluginRecord,
    digest: &str,
) -> CapabilityResult<()> {
    let version = installed_version(record, digest)?;
    registry.verify_installed_version(&record.qualified_id, digest)?;
    if !grants.allows(&record.qualified_id, digest, &version.requested_permissions)? {
        registry.mutate_record(&record.qualified_id, |record| {
            record.enabled_intent = true;
            record.status = CapabilityLifecycleStatus::ApprovalRequired;
            Ok(())
        })?;
        return Err(CapabilityInstallError::PermissionRequired(
            record.qualified_id.clone(),
        ));
    }
    Ok(())
}

fn installed_version<'a>(
    record: &'a CapabilityPluginRecord,
    digest: &str,
) -> CapabilityResult<&'a super::types::CapabilityInstalledVersion> {
    record
        .versions
        .iter()
        .find(|version| version.digest == digest)
        .ok_or_else(|| CapabilityInstallError::NotFound(digest.to_owned()))
}

fn latest_digest(record: &CapabilityPluginRecord) -> Option<String> {
    record
        .versions
        .iter()
        .filter_map(|candidate| {
            Version::parse(&candidate.version)
                .ok()
                .map(|version| (version, &candidate.digest))
        })
        .max_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, digest)| digest.clone())
}

fn required_record(
    registry: &CapabilityPluginRegistry,
    qualified_id: &str,
) -> CapabilityResult<CapabilityPluginRecord> {
    registry
        .get(qualified_id)?
        .ok_or_else(|| CapabilityInstallError::NotFound(qualified_id.to_owned()))
}

fn plugin_root(
    registry: &CapabilityPluginRegistry,
    qualified_id: &str,
) -> CapabilityResult<PathBuf> {
    let (publisher, package) = qualified_id
        .split_once('/')
        .ok_or_else(|| CapabilityInstallError::InvalidState("invalid capability id".to_owned()))?;
    if !safe_component(publisher) || !safe_component(package) {
        return Err(CapabilityInstallError::InvalidState(
            "invalid capability id".to_owned(),
        ));
    }
    Ok(registry.packages_root().join(publisher).join(package))
}

fn journal_path(
    registry: &CapabilityPluginRegistry,
    qualified_id: &str,
) -> CapabilityResult<PathBuf> {
    let (publisher, package) = qualified_id
        .split_once('/')
        .ok_or_else(|| CapabilityInstallError::InvalidState("invalid capability id".to_owned()))?;
    if !safe_component(publisher) || !safe_component(package) {
        return Err(CapabilityInstallError::InvalidState(
            "invalid capability id".to_owned(),
        ));
    }
    Ok(lifecycle_root(registry).join(format!("{publisher}--{package}.json")))
}

fn lifecycle_root(registry: &CapabilityPluginRegistry) -> PathBuf {
    registry.packages_root().join(".lifecycle")
}

fn write_journal(path: &Path, journal: &CapabilityLifecycleJournal) -> CapabilityResult<()> {
    validate_journal(journal)?;
    let mut bytes = serde_json::to_vec_pretty(journal)?;
    bytes.push(b'\n');
    if bytes.len() as u64 > JOURNAL_MAX_BYTES {
        return Err(CapabilityInstallError::InvalidState(
            "capability lifecycle journal exceeds 512 KiB".to_owned(),
        ));
    }
    write_private_file_atomic(path, &bytes)?;
    Ok(())
}

fn validate_journal(journal: &CapabilityLifecycleJournal) -> CapabilityResult<()> {
    let valid_tombstone = journal.tombstone.as_ref().is_none_or(|name| {
        !name.is_empty()
            && name.len() <= 512
            && !name
                .chars()
                .any(|character| matches!(character, '/' | '\\' | ':'))
            && name != "."
            && name != ".."
    });
    if journal.schema_version != JOURNAL_SCHEMA_VERSION
        || journal.operation.is_empty()
        || !valid_tombstone
        || journal
            .next_record
            .as_ref()
            .is_some_and(|record| record.qualified_id != journal.old_record.qualified_id)
    {
        return Err(CapabilityInstallError::InvalidRegistry(
            "invalid capability lifecycle journal".to_owned(),
        ));
    }
    Ok(())
}

fn clear_journal(path: &Path) -> CapabilityResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn cleanup_orphan_tombstones(registry: &CapabilityPluginRegistry) -> CapabilityResult<()> {
    let trash = registry.packages_root().join(".trash");
    if !trash.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(trash)? {
        let path = entry?.path();
        remove_private_tree(&path)?;
    }
    Ok(())
}

fn remove_private_tree(path: &Path) -> CapabilityResult<()> {
    crate::install::fs_safety::remove_tree(path)
        .map_err(|error| CapabilityInstallError::InvalidState(error.to_string()))
}

fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-' | b'_')
        })
        && !value.contains("..")
}

fn unique_nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
