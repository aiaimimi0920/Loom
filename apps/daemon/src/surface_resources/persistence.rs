use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use loom_protocol::{SurfaceResourceLease, SurfaceResourceTransportKind};

use super::content::resource_digest;
use super::*;
use crate::{create_sensitive_temporary, replace_sensitive_file, runtime_log_warn};

impl SurfaceResourceStore {
    #[cfg(test)]
    pub(crate) fn new(root: impl AsRef<Path>) -> Result<Self, SurfaceResourceStoreError> {
        Self::new_with_gc_min_age(root, DEFAULT_RESOURCE_GC_MIN_AGE_MILLIS)
    }

    /// Opens the content-addressed store while isolating individual damaged records.
    pub(crate) fn new_with_gc_min_age(
        root: impl AsRef<Path>,
        gc_min_age_ms: u64,
    ) -> Result<Self, SurfaceResourceStoreError> {
        if gc_min_age_ms < MIN_RESOURCE_GC_AGE_MILLIS {
            return Err(SurfaceResourceStoreError::Invalid(format!(
                "Surface resource GC minimum age must be at least {MIN_RESOURCE_GC_AGE_MILLIS} ms"
            )));
        }
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        let mut resources = BTreeMap::new();
        for entry in fs::read_dir(&root)? {
            let path = match entry {
                Ok(entry) => entry.path(),
                Err(error) => {
                    runtime_log_warn(format!(
                        "loom Surface resource store skipped an unreadable entry in {}: {error}",
                        root.display()
                    ));
                    continue;
                }
            };
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            if path.file_name().and_then(|value| value.to_str()) == Some("leases.json") {
                continue;
            }
            // A content-addressed object can be registered again, so one damaged record must not
            // prevent the daemon from opening the rest of the store.
            match load_stored_resource(&root, &path) {
                Ok(stored) => {
                    resources.insert(stored.descriptor.resource_id.clone(), stored);
                }
                Err(reason) => {
                    runtime_log_warn(format!(
                        "loom Surface resource store is discarding {}: {reason}",
                        path.display()
                    ));
                }
            }
        }
        let leases_path = root.join("leases.json");
        let leases_file_exists = leases_path.is_file();
        let leases = if leases_file_exists {
            serde_json::from_slice::<BTreeMap<String, SurfaceResourceLease>>(&fs::read(
                &leases_path,
            )?)?
        } else {
            BTreeMap::new()
        };
        let lease_count_before_cleanup = leases.len();
        let mut store = Self {
            root,
            resources,
            leases,
            verified: BTreeMap::new(),
            leases_dirty: false,
            leases_persisted_at_ms: 0,
            gc_min_age_ms,
        };
        store.cleanup_expired();
        store.leases.retain(|lease_id, lease| {
            lease.lease_id == *lease_id
                && lease.transport.kind != SurfaceResourceTransportKind::SharedMemory
                && store
                    .resources
                    .get(&lease.resource.resource_id)
                    .is_some_and(|resource| resource.descriptor == lease.resource)
        });
        store.enforce_lease_limit_after_load();
        if !leases_file_exists || store.leases.len() != lease_count_before_cleanup {
            store.persist_leases()?;
        } else {
            // The on-disk table already represents this state, so establish the debounce epoch
            // without paying for an identical serialization, fsync, and atomic replacement.
            store.leases_persisted_at_ms = crate::unix_time_millis();
        }
        Ok(store)
    }

    /// A hand-edited or older lease table must not bypass the same cap enforced at runtime.
    fn enforce_lease_limit_after_load(&mut self) {
        let excess = self.leases.len().saturating_sub(MAX_ACTIVE_RESOURCE_LEASES);
        if excess == 0 {
            return;
        }
        let mut by_expiration: Vec<(u64, String)> = self
            .leases
            .iter()
            .map(|(lease_id, lease)| (lease.expires_at_ms, lease_id.clone()))
            .collect();
        by_expiration.sort_unstable();
        for (_, lease_id) in by_expiration.into_iter().take(excess) {
            self.leases.remove(&lease_id);
        }
        runtime_log_warn(format!(
            "loom Surface resource store discarded {excess} excess leases while loading"
        ));
    }
}

/// Writes a sensitive store file through a synced temporary and atomic replacement.
pub(super) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), SurfaceResourceStoreError> {
    let (temporary, mut file) = create_sensitive_temporary(path)?;
    let result = (|| -> Result<(), SurfaceResourceStoreError> {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        replace_sensitive_file(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

/// Loads metadata only after confirming the paired payload exists at the declared length.
fn load_stored_resource(root: &Path, path: &Path) -> Result<StoredSurfaceResource, String> {
    let bytes = fs::read(path).map_err(|error| format!("metadata is unreadable: {error}"))?;
    let stored: StoredSurfaceResource = serde_json::from_slice(&bytes)
        .map_err(|error| format!("metadata does not parse: {error}"))?;
    let digest = resource_digest(&stored.descriptor.resource_id)
        .map_err(|error| format!("metadata carries an unusable resource id: {error}"))?;
    let payload = root.join(format!("{digest}.bin"));
    let metadata =
        fs::metadata(&payload).map_err(|error| format!("payload is unreadable: {error}"))?;
    if !metadata.is_file() {
        return Err("payload is not a file".to_owned());
    }
    if metadata.len() != stored.descriptor.size {
        return Err(format!(
            "payload is {} bytes, metadata claims {}",
            metadata.len(),
            stored.descriptor.size
        ));
    }
    Ok(stored)
}
