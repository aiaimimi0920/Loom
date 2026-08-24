use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use super::content::{normalize_digest, resource_digest};
use super::*;
use crate::{runtime_log_warn, unix_time_millis};

impl SurfaceResourceStore {
    /// Lowers the grace period only for tests that must sweep a freshly written object.
    #[cfg(test)]
    pub(super) fn set_gc_min_age_ms(&mut self, value: u64) {
        self.gc_min_age_ms = value;
    }

    /// Deletes objects unreachable from both live leases and the caller's instance reference set.
    ///
    /// The caller supplies references so it can release the instance-store lock before taking this
    /// store's lock. Over-approximation is intentional: retaining too much is safer than deleting a
    /// payload that a persisted Surface instance still uses after its lease expires.
    pub(crate) fn collect_garbage(
        &mut self,
        referenced_resource_ids: &BTreeSet<String>,
    ) -> SurfaceResourceGcOutcome {
        let mut outcome = SurfaceResourceGcOutcome::default();
        let leases_before = self.leases.len();
        self.cleanup_expired();
        if self.leases.len() != leases_before {
            if let Err(error) = self.persist_leases() {
                outcome.failures += 1;
                runtime_log_warn(format!(
                    "loom Surface resource GC could not persist the lease table: {error}"
                ));
            }
        }

        let mut live: BTreeSet<String> = BTreeSet::new();
        for resource_id in referenced_resource_ids {
            if let Ok(digest) = resource_digest(resource_id) {
                live.insert(digest);
            }
        }
        for lease in self.leases.values() {
            if let Ok(digest) = resource_digest(&lease.resource.resource_id) {
                live.insert(digest);
            }
        }

        let now = unix_time_millis();
        let mut condemned: Vec<(String, String, u64)> = Vec::new();
        for (resource_id, stored) in &self.resources {
            let digest = match resource_digest(resource_id) {
                Ok(digest) => digest,
                Err(_) => {
                    // `register` cannot create this state, and no safe file path can be derived.
                    outcome.retained_objects += 1;
                    continue;
                }
            };
            if live.contains(&digest)
                || now.saturating_sub(stored.created_at_ms) < self.gc_min_age_ms
            {
                outcome.retained_objects += 1;
                continue;
            }
            condemned.push((resource_id.clone(), digest, stored.descriptor.size));
        }

        for (resource_id, digest, size) in condemned {
            if self.remove_object_files(&digest) {
                self.resources.remove(&resource_id);
                self.verified.remove(&resource_id);
                outcome.removed_objects += 1;
                outcome.removed_bytes = outcome.removed_bytes.saturating_add(size);
            } else {
                outcome.failures += 1;
                outcome.retained_objects += 1;
            }
        }

        let (orphans, orphan_failures) = self.sweep_orphan_files(&live, now);
        outcome.removed_orphan_files += orphans;
        outcome.failures += orphan_failures;
        outcome
    }

    /// Deletes metadata before payload so a crash leaves a sweepable orphan, never a live record.
    fn remove_object_files(&self, digest: &str) -> bool {
        let mut removed = true;
        for extension in ["json", "bin"] {
            let path = self.root.join(format!("{digest}.{extension}"));
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    runtime_log_warn(format!(
                        "loom Surface resource GC could not delete {}: {error}",
                        path.display()
                    ));
                    removed = false;
                }
            }
        }
        removed
    }

    /// Sweeps only old `.bin`/`.json` files not claimed by a record or live reference.
    fn sweep_orphan_files(&self, live: &BTreeSet<String>, now: u64) -> (usize, usize) {
        let known: BTreeSet<String> = self
            .resources
            .keys()
            .filter_map(|resource_id| resource_digest(resource_id).ok())
            .collect();
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) => {
                runtime_log_warn(format!(
                    "loom Surface resource GC could not scan {}: {error}",
                    self.root.display()
                ));
                return (0, 1);
            }
        };
        let mut deleted = 0;
        let mut failures = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.file_name().and_then(|value| value.to_str()) == Some("leases.json") {
                continue;
            }
            let digest = match path
                .extension()
                .and_then(|value| value.to_str())
                .filter(|extension| matches!(*extension, "bin" | "json"))
                .and_then(|_| path.file_stem())
                .and_then(|value| value.to_str())
                .and_then(|stem| normalize_digest(stem).ok())
            {
                Some(digest) => digest,
                None => continue,
            };
            if known.contains(&digest) || live.contains(&digest) {
                continue;
            }
            let old_enough = file_modified_millis(&path)
                .is_some_and(|modified| now.saturating_sub(modified) >= self.gc_min_age_ms);
            if !old_enough {
                continue;
            }
            match fs::remove_file(&path) {
                Ok(()) => deleted += 1,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    runtime_log_warn(format!(
                        "loom Surface resource GC could not delete orphan {}: {error}",
                        path.display()
                    ));
                    failures += 1;
                }
            }
        }
        (deleted, failures)
    }
}

/// Unknown modification time means too young to delete.
fn file_modified_millis(path: &Path) -> Option<u64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    let since_epoch = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    u64::try_from(since_epoch.as_millis()).ok()
}
