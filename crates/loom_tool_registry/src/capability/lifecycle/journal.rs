//! Durable prepared/committed recovery for Capability Plugin lifecycle changes.

use std::fs;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[cfg(test)]
use super::journal_path;
use super::{cleanup_uninstall_side_state, lifecycle_root, plugin_root, remove_private_tree};
use crate::capability::registry::ensure_capability_root;
use crate::capability::types::CapabilityResult;
use crate::capability::{
    CapabilityConfigStore, CapabilityGrantStore, CapabilityInstallError, CapabilityPluginRecord,
    CapabilityPluginRegistry,
};
use crate::private_store::{
    lock_private_file, read_bounded_private_file, write_private_file_atomic,
};

pub(super) const JOURNAL_SCHEMA_VERSION: u32 = 1;
const JOURNAL_MAX_BYTES: u64 = 512 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CapabilityLifecycleJournal {
    pub(super) schema_version: u32,
    pub(super) operation: String,
    #[serde(default)]
    pub(super) phase: CapabilityLifecycleJournalPhase,
    pub(super) old_record: CapabilityPluginRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) next_record: Option<CapabilityPluginRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) tombstone: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum CapabilityLifecycleJournalPhase {
    #[default]
    Prepared,
    Committed,
}

impl CapabilityPluginRegistry {
    /// Rolls interrupted lifecycle operations back or forward from their durable phase.
    ///
    /// One bad journal must not stop the others. Both callers treat an error here as fatal — the
    /// daemon refuses to build the capability runtime, and every control-plane request then fails —
    /// so propagating a single unreadable journal would disable the whole subsystem, including the
    /// API needed to uninstall whatever wrote it. Per-journal outcomes instead:
    ///
    /// * unreadable or invalid: quarantined next to the original so the sweep makes progress and
    ///   the bytes stay on disk for diagnosis;
    /// * apply failed: left in place, because the usual cause is transient (a busy store, a locked
    ///   record) and the next start retries it.
    pub fn recover_capability_lifecycle(&self) -> CapabilityResult<usize> {
        let root = lifecycle_root(self);
        ensure_capability_root(&root)?;
        let mut recovered = 0usize;
        for entry in fs::read_dir(&root)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            // Serialize recovery with the live operation that owns this journal.
            let Ok(_lock) = lock_private_file(&path) else {
                continue;
            };
            let bytes = match read_bounded_private_file(&path, JOURNAL_MAX_BYTES) {
                Ok(bytes) => bytes,
                // A journal cleared by the transition that owned it between this sweep's `read_dir`
                // and its lock acquisition is the expected outcome of a concurrent lifecycle
                // request, not a fault: there is nothing left to recover or to quarantine.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    continue;
                }
                Err(_) => {
                    quarantine_journal(&path);
                    continue;
                }
            };
            let journal = serde_json::from_slice::<CapabilityLifecycleJournal>(&bytes)
                .ok()
                .filter(|journal| validate_journal(journal).is_ok());
            let Some(journal) = journal else {
                quarantine_journal(&path);
                continue;
            };
            if recover_journal(self, &journal).is_err() {
                continue;
            }
            if clear_journal(&path).is_err() {
                continue;
            }
            recovered += 1;
        }
        Ok(recovered)
    }

    #[cfg(test)]
    pub(crate) fn write_lifecycle_test_journal(
        &self,
        old_record: CapabilityPluginRecord,
        next_record: CapabilityPluginRecord,
        committed: bool,
    ) -> CapabilityResult<()> {
        let path = journal_path(self, &old_record.qualified_id)?;
        write_journal(
            &path,
            &CapabilityLifecycleJournal {
                schema_version: JOURNAL_SCHEMA_VERSION,
                operation: "test-transition".to_owned(),
                phase: phase(committed),
                old_record,
                next_record: Some(next_record),
                tombstone: None,
            },
        )
    }

    #[cfg(test)]
    pub(crate) fn write_uninstall_test_journal(
        &self,
        old_record: CapabilityPluginRecord,
        committed: bool,
    ) -> CapabilityResult<PathBuf> {
        let tombstone_name = format!(
            "{}~test-recovery",
            old_record.qualified_id.replace('/', "~")
        );
        let tombstone = self.packages_root().join(".trash").join(&tombstone_name);
        ensure_capability_root(tombstone.parent().expect("tombstone parent"))?;
        let path = journal_path(self, &old_record.qualified_id)?;
        write_journal(
            &path,
            &CapabilityLifecycleJournal {
                schema_version: JOURNAL_SCHEMA_VERSION,
                operation: "uninstall".to_owned(),
                phase: phase(committed),
                old_record,
                next_record: None,
                tombstone: Some(tombstone_name),
            },
        )?;
        Ok(tombstone)
    }
}

