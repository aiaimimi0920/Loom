//! Invocation-scoped, recoverable staging for Capability Plugin resource handles.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use fs2::FileExt as _;
use loom_capability_runtime::CapabilityStagedResource;
use loom_protocol::{
    ExtensionResourceKind, ExtensionResourceRef, SurfaceResourceKind,
    MAX_EXTENSION_INVOCATION_RESOURCE_BYTES,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::surface_resources::{
    SharedSurfaceResourceStore, SurfaceResourcePayload, SurfaceResourceStoreError,
};
use crate::unix_time_millis;

const MAX_ACTIVE_STAGING_LEASES: usize = 64;
const STAGING_TTL_MILLIS: u64 = 5 * 60 * 1000;

pub(crate) type SharedCapabilityResourceBroker = Arc<CapabilityResourceBroker>;

#[derive(Debug, Error)]
pub(crate) enum CapabilityResourceError {
    #[error("capability resource reference is invalid")]
    Invalid,
    #[error("capability resource lease was rejected")]
    LeaseRejected,
    #[error("capability resource staging is busy")]
    Busy,
    #[error("capability resource staging I/O failed")]
    Io(#[from] std::io::Error),
    #[error("capability resource staging journal failed")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapabilityResourceLeaseRecord {
    schema_version: u32,
    lease_id: String,
    plugin_id: String,
    owner_session: String,
    owner_scope: String,
    owner_request: String,
    created_at: u64,
    expires_at: u64,
    resources: Vec<CapabilityResourceLeaseEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CapabilityResourceLeaseEntry {
    resource_id: String,
    kind: ExtensionResourceKind,
    digest: String,
    byte_length: u64,
    refcount: u32,
    backing_location: String,
}

struct ActiveStagingLease {
    plugin_id: String,
}

pub(crate) struct CapabilityResourceBroker {
    _instance_lock: File,
    root: PathBuf,
    session_id: String,
    active: Mutex<HashMap<String, ActiveStagingLease>>,
}

pub(crate) struct CapabilityResourceLease {
    lease_id: Option<String>,
    resources: Vec<CapabilityStagedResource>,
    broker: Weak<CapabilityResourceBroker>,
}

impl CapabilityResourceBroker {
    pub(crate) fn open(
        root: impl AsRef<Path>,
    ) -> Result<SharedCapabilityResourceBroker, CapabilityResourceError> {
        let root = root.as_ref().to_path_buf();
        ensure_private_directory(&root)?;
        let instance_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("broker.lock"))?;
        loom_plugin_security::restrict_private_path_permissions(&root.join("broker.lock"), false)?;
        instance_lock
            .try_lock_exclusive()
            .map_err(|_| CapabilityResourceError::Busy)?;
        for state in ["prepared", "active"] {
            let state_root = root.join(state);
            ensure_private_directory(&state_root)?;
            remove_children(&state_root)?;
        }
        Ok(Arc::new(Self {
            _instance_lock: instance_lock,
            root,
            session_id: format!("session:{}", Uuid::new_v4().simple()),
            active: Mutex::new(HashMap::new()),
        }))
    }

    pub(crate) fn stage(
        self: &Arc<Self>,
        store: &SharedSurfaceResourceStore,
        plugin_id: &str,
        scope_id: &str,
        request_id: &str,
        references: &[ExtensionResourceRef],
    ) -> Result<CapabilityResourceLease, CapabilityResourceError> {
        if references.is_empty() {
            return Ok(CapabilityResourceLease {
                lease_id: None,
                resources: Vec::new(),
                broker: Arc::downgrade(self),
            });
        }
        let lease_id = Uuid::new_v4().simple().to_string();
        self.reserve(&lease_id, plugin_id)?;
        match self.stage_reserved(
            &lease_id, store, plugin_id, scope_id, request_id, references,
        ) {
            Ok(resources) => Ok(CapabilityResourceLease {
                lease_id: Some(lease_id),
                resources,
                broker: Arc::downgrade(self),
            }),
            Err(error) => {
                self.release(&lease_id);
                Err(error)
            }
        }
    }

    pub(crate) fn release_plugin(&self, plugin_id: &str) {
        let lease_ids = self
            .active
            .lock()
            .map(|active| {
                active
                    .iter()
                    .filter(|(_, lease)| lease.plugin_id == plugin_id)
                    .map(|(lease_id, _)| lease_id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for lease_id in lease_ids {
            self.release(&lease_id);
        }
    }

    fn reserve(&self, lease_id: &str, plugin_id: &str) -> Result<(), CapabilityResourceError> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| CapabilityResourceError::Busy)?;
        if active.len() >= MAX_ACTIVE_STAGING_LEASES {
            return Err(CapabilityResourceError::Busy);
        }
        active.insert(
            lease_id.to_owned(),
            ActiveStagingLease {
                plugin_id: plugin_id.to_owned(),
            },
        );
        Ok(())
    }

    fn stage_reserved(
        &self,
        lease_id: &str,
        store: &SharedSurfaceResourceStore,
        plugin_id: &str,
        scope_id: &str,
        request_id: &str,
        references: &[ExtensionResourceRef],
    ) -> Result<Vec<CapabilityStagedResource>, CapabilityResourceError> {
        let prepared = self.root.join("prepared").join(lease_id);
        let active = self.root.join("active").join(lease_id);
        ensure_private_directory(&prepared)?;
        let mut total = 0u64;
        let mut entries = Vec::with_capacity(references.len());
        for (index, reference) in references.iter().enumerate() {
            let payload = read_verified_resource(store, reference)?;
            total = total
                .checked_add(payload.descriptor.size)
                .ok_or(CapabilityResourceError::Invalid)?;
            if total > MAX_EXTENSION_INVOCATION_RESOURCE_BYTES {
                return Err(CapabilityResourceError::Invalid);
            }
            let file_name = format!("{index:03}-{}.bin", reference.digest);
            write_read_only(&prepared.join(&file_name), &payload.bytes)?;
            entries.push(CapabilityResourceLeaseEntry {
                resource_id: reference.resource_id.clone(),
                kind: reference.kind,
                digest: reference.digest.clone(),
                byte_length: reference.byte_length,
                refcount: 1,
                backing_location: file_name,
            });
        }
        let created_at = unix_time_millis();
        let record = CapabilityResourceLeaseRecord {
            schema_version: 1,
            lease_id: lease_id.to_owned(),
            plugin_id: plugin_id.to_owned(),
            owner_session: self.session_id.clone(),
            owner_scope: scope_id.to_owned(),
            owner_request: request_id.to_owned(),
            created_at,
            expires_at: created_at.saturating_add(STAGING_TTL_MILLIS),
            resources: entries,
        };
        let mut journal = serde_json::to_vec_pretty(&record)?;
        journal.push(b'\n');
        write_synced(&prepared.join("lease.json"), &journal)?;
        fs::rename(&prepared, &active)?;
        record
            .resources
            .into_iter()
            .zip(references.iter().cloned())
            .map(|(entry, resource_ref)| {
                Ok(CapabilityStagedResource {
                    resource_ref,
                    staged_path: active.join(entry.backing_location).canonicalize()?,
                })
            })
            .collect()
    }

    fn release(&self, lease_id: &str) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(lease_id);
        }
        for state in ["prepared", "active"] {
            let _ = fs::remove_dir_all(self.root.join(state).join(lease_id));
        }
    }
}

impl CapabilityResourceLease {
    pub(crate) fn resources(&self) -> &[CapabilityStagedResource] {
        &self.resources
    }
}

impl Drop for CapabilityResourceLease {
    fn drop(&mut self) {
        if let (Some(lease_id), Some(broker)) = (self.lease_id.as_deref(), self.broker.upgrade()) {
            broker.release(lease_id);
        }
    }
}

impl Drop for CapabilityResourceBroker {
    fn drop(&mut self) {
        let lease_ids = self
            .active
            .get_mut()
            .map(|active| active.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for lease_id in lease_ids {
            self.release(&lease_id);
        }
    }
}

fn read_verified_resource(
    store: &SharedSurfaceResourceStore,
    reference: &ExtensionResourceRef,
) -> Result<SurfaceResourcePayload, CapabilityResourceError> {
    if reference.resource_id != format!("sha256:{}", reference.digest) {
        return Err(CapabilityResourceError::Invalid);
    }
    let mut store = store.lock().map_err(|_| CapabilityResourceError::Busy)?;
    let payload = store
        .get_with_lease(&reference.digest, &reference.lease_id)
        .map_err(map_store_error)?;
    let kind_matches = match reference.kind {
        ExtensionResourceKind::SharedImage | ExtensionResourceKind::SharedMemory => {
            payload.descriptor.kind == SurfaceResourceKind::Image
        }
        ExtensionResourceKind::File => payload.descriptor.kind != SurfaceResourceKind::Image,
        ExtensionResourceKind::Inline => false,
    };
    if payload.descriptor.resource_id != reference.resource_id
        || payload.descriptor.size != reference.byte_length
        || !kind_matches
    {
        return Err(CapabilityResourceError::Invalid);
    }
    Ok(payload)
}

fn map_store_error(error: SurfaceResourceStoreError) -> CapabilityResourceError {
    match error {
        SurfaceResourceStoreError::LeaseRejected(_) | SurfaceResourceStoreError::NotFound(_) => {
            CapabilityResourceError::LeaseRejected
        }
        SurfaceResourceStoreError::Invalid(_) | SurfaceResourceStoreError::Json(_) => {
            CapabilityResourceError::Invalid
        }
        SurfaceResourceStoreError::Io(error) => CapabilityResourceError::Io(error),
    }
}

fn ensure_private_directory(path: &Path) -> Result<(), CapabilityResourceError> {
    fs::create_dir_all(path)?;
    loom_plugin_security::restrict_private_path_permissions(path, true)?;
    Ok(())
}

fn write_read_only(path: &Path, bytes: &[u8]) -> Result<(), CapabilityResourceError> {
    write_synced(path, bytes)?;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

fn write_synced(path: &Path, bytes: &[u8]) -> Result<(), CapabilityResourceError> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    loom_plugin_security::restrict_private_path_permissions(path, false)?;
    Ok(())
}

fn remove_children(root: &Path) -> Result<(), CapabilityResourceError> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || metadata.is_file() {
            fs::remove_file(path)?;
        } else if metadata.is_dir() {
            fs::remove_dir_all(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
