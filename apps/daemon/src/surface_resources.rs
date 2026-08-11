use std::collections::BTreeMap;
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
}

impl SurfaceResourceStore {
    pub(crate) fn new(root: impl AsRef<Path>) -> Result<Self, SurfaceResourceStoreError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        let mut resources = BTreeMap::new();
        for entry in fs::read_dir(&root)? {
            let path = entry?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            if path.file_name().and_then(|value| value.to_str()) == Some("leases.json") {
                continue;
            }
            let stored: StoredSurfaceResource = serde_json::from_slice(&fs::read(&path)?)?;
            let digest = resource_digest(&stored.descriptor.resource_id)?;
            let payload = root.join(format!("{digest}.bin"));
            if !payload.is_file() || fs::metadata(&payload)?.len() != stored.descriptor.size {
                return Err(SurfaceResourceStoreError::Invalid(format!(
                    "resource metadata does not match payload: {}",
                    stored.descriptor.resource_id
                )));
            }
            resources.insert(stored.descriptor.resource_id.clone(), stored);
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
        }
        self.cleanup_expired();
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
        self.persist_leases()?;
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
        let bytes = fs::read(self.root.join(format!("{digest}.bin")))?;
        if bytes.len() as u64 != stored.descriptor.size || hex_digest(&bytes) != digest {
            return Err(SurfaceResourceStoreError::Invalid(format!(
                "resource payload failed integrity validation: {resource_id}"
            )));
        }
        Ok(SurfaceResourcePayload {
            descriptor: stored.descriptor.clone(),
            bytes,
        })
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
        let payload = self.get(&digest)?;
        if payload.descriptor != *resource {
            return Err(SurfaceResourceStoreError::Invalid(
                "Surface resource descriptor does not match the host object".to_owned(),
            ));
        }
        Ok(())
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
        let mut duplicated = lease.clone();
        duplicated.lease_id = format!("lease:{}", Uuid::new_v4());
        self.leases
            .insert(duplicated.lease_id.clone(), duplicated.clone());
        self.persist_leases()?;
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

    fn cleanup_expired(&mut self) {
        let now = unix_time_millis();
        self.leases.retain(|_, lease| lease.expires_at_ms > now);
    }

    fn persist_leases(&self) -> Result<(), SurfaceResourceStoreError> {
        let mut bytes = serde_json::to_vec_pretty(&self.leases)?;
        bytes.push(b'\n');
        write_atomic(&self.root.join("leases.json"), &bytes)
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
}
