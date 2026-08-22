use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use loom_protocol::{
    SurfaceResourceDescriptor, SurfaceResourceKind, SurfaceResourceLease, SurfaceResourceTransport,
    SurfaceResourceTransportKind,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use uuid::Uuid;

use super::{create_sensitive_temporary, replace_sensitive_file, unix_time_millis};

pub(crate) const MAX_SURFACE_RESOURCE_BYTES: usize = 16 * 1024 * 1024;
const DEFAULT_RESOURCE_LEASE_MILLIS: u64 = 15 * 60 * 1000;
const MAX_RESOURCE_LEASE_MILLIS: u64 = 60 * 60 * 1000;
/// Upper bound on live leases. `MAX_RESOURCE_LEASE_MILLIS` alone lets an hour of grants pile up,
/// and every registration serializes the whole table, so the table needs a ceiling of its own.
/// The lease record carries no instance id, so the cap is global rather than per instance.
const MAX_ACTIVE_RESOURCE_LEASES: usize = 512;
/// How long lease additions may sit in memory before the table is written back. Every write is a
/// full re-serialization plus an `fsync` and an atomic replace, so a burst of registrations used to
/// pay that price once per lease. Removals are never debounced.
const LEASE_PERSIST_DEBOUNCE_MILLIS: u64 = 250;
/// Grace period before a stored object may be collected. `register` writes the payload and returns
/// a lease *before* the caller has persisted the Surface snapshot that carries it, so for a moment
/// an object is reachable only through the caller's stack. A sweep landing in that window would see
/// no instance reference, and — for a short `lease_millis` — no live lease either. Ten minutes is
/// far longer than that gap and far shorter than the default lease, so it never delays a real
/// collection by a meaningful amount.
const RESOURCE_GC_MIN_AGE_MILLIS: u64 = 10 * 60 * 1000;

/// What one `collect_garbage` pass did. Every field is reported so a sweep that deleted nothing
/// because it could not delete (`failures`) is distinguishable from one that deleted nothing
/// because everything is still referenced (`retained_objects`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SurfaceResourceGcOutcome {
    pub removed_objects: usize,
    pub removed_bytes: u64,
    pub removed_orphan_files: usize,
    pub retained_objects: usize,
    pub failures: usize,
}

pub(crate) type SharedSurfaceResourceStore = Arc<Mutex<SurfaceResourceStore>>;

#[derive(Debug, Error)]
pub(crate) enum SurfaceResourceStoreError {
    #[error("Surface resource I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Surface resource metadata is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid Surface resource: {0}")]
    Invalid(String),
    #[error("Surface resource was not found: {0}")]
    NotFound(String),
    #[error("Surface resource lease was rejected: {0}")]
    LeaseRejected(String),
}