pub(super) fn write_journal(
    path: &Path,
    journal: &CapabilityLifecycleJournal,
) -> CapabilityResult<()> {
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

pub(super) fn clear_journal(path: &Path) -> CapabilityResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

/// Moves a journal that cannot be read or validated out of the sweep's way.
///
/// Deleting it would destroy the only record of an interrupted operation, and leaving it in place
/// would make every future sweep stop at the same file. The new name keeps the `.invalid` extension
/// so the sweep skips it, and carries a nonce so repeated quarantines do not overwrite each other.
/// Best-effort by design: if the rename itself fails the sweep still continues to the next journal.
fn quarantine_journal(path: &Path) {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let quarantined = path.with_file_name(format!("{name}.{}.invalid", super::unique_nonce()));
    let _ = fs::rename(path, quarantined);
}

fn recover_journal(
    registry: &CapabilityPluginRegistry,
    journal: &CapabilityLifecycleJournal,
) -> CapabilityResult<()> {
    match journal.phase {
        CapabilityLifecycleJournalPhase::Prepared => {
            registry.restore_record(journal.old_record.clone())?;
            if let Some(name) = &journal.tombstone {
                let tombstone = registry.packages_root().join(".trash").join(name);
                let live = plugin_root(registry, &journal.old_record.qualified_id)?;
                if tombstone.exists() && !live.exists() {
                    if let Some(parent) = live.parent() {
                        ensure_capability_root(parent)?;
                    }
                    fs::rename(tombstone, live)?;
                }
            }
        }
        CapabilityLifecycleJournalPhase::Committed => {
            if let Some(next) = &journal.next_record {
                registry.restore_record(next.clone())?;
            } else {
                if registry.get(&journal.old_record.qualified_id)?.is_some() {
                    registry.remove_record(&journal.old_record.qualified_id)?;
                }
                if let Some(name) = &journal.tombstone {
                    let tombstone = registry.packages_root().join(".trash").join(name);
                    if tombstone.exists() {
                        remove_private_tree(&tombstone)?;
                    }
                }
                let grants = CapabilityGrantStore::new(registry.control_plane_root());
                let config = CapabilityConfigStore::new(registry.control_plane_root());
                cleanup_uninstall_side_state(&grants, &config, &journal.old_record.qualified_id)?;
            }
        }
    }
    Ok(())
}

fn validate_journal(journal: &CapabilityLifecycleJournal) -> CapabilityResult<()> {
    let valid_tombstone = journal
        .tombstone
        .as_ref()
        .is_none_or(|name| valid_tombstone_name(name));
    if journal.schema_version != JOURNAL_SCHEMA_VERSION
        || journal.operation.is_empty()
        || !valid_tombstone
        || journal
            .next_record
            .as_ref()
            .is_some_and(|record| record.qualified_id != journal.old_record.qualified_id)
        || (journal.phase == CapabilityLifecycleJournalPhase::Committed
            && journal.next_record.is_none()
            && journal.tombstone.is_none())
    {
        return Err(CapabilityInstallError::InvalidRegistry(
            "invalid capability lifecycle journal".to_owned(),
        ));
    }
    Ok(())
}

fn valid_tombstone_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 512
        && !name
            .chars()
            .any(|character| matches!(character, '/' | '\\' | ':'))
        && !name.ends_with(['.', ' '])
        && name != "."
        && name != ".."
        && !loom_protocol::is_windows_reserved_device_name(name)
}

#[cfg(test)]
fn phase(committed: bool) -> CapabilityLifecycleJournalPhase {
    if committed {
        CapabilityLifecycleJournalPhase::Committed
    } else {
        CapabilityLifecycleJournalPhase::Prepared
    }
}

#[cfg(test)]
mod tombstone_name_tests {
    use super::valid_tombstone_name;

    #[test]
    fn rejects_windows_device_names_and_ambiguous_suffixes() {
        for name in ["CON", "nul.txt", "plugin. ", "plugin."] {
            assert!(!valid_tombstone_name(name), "{name}");
        }
        assert!(valid_tombstone_name(
            "publisher.example~text-tools~123456789"
        ));
    }
}
