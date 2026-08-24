// Remote event admission, acknowledgement state, and confirmation lifecycle handling.
impl SurfaceInstanceStore {
    pub(crate) fn accept_event(
        &mut self,
        instance_id: &str,
        event: SurfaceEvent,
    ) -> Result<SurfaceActionAck, SurfaceStoreError> {
        validate_surface_event(&event)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        if event.class == SurfaceEventClass::Local {
            return Err(SurfaceStoreError::Invalid(
                "local Surface events must not cross the Loom boundary".to_owned(),
            ));
        }
        let action = event.action.as_deref().ok_or_else(|| {
            SurfaceStoreError::Invalid("remote Surface event has no declared action".to_owned())
        })?;
        if let Some(existing) = self
            .instances
            .get(instance_id)
            .and_then(|instance| instance.event_acks.get(&event.event_id))
        {
            return Ok(existing.clone());
        }
        let ack = {
            let instance = self
                .instances
                .get(instance_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(instance_id.to_owned()))?;
            validate_surface_event_context(instance, instance_id, &event, action)?;
            SurfaceActionAck {
                protocol_version: loom_protocol::SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.to_owned(),
                event_id: event.event_id.clone(),
                request_id: surface_request_id(&event.event_id),
                accepted: true,
                status: SurfaceActionStatus::Queued,
                error: None,
            }
        };

        if event.class == SurfaceEventClass::Continuous {
            // Continuous events are transient coalescing signals. No consumer
            // reads them from the store, so retaining every distinct key would
            // only keep payloads alive for the lifetime of the daemon.
            return Ok(ack);
        }

        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            if instance.pending_events.len() >= MAX_PENDING_SURFACE_EVENTS {
                return Err(SurfaceStoreError::Conflict(
                    "Surface action queue is full".to_owned(),
                ));
            }
            instance.pending_events.push(event.clone());
            instance
                .event_acks
                .insert(event.event_id.clone(), ack.clone());
            instance.updated_at_ms = unix_time_millis();
            Ok(ack)
        })
    }

    pub(crate) fn await_confirmation(
        &mut self,
        instance_id: &str,
        event: SurfaceEvent,
        risk: SurfaceActionRisk,
    ) -> Result<(SurfaceActionAck, SurfaceConfirmationRequest), SurfaceStoreError> {
        validate_surface_event(&event)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        if event.class == SurfaceEventClass::Local {
            return Err(SurfaceStoreError::Invalid(
                "local Surface events must not cross the Loom boundary".to_owned(),
            ));
        }
        if event.class == SurfaceEventClass::Continuous {
            return Err(SurfaceStoreError::Invalid(
                "continuous Surface events cannot require confirmation".to_owned(),
            ));
        }
        let action = event.action.as_deref().ok_or_else(|| {
            SurfaceStoreError::Invalid("remote Surface event has no declared action".to_owned())
        })?;
        if let Some(instance) = self.instances.get(instance_id) {
            if let Some(existing) = instance.event_acks.get(&event.event_id) {
                let pending = instance
                    .pending_confirmations
                    .values()
                    .find(|pending| pending.event.event_id == event.event_id)
                    .ok_or_else(|| {
                        SurfaceStoreError::Conflict(
                            "Surface event already has a non-confirmation action state".to_owned(),
                        )
                    })?;
                return Ok((existing.clone(), pending.request.clone()));
            }
        }
        let (attachment, request_id) = {
            let instance = self
                .instances
                .get(instance_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(instance_id.to_owned()))?;
            validate_surface_event_context(instance, instance_id, &event, action)?;
            let attachment = instance
                .attachments
                .get(&event.attachment_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(event.attachment_id.clone()))?
                .descriptor
                .clone();
            (attachment, surface_request_id(&event.event_id))
        };
        let request = SurfaceConfirmationRequest {
            protocol_version: loom_protocol::SURFACE_PROTOCOL_VERSION.to_owned(),
            confirmation_id: format!("confirmation:{}", Uuid::new_v4()),
            instance_id: instance_id.to_owned(),
            attachment_id: event.attachment_id.clone(),
            device_id: attachment.device_id,
            hook_node_id: attachment.hook_node_id,
            event_id: event.event_id.clone(),
            request_id: request_id.clone(),
            action_id: action.to_owned(),
            risk,
            expires_at_ms: unix_time_millis().saturating_add(SURFACE_CONFIRMATION_TTL_MILLIS),
            payload: event.payload.clone(),
        };
        validate_surface_confirmation_request(&request)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        let ack = SurfaceActionAck {
            protocol_version: loom_protocol::SURFACE_PROTOCOL_VERSION.to_owned(),
            instance_id: instance_id.to_owned(),
            event_id: event.event_id.clone(),
            request_id,
            accepted: true,
            status: SurfaceActionStatus::AwaitingConfirmation,
            error: None,
        };
        self.transaction(|instances| {
            let instance = instance_mut(instances, instance_id)?;
            if instance.pending_confirmations.len() >= MAX_PENDING_SURFACE_CONFIRMATIONS {
                return Err(SurfaceStoreError::Conflict(
                    "Surface confirmation queue is full".to_owned(),
                ));
            }
            instance
                .event_acks
                .insert(event.event_id.clone(), ack.clone());
            instance.pending_confirmations.insert(
                request.confirmation_id.clone(),
                SurfacePendingConfirmation {
                    request: request.clone(),
                    event,
                },
            );
            instance.updated_at_ms = unix_time_millis();
            Ok((ack, request))
        })
    }

    pub(crate) fn resolve_confirmation(
        &mut self,
        decision: SurfaceConfirmationDecision,
    ) -> Result<SurfaceConfirmationResolution, SurfaceStoreError> {
        validate_surface_confirmation_decision(&decision)
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?;
        self.transaction(|instances| {
            let instance = instance_mut(instances, &decision.instance_id)?;
            let pending = instance
                .pending_confirmations
                .get(&decision.confirmation_id)
                .cloned()
                .ok_or_else(|| SurfaceStoreError::NotFound(decision.confirmation_id.clone()))?;
            if pending.request.instance_id != decision.instance_id
                || pending.request.attachment_id != decision.attachment_id
                || pending.request.device_id != decision.device_id
            {
                return Err(SurfaceStoreError::Invalid(
                    "Surface confirmation decision identity does not match the request".to_owned(),
                ));
            }
            let mut ack = instance
                .event_acks
                .get(&pending.event.event_id)
                .cloned()
                .ok_or_else(|| {
                    SurfaceStoreError::Conflict(
                        "Surface confirmation action acknowledgement is missing".to_owned(),
                    )
                })?;
            instance
                .pending_confirmations
                .remove(&decision.confirmation_id);
            let resolution = if pending.request.expires_at_ms <= unix_time_millis() {
                ack.status = SurfaceActionStatus::Failed;
                ack.error = Some(SurfaceExecutionError {
                    code: "surface_confirmation_expired".to_owned(),
                    message: "Surface action confirmation expired".to_owned(),
                    detail: None,
                });
                SurfaceConfirmationResolution::Expired { ack: ack.clone() }
            } else if !decision.approved {
                ack.status = SurfaceActionStatus::Cancelled;
                ack.error = Some(SurfaceExecutionError {
                    code: "surface_confirmation_rejected".to_owned(),
                    message: "Surface action was rejected by the user".to_owned(),
                    detail: None,
                });
                SurfaceConfirmationResolution::Rejected { ack: ack.clone() }
            } else {
                if instance.pending_events.len() >= MAX_PENDING_SURFACE_EVENTS {
                    return Err(SurfaceStoreError::Conflict(
                        "Surface action queue is full".to_owned(),
                    ));
                }
                ack.status = SurfaceActionStatus::Queued;
                ack.error = None;
                instance.pending_events.push(pending.event.clone());
                SurfaceConfirmationResolution::Approved {
                    event: pending.event,
                    ack: ack.clone(),
                }
            };
            instance.event_acks.insert(ack.event_id.clone(), ack);
            instance.updated_at_ms = unix_time_millis();
            Ok(resolution)
        })
    }

    pub(crate) fn expire_confirmations(
        &mut self,
    ) -> Result<Vec<SurfaceActionAck>, SurfaceStoreError> {
        let now = unix_time_millis();
        // `recover_pending` ticks this on a timer, and almost every tick has nothing to do. The scan
        // is read-only, so the common case no longer clones the whole instance map inside a
        // transaction just to discover that it changed nothing.
        let any_expired = self.instances.values().any(|instance| {
            instance
                .pending_confirmations
                .values()
                .any(|pending| pending.request.expires_at_ms <= now)
        });
        if !any_expired {
            return Ok(Vec::new());
        }
        self.transaction(|instances| {
            let mut expired = Vec::new();
            for instance in instances.values_mut() {
                let ids = instance
                    .pending_confirmations
                    .iter()
                    .filter(|(_, pending)| pending.request.expires_at_ms <= now)
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>();
                for id in ids {
                    let Some(pending) = instance.pending_confirmations.remove(&id) else {
                        continue;
                    };
                    let Some(mut ack) = instance.event_acks.get(&pending.event.event_id).cloned()
                    else {
                        continue;
                    };
                    ack.status = SurfaceActionStatus::Failed;
                    ack.error = Some(SurfaceExecutionError {
                        code: "surface_confirmation_expired".to_owned(),
                        message: "Surface action confirmation expired".to_owned(),
                        detail: None,
                    });
                    instance
                        .event_acks
                        .insert(ack.event_id.clone(), ack.clone());
                    instance.updated_at_ms = now;
                    expired.push(ack);
                }
            }
            Ok(expired)
        })
    }
}