impl SurfaceResourceStoreError {
    pub(crate) fn status_code(&self) -> u16 {
        match self {
            Self::Invalid(_) | Self::Json(_) => 400,
            Self::LeaseRejected(_) => 403,
            Self::NotFound(_) => 404,
            Self::Io(_) => 500,
        }
    }

    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Invalid(_) | Self::Json(_) => "invalid_surface_resource",
            Self::LeaseRejected(_) => "surface_resource_lease_rejected",
            Self::NotFound(_) => "surface_resource_not_found",
            Self::Io(_) => "surface_resource_io_failed",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSurfaceResource {
    descriptor: SurfaceResourceDescriptor,
    created_at_ms: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct SurfaceResourcePayload {
    pub descriptor: SurfaceResourceDescriptor,
    pub bytes: Vec<u8>,
}

pub(crate) struct SurfaceResourceStore {
    root: PathBuf,
    resources: BTreeMap<String, StoredSurfaceResource>,
    leases: BTreeMap<String, SurfaceResourceLease>,
    /// Payload identity, by resource id, as of the last successful hash verification in this
    /// process. A descriptor check can trust the in-memory metadata while the file still matches
    /// its stamp; a changed stamp forces the full read-and-hash again.
    verified: BTreeMap<String, PayloadStamp>,
    leases_dirty: bool,
    leases_persisted_at_ms: u64,
    /// Grace period `collect_garbage` applies, in milliseconds. Always `RESOURCE_GC_MIN_AGE_MILLIS`
    /// in a shipped build; a test lowers it to sweep an object it has just written, because a
    /// file's modification time cannot be backdated through `std::fs`.
    gc_min_age_ms: u64,
}

/// Size plus modification time of a payload file. `None` when the platform did not report a
/// modification time, in which case the payload is always re-hashed rather than trusted.
type PayloadStamp = (u64, u128);

impl SurfaceResourceStore {
    pub(crate) fn new(root: impl AsRef<Path>) -> Result<Self, SurfaceResourceStoreError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        let mut resources = BTreeMap::new();
        for entry in fs::read_dir(&root)? {
            let path = match entry {
                Ok(entry) => entry.path(),
                Err(error) => {
                    super::runtime_log_warn(format!(
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
            // One unreadable object used to abort this constructor, and `Daemon::new` propagates
            // that with `?` — so a single truncated payload or half-written metadata file stopped
            // the whole daemon from starting, with no recovery but deleting files by hand. The
            // store is content addressed, so a lost object is not lost information: the client can
            // register the same bytes again and get the same id. Discard the record and keep going.
            // The paired files are cleaned up by `collect_garbage`'s orphan sweep, not here, so a
            // payload that is merely unavailable for a moment is not destroyed on the way past.
            match load_stored_resource(&root, &path) {
                Ok(stored) => {
                    resources.insert(stored.descriptor.resource_id.clone(), stored);
                }
                Err(reason) => {
                    super::runtime_log_warn(format!(
                        "loom Surface resource store is discarding {}: {reason}",
                        path.display()
                    ));
                }
            }
        }
        let leases_path = root.join("leases.json");
        let leases = if leases_path.is_file() {
            serde_json::from_slice::<BTreeMap<String, SurfaceResourceLease>>(&fs::read(
                &leases_path,
            )?)?
        } else {
            BTreeMap::new()
        };
        let mut store = Self {
            root,
            resources,
            leases,
            verified: BTreeMap::new(),
            leases_dirty: false,
            leases_persisted_at_ms: 0,
            gc_min_age_ms: RESOURCE_GC_MIN_AGE_MILLIS,
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
        store.persist_leases()?;
        Ok(store)
    }

    pub(crate) fn register(
        &mut self,
        kind: SurfaceResourceKind,
        mime: &str,
        bytes: &[u8],
        width: Option<u32>,
        height: Option<u32>,
        lease_millis: Option<u64>,
    ) -> Result<SurfaceResourceLease, SurfaceResourceStoreError> {
        if bytes.is_empty() {
            return Err(SurfaceResourceStoreError::Invalid(
                "resource payload cannot be empty".to_owned(),
            ));
        }
        if bytes.len() > MAX_SURFACE_RESOURCE_BYTES {
            return Err(SurfaceResourceStoreError::Invalid(format!(
                "resource exceeds {MAX_SURFACE_RESOURCE_BYTES} bytes"
            )));
        }
        let mime = mime.trim();
        if mime.is_empty() || mime.len() > 160 || !mime.is_ascii() {
            return Err(SurfaceResourceStoreError::Invalid(
                "resource MIME type is invalid".to_owned(),
            ));
        }
        let digest = hex_digest(bytes);
        let resource_id = format!("sha256:{digest}");
        let descriptor = SurfaceResourceDescriptor {
            resource_id: resource_id.clone(),
            kind,
            mime: mime.to_owned(),
            size: bytes.len() as u64,
            width,
            height,
        };
        if let Some(existing) = self.resources.get(&resource_id) {
            if existing.descriptor != descriptor {
                return Err(SurfaceResourceStoreError::Invalid(
                    "resource digest metadata conflicts with an existing object".to_owned(),
                ));
            }
        } else {
            write_atomic(&self.root.join(format!("{digest}.bin")), bytes)?;
            let stored = StoredSurfaceResource {
                descriptor: descriptor.clone(),
                created_at_ms: unix_time_millis(),
            };
            let mut metadata = serde_json::to_vec_pretty(&stored)?;
            metadata.push(b'\n');
            write_atomic(&self.root.join(format!("{digest}.json")), &metadata)?;
            self.resources.insert(resource_id.clone(), stored);
            // The bytes were hashed above to derive `digest` and were just written, so the payload
            // on disk is verified content as of right now. Stamping it here keeps the first
            // `validate_descriptor` off the disk. The reuse branch deliberately does not stamp: it
            // never looked at the stored file, so it cannot vouch for it.
            if let Some(stamp) = self.payload_stamp(&digest) {
                self.verified.insert(resource_id.clone(), stamp);
            }
        }
        self.cleanup_expired();
        // Every registration mints a new lease id, so there is no existing entry to reuse: the cap
        // has to be a refusal. It is a clear error the caller can act on by releasing a lease,
        // which is safer than evicting a grant another attachment is still using.
        if self.leases.len() >= MAX_ACTIVE_RESOURCE_LEASES {
            return Err(SurfaceResourceStoreError::LeaseRejected(format!(
                "the host is already holding {MAX_ACTIVE_RESOURCE_LEASES} active resource leases"
            )));
        }
        let ttl = lease_millis
            .unwrap_or(DEFAULT_RESOURCE_LEASE_MILLIS)
            .clamp(1, MAX_RESOURCE_LEASE_MILLIS);
        let lease = SurfaceResourceLease {
            lease_id: format!("lease:{}", Uuid::new_v4()),
            resource: descriptor,
            transport: SurfaceResourceTransport {
                kind: SurfaceResourceTransportKind::LoomResource,
                handle: None,
                path: Some(format!("/v1/surfaces/resources/{digest}")),
                stream_id: None,
            },
            expires_at_ms: unix_time_millis().saturating_add(ttl),
        };
        self.leases.insert(lease.lease_id.clone(), lease.clone());
        self.queue_lease_persist()?;
        Ok(lease)
    }

    pub(crate) fn get(
        &mut self,
        digest: &str,
    ) -> Result<SurfaceResourcePayload, SurfaceResourceStoreError> {
        self.cleanup_expired();
        let digest = normalize_digest(digest)?;
        let resource_id = format!("sha256:{digest}");
        let stored = self
            .resources
            .get(&resource_id)
            .ok_or_else(|| SurfaceResourceStoreError::NotFound(resource_id.clone()))?;
        let descriptor = stored.descriptor.clone();
        // The stamp is taken before the read, not after: if the file is replaced while it is being
        // read, the digest check below fails rather than a stamp for unverified content being
        // recorded.
        let stamp = self.payload_stamp(&digest);
        let bytes = fs::read(self.root.join(format!("{digest}.bin")))?;
        if bytes.len() as u64 != descriptor.size || hex_digest(&bytes) != digest {
            self.verified.remove(&resource_id);
            return Err(SurfaceResourceStoreError::Invalid(format!(
                "resource payload failed integrity validation: {resource_id}"
            )));
        }
        match stamp {
            Some(stamp) => {
                self.verified.insert(resource_id, stamp);
            }
            None => {
                self.verified.remove(&resource_id);
            }
        }
        Ok(SurfaceResourcePayload { descriptor, bytes })
    }

    pub(crate) fn get_with_lease(
        &mut self,
        digest: &str,
        lease_id: &str,
    ) -> Result<SurfaceResourcePayload, SurfaceResourceStoreError> {
        self.cleanup_expired();
        let digest = normalize_digest(digest)?;
        let resource_id = format!("sha256:{digest}");
        let lease = self.leases.get(lease_id).ok_or_else(|| {
            SurfaceResourceStoreError::LeaseRejected("unknown or expired lease".to_owned())
        })?;
        if lease.resource.resource_id != resource_id {
            return Err(SurfaceResourceStoreError::LeaseRejected(
                "lease does not authorize the requested object".to_owned(),
            ));
        }
        self.get(&digest)
    }

    pub(crate) fn validate_references(
        &mut self,
        resources: &[SurfaceResourceDescriptor],
        leases: &[SurfaceResourceLease],
    ) -> Result<(), SurfaceResourceStoreError> {
        self.cleanup_expired();
        for resource in resources {
            self.validate_descriptor(resource)?;
        }
        for lease in leases {
            let stored = self.leases.get(&lease.lease_id).ok_or_else(|| {
                SurfaceResourceStoreError::LeaseRejected(
                    "unknown or expired lease in Surface update".to_owned(),
                )
            })?;
            if stored != lease {
                return Err(SurfaceResourceStoreError::LeaseRejected(
                    "Surface update lease does not match the host-issued lease".to_owned(),
                ));
            }
            self.validate_descriptor(&lease.resource)?;
        }
        Ok(())
    }

    pub(crate) fn validate_descriptor(
        &mut self,
        resource: &SurfaceResourceDescriptor,
    ) -> Result<(), SurfaceResourceStoreError> {
        let digest = resource_digest(&resource.resource_id)?;
        let resource_id = format!("sha256:{digest}");
        // `validate_references` runs on every snapshot and every patch, once per resource and once
        // per lease. Going through `get` there meant a full SHA-256 over up to 16 MiB per reference
        // per revision. Verification still happens on ingest (`register` hashes the bytes it is
        // given) and on the HTTP fetch path (`get`); here it is enough to confirm the descriptor
        // matches host metadata and that the payload file has not changed since it was last
        // verified in this process.
        if let Some(stamp) = self.verified.get(&resource_id).copied() {
            if self.payload_stamp(&digest) == Some(stamp) {
                let stored = self
                    .resources
                    .get(&resource_id)
                    .ok_or_else(|| SurfaceResourceStoreError::NotFound(resource_id.clone()))?;
                if stored.descriptor != *resource {
                    return Err(SurfaceResourceStoreError::Invalid(
                        "Surface resource descriptor does not match the host object".to_owned(),
                    ));
                }
                return Ok(());
            }
        }
        let payload = self.get(&digest)?;
        if payload.descriptor != *resource {
            return Err(SurfaceResourceStoreError::Invalid(
                "Surface resource descriptor does not match the host object".to_owned(),
            ));
        }
        Ok(())
    }

    /// Size and modification time of a payload file, or `None` when either is unavailable. A
    /// missing stamp means "cannot be trusted", so the caller falls back to a full re-hash.
    fn payload_stamp(&self, digest: &str) -> Option<PayloadStamp> {
        let metadata = fs::metadata(self.root.join(format!("{digest}.bin"))).ok()?;
        let modified = metadata
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos();
        Some((metadata.len(), modified))
    }

    pub(crate) fn replace_lease_transport(
        &mut self,
        lease_id: &str,
        transport: SurfaceResourceTransport,
    ) -> Result<SurfaceResourceLease, SurfaceResourceStoreError> {
        let lease = self.leases.get_mut(lease_id).ok_or_else(|| {
            SurfaceResourceStoreError::LeaseRejected("unknown or expired lease".to_owned())
        })?;
        validate_replacement_transport(&lease.resource, &transport)?;
        lease.transport = transport;
        let lease = lease.clone();
        self.persist_leases()?;
        Ok(lease)
    }

    pub(crate) fn release(
        &mut self,
        lease_id: &str,
    ) -> Result<Option<SurfaceResourceLease>, SurfaceResourceStoreError> {
        let removed = self.leases.remove(lease_id);
        if removed.is_some() {
            self.persist_leases()?;
        }
        Ok(removed)
    }

    pub(crate) fn duplicate_loom_resource_lease(
        &mut self,
        lease: &SurfaceResourceLease,
    ) -> Result<SurfaceResourceLease, SurfaceResourceStoreError> {
        let stored = self.leases.get(&lease.lease_id).ok_or_else(|| {
            SurfaceResourceStoreError::LeaseRejected(
                "cannot duplicate an unknown or expired lease".to_owned(),
            )
        })?;
        if stored != lease {
            return Err(SurfaceResourceStoreError::LeaseRejected(
                "cannot duplicate a lease that differs from host state".to_owned(),
            ));
        }
        if lease.transport.kind != SurfaceResourceTransportKind::LoomResource {
            return Err(SurfaceResourceStoreError::LeaseRejected(
                "shared Surface fanout requires independently leased Loom resources".to_owned(),
            ));
        }
        if self.leases.len() >= MAX_ACTIVE_RESOURCE_LEASES {
            return Err(SurfaceResourceStoreError::LeaseRejected(format!(
                "the host is already holding {MAX_ACTIVE_RESOURCE_LEASES} active resource leases"
            )));
        }
        let mut duplicated = lease.clone();
        duplicated.lease_id = format!("lease:{}", Uuid::new_v4());
        // A duplicate handed out 14 minutes into a 15-minute grant used to inherit that minute of
        // remaining life, and the receiving attachment then failed its next `validate_references`
        // with a lease rejection that looked like a protocol error. `renew_loom_resource_lease`
        // goes through `register` and already gets a full TTL; match it, and never shorten a lease
        // that happens to have longer left than the default.
        duplicated.expires_at_ms = duplicated
            .expires_at_ms
            .max(unix_time_millis().saturating_add(DEFAULT_RESOURCE_LEASE_MILLIS));
        self.leases
            .insert(duplicated.lease_id.clone(), duplicated.clone());
        self.queue_lease_persist()?;
        Ok(duplicated)
    }

    pub(crate) fn renew_loom_resource_lease(
        &mut self,
        lease: &SurfaceResourceLease,
    ) -> Result<SurfaceResourceLease, SurfaceResourceStoreError> {
        if lease.transport.kind != SurfaceResourceTransportKind::LoomResource {
            return Err(SurfaceResourceStoreError::LeaseRejected(
                "Surface recovery can only renew Loom resource leases".to_owned(),
            ));
        }
        let digest = resource_digest(&lease.resource.resource_id)?;
        let payload = self.get(&digest)?;
        if payload.descriptor != lease.resource {
            return Err(SurfaceResourceStoreError::Invalid(
                "recovered Surface resource descriptor differs from host content".to_owned(),
            ));
        }
        self.register(
            payload.descriptor.kind,
            &payload.descriptor.mime,
            &payload.bytes,
            payload.descriptor.width,
            payload.descriptor.height,
            None,
        )
    }

    /// Lowers the GC grace period. Only a test needs this: it has to sweep an object it wrote
    /// moments ago, and a file's modification time cannot be backdated through `std::fs`.
    #[cfg(test)]
    fn set_gc_min_age_ms(&mut self, value: u64) {
        self.gc_min_age_ms = value;
    }

    /// Deletes stored objects nothing can still reach: no live lease, and no reference from any
    /// Surface instance the caller knows about.
    ///
    /// `referenced_resource_ids` is supplied by the caller instead of being read from the instance
    /// store here, for two reasons. `delete_surface_instance` fixes the lock order — the
    /// instance-store lock is dropped *before* this store's lock is taken — so reaching back into
    /// the instance store from inside this call would invert it. And the caller is free to
    /// over-approximate: it scans the serialized instance records for anything shaped like a
    /// content address, so a reference this store has never issued a lease for still protects its
    /// object. Over-retention is the only safe direction; being wrong the other way deletes a
    /// resource a live Surface is still painting with.
    ///
    /// In-memory leases alone are **not** a sufficient reference set. Leases expire in fifteen
    /// minutes by default and are dropped outright when the process restarts, while a persisted
    /// Surface instance keeps referring to its resources indefinitely — so a leases-only sweep
    /// after a restart would delete exactly the objects still in use.
    ///
    /// Objects younger than `gc_min_age_ms` are never swept; see that field and
    /// `RESOURCE_GC_MIN_AGE_MILLIS` for why the window exists.
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
                super::runtime_log_warn(format!(
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
                    // Not reachable through `register`, which derives the id from the bytes. A
                    // record that cannot be turned into a digest also cannot be turned into a file
                    // path, so there is nothing here to delete.
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

    /// Deletes both halves of one stored object, metadata first so that a crash in between leaves a
    /// payload the orphan sweep can finish rather than a record pointing at nothing. An already
    /// absent file counts as deleted.
    fn remove_object_files(&self, digest: &str) -> bool {
        let mut removed = true;
        for extension in ["json", "bin"] {
            let path = self.root.join(format!("{digest}.{extension}"));
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    super::runtime_log_warn(format!(
                        "loom Surface resource GC could not delete {}: {error}",
                        path.display()
                    ));
                    removed = false;
                }
            }
        }
        removed
    }

    /// Deletes files in the store directory that no in-memory record claims: the metadata of an
    /// object discarded at load, and the payload of a registration that died between its two
    /// writes. Returns `(deleted, failures)`.
    ///
    /// Only the two extensions this store writes are ever considered, so a `write_atomic`
    /// temporary — `.{name}.tmp-{pid}-{nonce}-{attempt}`, whose extension is the `tmp-…` tail — can
    /// never be a candidate, and neither can `leases.json`. The same age guard as the record sweep
    /// applies, read from the file's modification time; a file whose age cannot be established is
    /// treated as young and kept.
    fn sweep_orphan_files(&self, live: &BTreeSet<String>, now: u64) -> (usize, usize) {
        let known: BTreeSet<String> = self
            .resources
            .keys()
            .filter_map(|resource_id| resource_digest(resource_id).ok())
            .collect();
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) => {
                super::runtime_log_warn(format!(
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
            let old_enough = file_modified_millis(&path).map_or(false, |modified| {
                now.saturating_sub(modified) >= self.gc_min_age_ms
            });
            if !old_enough {
                continue;
            }
            match fs::remove_file(&path) {
                Ok(()) => deleted += 1,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    super::runtime_log_warn(format!(
                        "loom Surface resource GC could not delete orphan {}: {error}",
                        path.display()
                    ));
                    failures += 1;
                }
            }
        }
        (deleted, failures)
    }

    fn cleanup_expired(&mut self) {
        let now = unix_time_millis();
        self.leases.retain(|_, lease| lease.expires_at_ms > now);
    }

    /// Records a lease addition and writes the table back only when the debounce window has
    /// elapsed. A lease that is still only in memory when the process dies is simply gone on the
    /// next start, which is the same outcome the caller already handles for an expired lease: the
    /// resource payload itself is written durably before the lease is minted, so the client can
    /// register again and get a new grant over the same object.
    fn queue_lease_persist(&mut self) -> Result<(), SurfaceResourceStoreError> {
        self.leases_dirty = true;
        let now = unix_time_millis();
        if now.saturating_sub(self.leases_persisted_at_ms) < LEASE_PERSIST_DEBOUNCE_MILLIS {
            return Ok(());
        }
        self.persist_leases()
    }

    fn persist_leases(&mut self) -> Result<(), SurfaceResourceStoreError> {
        let mut bytes = serde_json::to_vec_pretty(&self.leases)?;
        bytes.push(b'\n');
        write_atomic(&self.root.join("leases.json"), &bytes)?;
        self.leases_dirty = false;
        self.leases_persisted_at_ms = unix_time_millis();
        Ok(())
    }
}

impl Drop for SurfaceResourceStore {
    fn drop(&mut self) {
        // Flush whatever the debounce window is still holding. A failure here has nowhere to go and
        // nothing left to protect, so it is dropped rather than logged from a destructor.
        if self.leases_dirty {
            let _ = self.persist_leases();
        }
    }
}

fn validate_replacement_transport(
    resource: &SurfaceResourceDescriptor,
    transport: &SurfaceResourceTransport,
) -> Result<(), SurfaceResourceStoreError> {
    match transport.kind {
        SurfaceResourceTransportKind::LoomResource => {
            let digest = resource_digest(&resource.resource_id)?;
            if transport.handle.is_some()
                || transport.stream_id.is_some()
                || transport.path.as_deref()
                    != Some(format!("/v1/surfaces/resources/{digest}").as_str())
            {
                return Err(SurfaceResourceStoreError::Invalid(
                    "loom_resource transport descriptor is invalid".to_owned(),
                ));
            }
        }
        SurfaceResourceTransportKind::SharedMemory => {
            let handle = transport.handle.as_deref().unwrap_or_default();
            let expected_size = resource
                .width
                .zip(resource.height)
                .and_then(|(width, height)| {
                    u64::from(width)
                        .checked_mul(u64::from(height))
                        .and_then(|pixels| pixels.checked_mul(4))
                });
            if resource.kind != SurfaceResourceKind::Image
                || resource.mime != "application/x-neuro-rgba8"
                || expected_size != Some(resource.size)
                || handle.is_empty()
                || handle.len() > 256
                || !handle.is_ascii()
                || transport.path.is_some()
                || transport.stream_id.is_some()
            {
                return Err(SurfaceResourceStoreError::Invalid(
                    "shared_memory transport descriptor is invalid".to_owned(),
                ));
            }
        }
        SurfaceResourceTransportKind::Stream => {
            return Err(SurfaceResourceStoreError::Invalid(
                "stream transport cannot replace a resource lease".to_owned(),
            ));
        }
    }
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), SurfaceResourceStoreError> {
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

/// Reads one `{digest}.json` metadata record and confirms its payload is present and the right
/// length. The failure is a plain reason string rather than a `SurfaceResourceStoreError` because
/// the only caller discards the record and logs the reason — none of these conditions is a client
/// error, so none of them should ever be shaped like a response.
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

/// Modification time of a file as Unix milliseconds, or `None` when the platform did not report
/// one. Used only by the GC age guard, where an unknown time means "too young to touch".
fn file_modified_millis(path: &Path) -> Option<u64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    let since_epoch = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    u64::try_from(since_epoch.as_millis()).ok()
}

fn resource_digest(resource_id: &str) -> Result<String, SurfaceResourceStoreError> {
    let digest = resource_id.strip_prefix("sha256:").ok_or_else(|| {
        SurfaceResourceStoreError::Invalid("resource id is not content addressed".to_owned())
    })?;
    normalize_digest(digest)
}

fn normalize_digest(digest: &str) -> Result<String, SurfaceResourceStoreError> {
    let digest = digest.trim();
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(SurfaceResourceStoreError::Invalid(
            "resource digest must be a SHA-256 hex value".to_owned(),
        ));
    }
    Ok(digest.to_ascii_lowercase())
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resources_are_content_addressed_reused_and_verified_after_restart() {
        let root = std::env::temp_dir().join(format!("loom-surface-resources-{}", Uuid::new_v4()));
        let first = {
            let mut store = SurfaceResourceStore::new(&root).expect("open store");
            let first = store
                .register(
                    SurfaceResourceKind::Image,
                    "image/png",
                    b"fixture-image",
                    Some(2),
                    Some(3),
                    None,
                )
                .expect("register resource");
            let second = store
                .register(
                    SurfaceResourceKind::Image,
                    "image/png",
                    b"fixture-image",
                    Some(2),
                    Some(3),
                    None,
                )
                .expect("reuse resource");
            assert_eq!(first.resource, second.resource);
            assert_ne!(first.lease_id, second.lease_id);
            first
        };
        let mut reloaded = SurfaceResourceStore::new(&root).expect("reload store");
        let digest = first
            .resource
            .resource_id
            .strip_prefix("sha256:")
            .expect("digest");
        let payload = reloaded
            .get_with_lease(digest, &first.lease_id)
            .expect("read leased resource after restart");
        assert_eq!(payload.bytes, b"fixture-image");
        assert_eq!(payload.descriptor, first.resource);
        reloaded
            .validate_references(
                std::slice::from_ref(&first.resource),
                std::slice::from_ref(&first),
            )
            .expect("validate host-issued resource references");
        let mut forged = first.clone();
        forged.expires_at_ms = forged.expires_at_ms.saturating_add(1);
        assert!(matches!(
            reloaded.validate_references(&[], &[forged]),
            Err(SurfaceResourceStoreError::LeaseRejected(_))
        ));
        let mut forged_descriptor = first.resource.clone();
        forged_descriptor.mime = "image/webp".to_owned();
        assert!(matches!(
            reloaded.validate_references(&[forged_descriptor], &[]),
            Err(SurfaceResourceStoreError::Invalid(_))
        ));
        assert!(matches!(
            reloaded.replace_lease_transport(
                &first.lease_id,
                SurfaceResourceTransport {
                    kind: SurfaceResourceTransportKind::Stream,
                    handle: None,
                    path: None,
                    stream_id: Some("stream:forged".to_owned()),
                },
            ),
            Err(SurfaceResourceStoreError::Invalid(_))
        ));
        assert!(matches!(
            reloaded.get_with_lease(digest, "lease:missing"),
            Err(SurfaceResourceStoreError::LeaseRejected(_))
        ));
        assert!(reloaded
            .release(&first.lease_id)
            .expect("release lease")
            .is_some());
        assert!(matches!(
            reloaded.get_with_lease(digest, &first.lease_id),
            Err(SurfaceResourceStoreError::LeaseRejected(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn persisted_resource_can_receive_a_fresh_lease_after_the_old_lease_expires() {
        let root = std::env::temp_dir().join(format!("loom-surface-renew-{}", Uuid::new_v4()));
        let mut store = SurfaceResourceStore::new(&root).expect("open store");
        let expired = store
            .register(
                SurfaceResourceKind::Binary,
                "application/octet-stream",
                b"persistent-resource",
                None,
                None,
                None,
            )
            .expect("register resource");
        store
            .leases
            .get_mut(&expired.lease_id)
            .expect("stored lease")
            .expires_at_ms = 0;
        store.persist_leases().expect("persist expired lease");

        let renewed = store
            .renew_loom_resource_lease(&expired)
            .expect("renew persisted resource lease");
        assert_ne!(renewed.lease_id, expired.lease_id);
        assert_eq!(renewed.resource, expired.resource);
        assert!(renewed.expires_at_ms > unix_time_millis());
        let digest = renewed
            .resource
            .resource_id
            .strip_prefix("sha256:")
            .expect("digest");
        assert_eq!(
            store
                .get_with_lease(digest, &renewed.lease_id)
                .expect("read renewed resource")
                .bytes,
            b"persistent-resource"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn descriptor_validation_trusts_an_unchanged_payload_and_re_verifies_a_changed_one() {
        let root = std::env::temp_dir().join(format!("loom-surface-stamp-{}", Uuid::new_v4()));
        let mut store = SurfaceResourceStore::new(&root).expect("open store");
        let lease = store
            .register(
                SurfaceResourceKind::Binary,
                "application/octet-stream",
                b"stamped-payload",
                None,
                None,
                None,
            )
            .expect("register resource");
        let digest = lease
            .resource
            .resource_id
            .strip_prefix("sha256:")
            .expect("digest")
            .to_owned();
        let payload_path = root.join(format!("{digest}.bin"));

        store
            .validate_descriptor(&lease.resource)
            .expect("registration stamps the payload it just wrote");

        // Tamper with the payload but keep its length and modification time, so the stamp still
        // matches. The descriptor check is expected to trust the stamp and stay off the disk, which
        // is exactly what makes it cheap; the fetch path is the one that still hashes.
        let times = fs::FileTimes::new()
            .set_accessed(
                fs::metadata(&payload_path)
                    .expect("payload metadata")
                    .accessed()
                    .expect("accessed time"),
            )
            .set_modified(
                fs::metadata(&payload_path)
                    .expect("payload metadata")
                    .modified()
                    .expect("modified time"),
            );
        // Same byte length as the original, so only the content differs.
        assert_eq!(b"tampered-paylod".len(), b"stamped-payload".len());
        fs::write(&payload_path, b"tampered-paylod").expect("tamper with the payload");
        fs::File::options()
            .write(true)
            .open(&payload_path)
            .expect("reopen payload")
            .set_times(times)
            .expect("restore the payload timestamps");

        store
            .validate_descriptor(&lease.resource)
            .expect("an unchanged stamp is trusted without a re-hash");
        assert!(matches!(
            store.get(&digest),
            Err(SurfaceResourceStoreError::Invalid(_))
        ));
        // The failed fetch dropped the stamp, so the next descriptor check re-hashes and fails too.
        assert!(matches!(
            store.validate_descriptor(&lease.resource),
            Err(SurfaceResourceStoreError::Invalid(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lease_registration_is_capped_and_debounced_but_still_survives_a_restart() {
        let root = std::env::temp_dir().join(format!("loom-surface-leases-{}", Uuid::new_v4()));
        let (first_id, second_id) = {
            let mut store = SurfaceResourceStore::new(&root).expect("open store");
            let first = store
                .register(
                    SurfaceResourceKind::Binary,
                    "application/octet-stream",
                    b"lease-payload-one",
                    None,
                    None,
                    None,
                )
                .expect("register the first resource");
            let second = store
                .register(
                    SurfaceResourceKind::Binary,
                    "application/octet-stream",
                    b"lease-payload-two",
                    None,
                    None,
                    None,
                )
                .expect("register the second resource");
            assert!(
                store.leases_dirty,
                "the second registration inside the debounce window must not have written the table"
            );

            let template = second.clone();
            while store.leases.len() < MAX_ACTIVE_RESOURCE_LEASES {
                let mut filler = template.clone();
                filler.lease_id = format!("lease:{}", Uuid::new_v4());
                store.leases.insert(filler.lease_id.clone(), filler);
            }
            let error = store
                .register(
                    SurfaceResourceKind::Binary,
                    "application/octet-stream",
                    b"lease-payload-three",
                    None,
                    None,
                    None,
                )
                .expect_err("a full lease table must refuse a new grant");
            assert!(matches!(error, SurfaceResourceStoreError::LeaseRejected(_)));
            assert!(matches!(
                store.duplicate_loom_resource_lease(&second),
                Err(SurfaceResourceStoreError::LeaseRejected(_))
            ));

            store
                .leases
                .retain(|lease_id, _| *lease_id == first.lease_id || *lease_id == second.lease_id);
            (first.lease_id, second.lease_id)
        };

        let reloaded = SurfaceResourceStore::new(&root).expect("reload store");
        assert!(
            reloaded.leases.contains_key(&first_id),
            "dropping the store must flush the debounced lease table"
        );
        assert!(reloaded.leases.contains_key(&second_id));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_duplicated_lease_gets_a_fresh_ttl_without_shortening_a_longer_one() {
        let root = std::env::temp_dir().join(format!("loom-surface-duplicate-{}", Uuid::new_v4()));
        let mut store = SurfaceResourceStore::new(&root).expect("open store");
        let short = store
            .register(
                SurfaceResourceKind::Binary,
                "application/octet-stream",
                b"almost-expired",
                None,
                None,
                Some(2_000),
            )
            .expect("register with a short lease");
        let duplicated = store
            .duplicate_loom_resource_lease(&short)
            .expect("duplicate the short lease");
        assert_ne!(duplicated.lease_id, short.lease_id);
        assert_eq!(duplicated.resource, short.resource);
        assert!(
            duplicated.expires_at_ms
                >= unix_time_millis().saturating_add(DEFAULT_RESOURCE_LEASE_MILLIS) - 5_000,
            "a duplicate must not inherit an almost-expired grant"
        );

        let long = store
            .register(
                SurfaceResourceKind::Binary,
                "application/octet-stream",
                b"long-lived",
                None,
                None,
                Some(MAX_RESOURCE_LEASE_MILLIS),
            )
            .expect("register with the longest allowed lease");
        let duplicated_long = store
            .duplicate_loom_resource_lease(&long)
            .expect("duplicate the long lease");
        assert_eq!(duplicated_long.expires_at_ms, long.expires_at_ms);
        let _ = fs::remove_dir_all(root);
    }

    /// A payload that went missing, or that does not match the length its metadata claims, used to
    /// make `new` return `Invalid` — and `Daemon::new` propagates that with `?`, so one damaged
    /// object stopped the whole daemon from starting.
    #[test]
    fn a_damaged_object_is_discarded_at_load_instead_of_failing_the_store() {
        let root = std::env::temp_dir().join(format!("loom-surface-damaged-{}", Uuid::new_v4()));
        let (missing, truncated, intact) = {
            let mut store = SurfaceResourceStore::new(&root).expect("open store");
            let missing = store
                .register(
                    SurfaceResourceKind::Binary,
                    "application/octet-stream",
                    b"payload-will-be-deleted",
                    None,
                    None,
                    None,
                )
                .expect("register the object whose payload is deleted");
            let truncated = store
                .register(
                    SurfaceResourceKind::Binary,
                    "application/octet-stream",
                    b"payload-will-be-truncated",
                    None,
                    None,
                    None,
                )
                .expect("register the object whose payload shrinks");
            let intact = store
                .register(
                    SurfaceResourceKind::Binary,
                    "application/octet-stream",
                    b"payload-stays-whole",
                    None,
                    None,
                    None,
                )
                .expect("register the object that survives");
            (missing, truncated, intact)
        };
        let digest_of = |lease: &SurfaceResourceLease| {
            resource_digest(&lease.resource.resource_id).expect("content addressed lease")
        };
        let missing_digest = digest_of(&missing);
        let truncated_digest = digest_of(&truncated);
        fs::remove_file(root.join(format!("{missing_digest}.bin"))).expect("delete a payload");
        fs::write(root.join(format!("{truncated_digest}.bin")), b"x").expect("shrink a payload");

        let mut reloaded =
            SurfaceResourceStore::new(&root).expect("a damaged object must not fail");
        assert_eq!(
            reloaded.resources.keys().collect::<Vec<_>>(),
            vec![&intact.resource.resource_id],
            "only the intact object may be loaded"
        );
        assert!(
            !reloaded.leases.contains_key(&missing.lease_id)
                && !reloaded.leases.contains_key(&truncated.lease_id),
            "a lease over a discarded object must not survive the load"
        );
        let intact_digest = digest_of(&intact);
        assert_eq!(
            reloaded
                .get_with_lease(&intact_digest, &intact.lease_id)
                .expect("the intact object stays readable")
                .bytes,
            b"payload-stays-whole"
        );
        let _ = fs::remove_dir_all(root);
    }

    /// The reference set, not the lease table, is what keeps an object alive. Leases expire in
    /// fifteen minutes and do not survive a restart, so a sweep that consulted only the leases
    /// would delete exactly the objects a persisted Surface instance is still painting with.
    #[test]
    fn gc_keeps_an_object_a_reference_still_names_after_its_lease_is_gone() {
        let root = std::env::temp_dir().join(format!("loom-surface-gc-ref-{}", Uuid::new_v4()));
        let mut store = SurfaceResourceStore::new(&root).expect("open store");
        store.set_gc_min_age_ms(0);
        let lease = store
            .register(
                SurfaceResourceKind::Binary,
                "application/octet-stream",
                b"referenced-by-an-instance",
                None,
                None,
                None,
            )
            .expect("register resource");
        let digest = resource_digest(&lease.resource.resource_id).expect("content addressed lease");
        let payload = root.join(format!("{digest}.bin"));
        let metadata = root.join(format!("{digest}.json"));
        store.release(&lease.lease_id).expect("release the lease");

        let mut referenced = BTreeSet::new();
        referenced.insert(lease.resource.resource_id.clone());
        let kept = store.collect_garbage(&referenced);
        assert_eq!(kept.removed_objects, 0);
        assert_eq!(kept.removed_orphan_files, 0);
        assert_eq!(kept.retained_objects, 1);
        assert_eq!(kept.failures, 0);
        assert!(payload.is_file() && metadata.is_file());

        let swept = store.collect_garbage(&BTreeSet::new());
        assert_eq!(swept.removed_objects, 1);
        assert_eq!(swept.removed_bytes, lease.resource.size);
        assert_eq!(swept.retained_objects, 0);
        assert_eq!(swept.failures, 0);
        assert!(!payload.exists() && !metadata.exists());
        assert!(store.resources.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    /// `register` hands back a lease before its caller has persisted the snapshot carrying it, so a
    /// brand new object is briefly reachable only through the caller's stack. The grace period is
    /// what stops a sweep landing in that window from deleting it.
    #[test]
    fn gc_leaves_a_young_unreferenced_object_alone() {
        let root = std::env::temp_dir().join(format!("loom-surface-gc-young-{}", Uuid::new_v4()));
        let mut store = SurfaceResourceStore::new(&root).expect("open store");
        let lease = store
            .register(
                SurfaceResourceKind::Binary,
                "application/octet-stream",
                b"just-registered",
                None,
                None,
                Some(1),
            )
            .expect("register with the shortest possible lease");
        let digest = resource_digest(&lease.resource.resource_id).expect("content addressed lease");
        store.release(&lease.lease_id).expect("release the lease");

        let outcome = store.collect_garbage(&BTreeSet::new());
        assert_eq!(outcome.removed_objects, 0);
        assert_eq!(outcome.retained_objects, 1);
        assert!(root.join(format!("{digest}.bin")).is_file());
        let _ = fs::remove_dir_all(root);
    }

    /// The orphan sweep exists for the halves left behind by a discarded record or a registration
    /// that died between its two writes. It must not reach anything else in the directory.
    #[test]
    fn gc_sweeps_orphan_halves_but_not_the_lease_table_or_a_temporary() {
        let root = std::env::temp_dir().join(format!("loom-surface-gc-orphan-{}", Uuid::new_v4()));
        let mut store = SurfaceResourceStore::new(&root).expect("open store");
        store.set_gc_min_age_ms(0);
        let orphan_payload = root.join(format!("{}.bin", "ab".repeat(32)));
        let orphan_metadata = root.join(format!("{}.json", "cd".repeat(32)));
        let temporary = root.join(format!(".{}.bin.tmp-4242-abc-0", "ef".repeat(32)));
        let unrelated = root.join("notes.txt");
        for path in [&orphan_payload, &orphan_metadata, &temporary, &unrelated] {
            fs::write(path, b"stray").expect("write a stray file");
        }

        let outcome = store.collect_garbage(&BTreeSet::new());
        assert_eq!(outcome.removed_orphan_files, 2);
        assert_eq!(outcome.removed_objects, 0);
        assert_eq!(outcome.failures, 0);
        assert!(!orphan_payload.exists() && !orphan_metadata.exists());
        assert!(
            temporary.is_file(),
            "a write_atomic temporary is not an orphan; its extension is the tmp-... tail"
        );
        assert!(unrelated.is_file(), "the sweep only knows .bin and .json");
        assert!(
            root.join("leases.json").is_file(),
            "the lease table must never be swept"
        );
        let _ = fs::remove_dir_all(root);
    }
}
