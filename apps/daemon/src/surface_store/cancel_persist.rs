// Action cancellation, acknowledgement updates, rollback transactions, and durable replacement.
impl SurfaceInstanceStore {
    pub(crate) fn request_cancel(
        &mut self,
        request: SurfaceActionCancelRequest,
    ) -> Result<(SurfaceEvent, SurfaceActionAck), SurfaceStoreError> {
        validate_surface_action_cancel_request(&request)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        self.transaction(|instances| {
            let instance = instance_mut(instances, &request.instance_id)?;
            let ack = instance
                .event_acks
                .values()
                .find(|ack| ack.request_id == request.request_id)
                .cloned()
                .ok_or_else(|| SurfaceStoreError::NotFound(request.request_id.clone()))?;
            if !matches!(
                ack.status,
                SurfaceActionStatus::Queued
                    | SurfaceActionStatus::Running
                    | SurfaceActionStatus::CancelRequested
            ) {
                return Err(SurfaceStoreError::Conflict(format!(
                    "Surface action cannot be cancelled while {:?}",
                    ack.status
                )));
            }
            let event = instance
                .pending_events
                .iter()
                .find(|event| event.event_id == ack.event_id)
                .cloned()
                .ok_or_else(|| {
                    SurfaceStoreError::Conflict(
                        "Surface action is no longer pending or running".to_owned(),
                    )
                })?;
            let attachment = instance
                .attachments
                .get(&event.attachment_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(event.attachment_id.clone()))?;
            if attachment.descriptor.device_id != request.device_id {
                return Err(SurfaceStoreError::Invalid(
                    "Surface cancel device does not own the action attachment".to_owned(),
                ));
            }
            let mut cancel_ack = ack;
            cancel_ack.status = SurfaceActionStatus::CancelRequested;
            cancel_ack.error = None;
            instance
                .event_acks
                .insert(cancel_ack.event_id.clone(), cancel_ack.clone());
            instance.updated_at_ms = unix_time_millis();
            Ok((event, cancel_ack))
        })
    }

    pub(crate) fn update_event_ack(
        &mut self,
        mut ack: SurfaceActionAck,
        remove_pending: bool,
    ) -> Result<SurfaceActionAck, SurfaceStoreError> {
        validate_surface_protocol(&ack.protocol_version)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        validate_identity(&ack.instance_id, "instance id")?;
        validate_identity(&ack.event_id, "event id")?;
        validate_identity(&ack.request_id, "request id")?;
        self.transaction(|instances| {
            let instance = instance_mut(instances, &ack.instance_id)?;
            if let Some(previous) = instance.event_acks.get(&ack.event_id) {
                if previous.request_id != ack.request_id {
                    return Err(SurfaceStoreError::Conflict(
                        "Surface event request identity changed".to_owned(),
                    ));
                }
                ack.accepted = previous.accepted;
            }
            instance
                .event_acks
                .insert(ack.event_id.clone(), ack.clone());
            if remove_pending {
                instance
                    .pending_events
                    .retain(|event| event.event_id != ack.event_id);
            }
            instance.updated_at_ms = unix_time_millis();
            Ok(ack)
        })
    }

    fn transaction<T>(
        &mut self,
        change: impl FnOnce(
            &mut BTreeMap<String, SurfaceInstanceRecord>,
        ) -> Result<T, SurfaceStoreError>,
    ) -> Result<T, SurfaceStoreError> {
        let previous = self.instances.clone();
        let output = match change(&mut self.instances) {
            Ok(output) => output,
            Err(error) => {
                self.instances = previous;
                return Err(error);
            }
        };
        if let Err(error) = self.persist() {
            self.instances = previous;
            return Err(error);
        }
        Ok(output)
    }

    fn persist(&mut self) -> Result<(), SurfaceStoreError> {
        let parent = self.path.parent().ok_or_else(|| {
            SurfaceStoreError::Invalid("Surface store path has no parent".to_owned())
        })?;
        let bytes = document_bytes(&self.instances)?;
        // Serializing is cheap next to `create_dir_all` plus a temporary file plus an `fsync` plus an
        // atomic replace, so the comparison happens first and the filesystem is only touched when the
        // projection actually moved. Nothing is deferred: whatever does change is durable before the
        // transaction that changed it returns, which `recover_pending` depends on.
        if self.persisted.as_deref() == Some(bytes.as_slice()) {
            return Ok(());
        }
        fs::create_dir_all(parent)?;
        let (temporary, mut file) = create_sensitive_temporary(&self.path)?;
        let result = (|| -> Result<(), SurfaceStoreError> {
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            replace_sensitive_file(&temporary, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
            return result;
        }
        self.persisted = Some(bytes);
        result
    }
}
