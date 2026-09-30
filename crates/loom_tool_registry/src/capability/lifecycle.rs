use std::fs;
use std::path::{Path, PathBuf};

use loom_protocol::{is_safe_capability_package_id, is_safe_capability_publisher_id};

use super::config_store::CapabilityConfigStore;
use super::grant_store::CapabilityGrantStore;
use super::registry::ensure_capability_root;
use super::types::{
    CapabilityInstallError, CapabilityLifecycleStatus, CapabilityPluginRecord, CapabilityResult,
};
use super::CapabilityPluginRegistry;
use crate::private_store::lock_private_file;

mod journal;
use journal::{
    clear_journal, write_journal, CapabilityLifecycleJournal, CapabilityLifecycleJournalPhase,
    JOURNAL_SCHEMA_VERSION,
};

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
                // `previous_digest` is the rollback target, so only an activation that displaces
                // a different version writes it. Disabling displaces nothing, and re-enabling the
                // same version displaces nothing either; overwriting in those cases would drop
                // the version the user actually wants to roll back to.
                if let Some(displaced) = record
                    .active_digest
                    .take()
                    .filter(|previous| previous != &digest)
                {
                    record.previous_digest = Some(displaced);
                }
                record.active_digest = Some(digest);
                record.enabled_intent = true;
                record.status = CapabilityLifecycleStatus::Active;
                record.runtime_failures = Default::default();
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
                // Deliberately leaves `previous_digest` alone: disabling is not an activation, so
                // the version the user can roll back to is still whatever the last upgrade
                // displaced.
                record.active_digest = None;
                record.enabled_intent = false;
                record.status = CapabilityLifecycleStatus::InstalledDisabled;
                record.runtime_failures = Default::default();
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
                record.runtime_failures = Default::default();
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
                record.runtime_failures = Default::default();
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
        let live = plugin_root(self, qualified_id)?;
        // `~` is not in the id alphabet; `--` was, so publisher `a--b` package `c` and publisher
        // `a` package `b--c` produced the same tombstone name if they shared a nanos timestamp.
        let tombstone_name = format!("{}~{}", qualified_id.replace('/', "~"), unique_nonce());
        let trash = self.packages_root().join(".trash");
        ensure_capability_root(&trash)?;
        let tombstone = trash.join(&tombstone_name);
        let mut journal = CapabilityLifecycleJournal {
            schema_version: JOURNAL_SCHEMA_VERSION,
            operation: "uninstall".to_owned(),
            phase: CapabilityLifecycleJournalPhase::Prepared,
            old_record: old.clone(),
            next_record: None,
            tombstone: Some(tombstone_name),
        };
        let journal_path = journal_path(self, qualified_id)?;
        let _lock = lock_private_file(&journal_path)?;
        write_journal(&journal_path, &journal)?;
        let mut committed = false;
        let result = (|| {
            if live.exists() {
                fs::rename(&live, &tombstone)?;
            }
            self.remove_record(qualified_id)?;
            journal.phase = CapabilityLifecycleJournalPhase::Committed;
            write_journal(&journal_path, &journal)?;
            committed = true;
            cleanup_uninstall_side_state(grants, config, qualified_id)?;
            if tombstone.exists() {
                remove_private_tree(&tombstone)?;
            }
            clear_journal(&journal_path)?;
            Ok(old.clone())
        })();
        if result.is_err() && !committed {
            let _ = self.restore_record(old);
            if tombstone.exists() && !live.exists() {
                let _ = fs::rename(&tombstone, &live);
            }
        }
        result
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
    let mut journal = CapabilityLifecycleJournal {
        schema_version: JOURNAL_SCHEMA_VERSION,
        operation: operation.to_owned(),
        phase: CapabilityLifecycleJournalPhase::Prepared,
        old_record: old.clone(),
        next_record: Some(next.clone()),
        tombstone: None,
    };
    write_journal(&path, &journal)?;
    if let Err(error) = registry.restore_record(intermediate) {
        let _ = registry.restore_record(old);
        return Err(error);
    }
    if let Err(error) = registry.restore_record(next.clone()) {
        let _ = registry.restore_record(old);
        return Err(error);
    }
    journal.phase = CapabilityLifecycleJournalPhase::Committed;
    write_journal(&path, &journal)?;
    clear_journal(&path)?;
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
    record.latest_semver_digest().map(str::to_owned)
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
    if !is_safe_capability_publisher_id(publisher) || !is_safe_capability_package_id(package) {
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
    if !is_safe_capability_publisher_id(publisher) || !is_safe_capability_package_id(package) {
        return Err(CapabilityInstallError::InvalidState(
            "invalid capability id".to_owned(),
        ));
    }
    // `~` is not in the id alphabet, which makes this mapping injective. A `--` separator was not:
    // `-` is legal anywhere inside an id, so publisher `a--b` package `c` and publisher `a`
    // package `b--c` produced the same file name. Two plugins sharing one journal means one
    // plugin's prepared operation is overwritten by the other's, and the interrupted record is
    // then left in its intermediate status with nothing left on disk to recover it from.
    Ok(lifecycle_root(registry).join(format!("{publisher}~{package}.json")))
}

fn lifecycle_root(registry: &CapabilityPluginRegistry) -> PathBuf {
    registry.packages_root().join(".lifecycle")
}

fn cleanup_uninstall_side_state(
    grants: &CapabilityGrantStore,
    config: &CapabilityConfigStore,
    qualified_id: &str,
) -> CapabilityResult<()> {
    grants.revoke_plugin(qualified_id)?;
    config.delete(qualified_id)
}

fn remove_private_tree(path: &Path) -> CapabilityResult<()> {
    crate::install::fs_safety::remove_tree(path)
        .map_err(|error| CapabilityInstallError::InvalidState(error.to_string()))
}

fn unique_nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
