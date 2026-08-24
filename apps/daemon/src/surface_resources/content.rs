use std::fs;

use loom_protocol::{
    SurfaceResourceDescriptor, SurfaceResourceKind, SurfaceResourceLease, SurfaceResourceTransport,
    SurfaceResourceTransportKind,
};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

use super::persistence::write_atomic;
use super::*;
use crate::unix_time_millis;

impl SurfaceResourceStore {
    /// Stores a payload by SHA-256 and mints an independently revocable lease for it.
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
        let resource_exists = if let Some(existing) = self.resources.get(&resource_id) {
            if existing.descriptor != descriptor {
                return Err(SurfaceResourceStoreError::Invalid(
                    "resource digest metadata conflicts with an existing object".to_owned(),
                ));
            }
            true
        } else {
            false
        };
        self.cleanup_expired();
        // Refuse before writing a new object; otherwise a full lease table can be used to leave
        // one durable orphan per rejected request until the GC grace period elapses.
        if self.leases.len() >= MAX_ACTIVE_RESOURCE_LEASES {
            return Err(SurfaceResourceStoreError::LeaseRejected(format!(
                "the host is already holding {MAX_ACTIVE_RESOURCE_LEASES} active resource leases"
            )));
        }
        if !resource_exists {
            write_atomic(&self.root.join(format!("{digest}.bin")), bytes)?;
            let stored = StoredSurfaceResource {
                descriptor: descriptor.clone(),
                created_at_ms: unix_time_millis(),
            };
            let mut metadata = serde_json::to_vec_pretty(&stored)?;
            metadata.push(b'\n');
            write_atomic(&self.root.join(format!("{digest}.json")), &metadata)?;
            self.resources.insert(resource_id.clone(), stored);
            // The payload was hashed before the atomic write, so this process can trust its stamp.
            if let Some(stamp) = self.payload_stamp(&digest) {
                self.verified.insert(resource_id.clone(), stamp);
            }
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

    /// Reads and hashes a stored payload before returning it to a caller.
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
        // Capture the stamp before reading so a concurrent replacement cannot be marked verified.
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

    /// Uses a verified file stamp for the hot path and re-hashes whenever that stamp changes.
    pub(crate) fn validate_descriptor(
        &mut self,
        resource: &SurfaceResourceDescriptor,
    ) -> Result<(), SurfaceResourceStoreError> {
        let digest = resource_digest(&resource.resource_id)?;
        let resource_id = format!("sha256:{digest}");
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
}

pub(super) fn resource_digest(resource_id: &str) -> Result<String, SurfaceResourceStoreError> {
    let digest = resource_id.strip_prefix("sha256:").ok_or_else(|| {
        SurfaceResourceStoreError::Invalid("resource id is not content addressed".to_owned())
    })?;
    normalize_digest(digest)
}

pub(super) fn normalize_digest(digest: &str) -> Result<String, SurfaceResourceStoreError> {
    let digest = digest.trim();
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(SurfaceResourceStoreError::Invalid(
            "resource digest must be a SHA-256 hex value".to_owned(),
        ));
    }
    Ok(digest.to_ascii_lowercase())
}

pub(super) fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
