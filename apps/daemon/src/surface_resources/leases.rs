use loom_protocol::{
    SurfaceResourceDescriptor, SurfaceResourceKind, SurfaceResourceLease, SurfaceResourceTransport,
    SurfaceResourceTransportKind,
};
use uuid::Uuid;

use super::content::resource_digest;
use super::persistence::write_atomic;
use super::*;
use crate::unix_time_millis;

impl SurfaceResourceStore {
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
        // An expired grant must not become renewable merely because no other request has triggered
        // the store's lazy cleanup yet.
        self.cleanup_expired();
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
        // A late duplicate must receive a fresh default TTL without shortening a longer grant.
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

    pub(super) fn cleanup_expired(&mut self) {
        let now = unix_time_millis();
        self.leases.retain(|_, lease| lease.expires_at_ms > now);
    }

    /// Batches additions, while release and transport replacement remain immediately durable.
    pub(super) fn queue_lease_persist(&mut self) -> Result<(), SurfaceResourceStoreError> {
        self.leases_dirty = true;
        let now = unix_time_millis();
        if now.saturating_sub(self.leases_persisted_at_ms) < LEASE_PERSIST_DEBOUNCE_MILLIS {
            return Ok(());
        }
        self.persist_leases()
    }

    pub(super) fn persist_leases(&mut self) -> Result<(), SurfaceResourceStoreError> {
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
        // A destructor cannot report persistence failure; explicit mutations do propagate it.
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
