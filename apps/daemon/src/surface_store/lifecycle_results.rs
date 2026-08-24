// Attachment lifecycle transitions and generation-scoped preview, result, and failure commits.
impl SurfaceInstanceStore {
    pub(crate) fn transition_lifecycle(
        &mut self,
        instance_id: &str,
        event: SurfaceLifecycleEvent,
    ) -> Result<SurfaceAttachmentRecord, SurfaceStoreError> {
        validate_surface_protocol(&event.protocol_version)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        validate_identity(&event.instance_id, "instance id")?;
        validate_identity(&event.attachment_id, "attachment id")?;
        if event.instance_id != instance_id {
            return Err(SurfaceStoreError::Invalid(
                "lifecycle instance id does not match route".to_owned(),
            ));
        }
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            let attachment = attachment_mut(instance, &event.attachment_id)?;
            if event.revision < attachment.lifecycle_revision {
                return Err(SurfaceStoreError::Conflict(format!(
                    "lifecycle revision {} is older than {}",
                    event.revision, attachment.lifecycle_revision
                )));
            }
            if event.revision == attachment.lifecycle_revision {
                if event.state == attachment.lifecycle {
                    return Ok(attachment.clone());
                }
                return Err(SurfaceStoreError::Conflict(
                    "lifecycle revision already contains a different state".to_owned(),
                ));
            }
            if event.revision != attachment.lifecycle_revision.saturating_add(1) {
                return Err(SurfaceStoreError::Conflict(format!(
                    "lifecycle revision {} must advance {} by exactly one",
                    event.revision, attachment.lifecycle_revision
                )));
            }
            if !lifecycle_transition_allowed(&attachment.lifecycle, &event.state) {
                return Err(SurfaceStoreError::Conflict(format!(
                    "invalid Surface lifecycle transition {:?} -> {:?}",
                    attachment.lifecycle, event.state
                )));
            }
            attachment.lifecycle = event.state;
            attachment.lifecycle_revision = event.revision;
            if attachment.lifecycle == SurfaceLifecycleState::Disposed {
                attachment.snapshot = None;
                attachment.host_capabilities = None;
            }
            let result = attachment.clone();
            if result.lifecycle == SurfaceLifecycleState::Disposed {
                let confirmation_ids = instance
                    .pending_confirmations
                    .iter()
                    .filter(|(_, pending)| {
                        pending.request.attachment_id == result.descriptor.attachment_id
                    })
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>();
                for confirmation_id in confirmation_ids {
                    let Some(pending) = instance.pending_confirmations.remove(&confirmation_id)
                    else {
                        continue;
                    };
                    if let Some(ack) = instance.event_acks.get_mut(&pending.event.event_id) {
                        ack.status = SurfaceActionStatus::Cancelled;
                        ack.error = Some(SurfaceExecutionError {
                            code: "surface_attachment_disposed".to_owned(),
                            message: "Surface attachment was disposed before confirmation"
                                .to_owned(),
                            detail: None,
                        });
                    }
                }
            }
            instance.updated_at_ms = unix_time_millis();
            Ok(result)
        })
    }

    pub(crate) fn commit_preview(
        &mut self,
        instance_id: &str,
        commit: SurfacePreviewCommit,
    ) -> Result<SurfaceInstanceRecord, SurfaceStoreError> {
        validate_surface_protocol(&commit.protocol_version)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        validate_identity(&commit.request_id, "request id")?;
        validate_identity(&commit.port_id, "preview port id")?;
        validate_port_value(&commit.value)?;
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            validate_commit_identity(
                instance,
                instance_id,
                &commit.instance_id,
                commit.generation,
            )?;
            if commit.preview_revision <= instance.descriptor.preview_revision {
                return Err(SurfaceStoreError::Conflict(format!(
                    "preview revision {} does not advance {}",
                    commit.preview_revision, instance.descriptor.preview_revision
                )));
            }
            instance.descriptor.preview_revision = commit.preview_revision;
            instance.latest_preview = Some(commit);
            instance.updated_at_ms = unix_time_millis();
            Ok(instance.clone())
        })
    }

    pub(crate) fn commit_result(
        &mut self,
        instance_id: &str,
        commit: SurfaceResultCommit,
    ) -> Result<SurfaceInstanceRecord, SurfaceStoreError> {
        validate_surface_protocol(&commit.protocol_version)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        validate_identity(&commit.request_id, "request id")?;
        if commit.outputs.is_empty() {
            return Err(SurfaceStoreError::Invalid(
                "formal result must contain at least one output".to_owned(),
            ));
        }
        for (port_id, value) in &commit.outputs {
            validate_identity(port_id, "formal output port id")?;
            validate_port_value(value)?;
        }
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            validate_commit_identity(
                instance,
                instance_id,
                &commit.instance_id,
                commit.generation,
            )?;
            if commit.result_revision <= instance.descriptor.result_revision {
                return Err(SurfaceStoreError::Conflict(format!(
                    "result revision {} does not advance {}",
                    commit.result_revision, instance.descriptor.result_revision
                )));
            }
            let mut next_state = instance.authoritative_state.clone();
            merge_json(&mut next_state, &commit.state_patch);
            instance.authoritative_state = next_state;
            instance.descriptor.result_revision = commit.result_revision;
            instance.latest_result = Some(commit);
            instance.last_failure = None;
            instance.updated_at_ms = unix_time_millis();
            Ok(instance.clone())
        })
    }

    pub(crate) fn record_failure(
        &mut self,
        instance_id: &str,
        mut failure: SurfaceExecutionFailure,
    ) -> Result<SurfaceInstanceRecord, SurfaceStoreError> {
        validate_surface_protocol(&failure.protocol_version)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        validate_identity(&failure.request_id, "request id")?;
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            validate_commit_identity(
                instance,
                instance_id,
                &failure.instance_id,
                failure.generation,
            )?;
            failure.last_successful_result_revision = instance
                .latest_result
                .as_ref()
                .map(|result| result.result_revision);
            instance.last_failure = Some(failure);
            instance.updated_at_ms = unix_time_millis();
            Ok(instance.clone())
        })
    }
}
