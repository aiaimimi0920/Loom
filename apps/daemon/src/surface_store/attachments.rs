// Attachment ownership plus snapshot and patch mutations for mounted Surface instances.
impl SurfaceInstanceStore {
    pub(crate) fn attach(
        &mut self,
        instance_id: &str,
        hook_node_id: &str,
        device_id: &str,
        host_capabilities: Option<SurfaceHostCapabilities>,
    ) -> Result<SurfaceAttachmentRecord, SurfaceStoreError> {
        validate_identity(hook_node_id, "Hook node id")?;
        validate_identity(device_id, "device id")?;
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            if let Some(existing) = instance.attachments.values().find(|attachment| {
                attachment.descriptor.hook_node_id == hook_node_id
                    && attachment.descriptor.device_id == device_id
            }) {
                return Ok(existing.clone());
            }
            let attachment_id = format!("attachment:{}", Uuid::new_v4());
            let record = SurfaceAttachmentRecord {
                descriptor: SurfaceAttachmentDescriptor {
                    attachment_id: attachment_id.clone(),
                    instance_id: instance_id.to_owned(),
                    hook_node_id: hook_node_id.to_owned(),
                    device_id: device_id.to_owned(),
                },
                lifecycle: SurfaceLifecycleState::Created,
                lifecycle_revision: 0,
                host_capabilities,
                snapshot: None,
            };
            instance.attachments.insert(attachment_id, record.clone());
            instance.updated_at_ms = unix_time_millis();
            Ok(record)
        })
    }

    pub(crate) fn put_snapshot(
        &mut self,
        instance_id: &str,
        mut snapshot: SurfaceSnapshot,
    ) -> Result<SurfaceInstanceRecord, SurfaceStoreError> {
        validate_surface_snapshot(&snapshot)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            validate_snapshot_identity(instance, instance_id, &snapshot)?;
            let has_authoritative_state = !instance.authoritative_state.is_null()
                && !instance
                    .authoritative_state
                    .as_object()
                    .is_some_and(serde_json::Map::is_empty);
            if !has_authoritative_state {
                instance.authoritative_state = snapshot.authoritative_state.clone();
            } else {
                snapshot.authoritative_state = instance.authoritative_state.clone();
            }
            let attachment = attachment_mut(instance, &snapshot.attachment_id)?;
            if let Some(previous) = attachment.snapshot.as_ref() {
                if snapshot.revision < previous.revision {
                    return Err(SurfaceStoreError::Conflict(format!(
                        "snapshot revision {} is older than {}",
                        snapshot.revision, previous.revision
                    )));
                }
                if snapshot.revision == previous.revision {
                    if &snapshot == previous {
                        return Ok(instance.clone());
                    }
                    return Err(SurfaceStoreError::Conflict(format!(
                        "snapshot revision {} already contains different state",
                        snapshot.revision
                    )));
                }
            }
            attachment.snapshot = Some(snapshot.clone());
            if attachment.lifecycle == SurfaceLifecycleState::Disposed {
                return Err(SurfaceStoreError::Conflict(
                    "a disposed Surface attachment cannot be remounted".to_owned(),
                ));
            }
            if attachment.lifecycle == SurfaceLifecycleState::Created {
                attachment.lifecycle = SurfaceLifecycleState::Mounted;
                attachment.lifecycle_revision = attachment.lifecycle_revision.saturating_add(1);
            }
            instance.descriptor.surface_revision =
                instance.descriptor.surface_revision.max(snapshot.revision);
            instance.updated_at_ms = unix_time_millis();
            Ok(instance.clone())
        })
    }

    pub(crate) fn apply_patch(
        &mut self,
        instance_id: &str,
        patch: SurfacePatch,
    ) -> Result<SurfaceInstanceRecord, SurfaceStoreError> {
        validate_surface_patch(&patch)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            if patch.instance_id != instance_id {
                return Err(SurfaceStoreError::Invalid(
                    "patch instance id does not match route".to_owned(),
                ));
            }
            let attachment = attachment_mut(instance, &patch.attachment_id)?;
            let snapshot = attachment.snapshot.as_mut().ok_or_else(|| {
                SurfaceStoreError::Conflict("an initial snapshot is required before patches".into())
            })?;
            if patch.base_revision != snapshot.revision {
                return Err(SurfaceStoreError::Conflict(format!(
                    "patch base revision {} does not match current revision {}",
                    patch.base_revision, snapshot.revision
                )));
            }

            let mut next = snapshot.clone();
            for operation in &patch.operations {
                apply_operation(&mut next.scene, operation)?;
            }
            merge_json(&mut next.authoritative_state, &patch.state_patch);
            merge_resources(&mut next.resources, &patch.resources);
            merge_resource_leases(&mut next.resource_leases, &patch.resource_leases);
            next.revision = patch.revision;
            loom_protocol::validate_surface_node_tree(&next.scene)
                .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
            *snapshot = next;
            merge_json(&mut instance.authoritative_state, &patch.state_patch);
            instance.descriptor.surface_revision =
                instance.descriptor.surface_revision.max(patch.revision);
            instance.updated_at_ms = unix_time_millis();
            Ok(instance.clone())
        })
    }

    pub(crate) fn begin_generation(
        &mut self,
        instance_id: &str,
        expected_generation: Option<u64>,
    ) -> Result<SurfaceInstanceDescriptor, SurfaceStoreError> {
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            if let Some(expected) = expected_generation {
                if expected != instance.descriptor.generation {
                    return Err(SurfaceStoreError::Conflict(format!(
                        "expected generation {expected}, current generation is {}",
                        instance.descriptor.generation
                    )));
                }
            }
            instance.descriptor.generation = instance.descriptor.generation.saturating_add(1);
            instance.last_failure = None;
            instance.updated_at_ms = unix_time_millis();
            Ok(instance.descriptor.clone())
        })
    }
}
