use super::*;
impl OfflinePeers {
    pub(crate) fn local_transfer(
        &self,
        request: &ParsedHttpRequest,
        registry: &SharedDeviceRegistryStore,
    ) -> PeerResult<Value> {
        if request.method != "POST" {
            return Err(failure(405, "peer_method_not_allowed"));
        }
        let actor = authenticate_http_device_session(request, registry)
            .map_err(|_| failure(401, "projection_pairing_required"))?
            .ok_or_else(|| failure(403, "projection_device_required"))?;
        if self
            .transfer_work
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(failure(429, "projection_busy"));
        }
        let _guard = ProbeGuard(&self.transfer_work);
        let operation = request
            .path
            .split('?')
            .next()
            .unwrap_or_default()
            .rsplit('/')
            .next()
            .unwrap_or_default();
        if operation == "create" {
            return self.create_transfer(&actor, parse_transfer(&request.body)?, registry);
        }
        let body: Value = parse_transfer(&request.body)?;
        if operation == "inbox" {
            let state = self
                .state
                .lock()
                .map_err(|_| failure(503, "peer_unavailable"))?;
            let mut registry = registry
                .try_lock()
                .map_err(|_| failure(503, "projection_busy"))?;
            let actor = device(&registry, &actor)?;
            let policy: ProjectionInbox = actions::from(body)?;
            handle_projection_delivery(
                "/v1/projections/inbox",
                &request.body,
                &actor,
                &mut registry,
                unix_time_millis(),
            )
            .map_err(convert)?;
            if policy.policy == ProjectionReceivePolicy::Disabled {
                return Ok(json!({"invitations": []}));
            }
            let store = self
                .transfers
                .lock()
                .map_err(|_| failure(503, "projection_busy"))?;
            let invitations: Vec<_> = store
                .records
                .values()
                .filter(|r| {
                    r.incoming
                        && r.target_id == actor.id
                        && r.active().is_ok()
                        && authorized(&state, &registry, r).is_ok()
                        && matches!(
                            r.status,
                            DeliveryStatus::AwaitingConfirmation | DeliveryStatus::Accepted
                        )
                })
                .filter_map(|r| {
                    let policy = crate::projection_receive_decision(
                        &registry,
                        &actor.id,
                        &r.envelope.source.device_id,
                        Some(&r.peer_id),
                    )
                    .ok()?;
                    if r.status == DeliveryStatus::AwaitingConfirmation
                        && policy == crate::projection_settings::Decision::Reject
                    {
                        return None;
                    }
                    let mut response = r.response(false);
                    response["receivePolicy"] = json!(if r.status == DeliveryStatus::Accepted {
                        crate::projection_settings::Decision::Auto
                    } else {
                        policy
                    });
                    Some(response)
                })
                .collect();
            return Ok(json!({"invitations": invitations}));
        }
        // Decode potentially expensive PNGs before acquiring trust/device/store locks.
        let update = if operation == "update" {
            let input: ProjectionUpdate = actions::from(body.clone())?;
            validate_projection_snapshot(&input.snapshot, &input.digest).map_err(convert)?;
            Some(input)
        } else {
            None
        };
        let id = body["projectionId"]
            .as_str()
            .or(body["envelope"]["projectionId"].as_str())
            .ok_or_else(|| failure(400, "projection_invalid_request"))?;
        let (record, local_response) = {
            let state = self
                .state
                .lock()
                .map_err(|_| failure(503, "peer_unavailable"))?;
            let mut registry = registry
                .try_lock()
                .map_err(|_| failure(503, "projection_busy"))?;
            let mut store = self
                .transfers
                .lock()
                .map_err(|_| failure(503, "projection_busy"))?;
            let mut record = store.get(id)?;
            authorized(&state, &registry, &record)?;
            let owner = if record.incoming {
                &record.target_id
            } else {
                &record.envelope.source.device_id
            };
            if owner != &actor {
                return Err(failure(403, "projection_access_denied"));
            }
            if record.incoming
                && record.status == DeliveryStatus::AwaitingConfirmation
                && matches!(operation, "accept" | "inspect")
            {
                crate::projection_receive_allowed(
                    &registry,
                    &actor,
                    &record.envelope.source.device_id,
                    Some(&record.peer_id),
                )
                .map_err(convert)?;
            }
            let result = if record.incoming {
                None
            } else {
                match operation {
                    "read" => {
                        record.active()?;
                        Some(edit::document(&registry, &record, record.response(false))?)
                    }
                    "edit" => {
                        record.active()?;
                        if matches!(body["operation"].as_str(), Some("initialize" | "attach")) {
                            edit::prune(&state, &mut registry, &store)?;
                        }
                        Some(
                            crate::projection_edit::handle_authorized(
                                &request.body,
                                edit::access(&record, true),
                                &mut registry.projection_edits,
                            )
                            .map_err(convert)?,
                        )
                    }
                    "update" => {
                        if registry.projection_edits.has_binding(id).map_err(convert)? {
                            return Err(failure(409, "projection_edit_snapshot_locked"));
                        }
                        actions::update(
                            &mut record,
                            update.ok_or_else(|| failure(400, "projection_invalid_request"))?,
                        )?;
                        store.commit(record.clone())?;
                        Some(record.response(false))
                    }
                    "unlink" => {
                        record.unlinked = true;
                        store.commit(record.clone())?;
                        registry.projection_edits.release(id).map_err(convert)?;
                        Some(json!({"unlinked": true}))
                    }
                    _ => return Err(failure(403, "projection_access_denied")),
                }
            };
            (record, result)
        };
        if let Some(response) = local_response {
            if operation == "unlink" && record.target_epoch.is_some() {
                // Durable source stop is authoritative; receiver reads also observe it after reconnect.
                let _ = self.exchange(
                    &record.peer_id,
                    record.trust_revision,
                    remote_call(&record, "stop", operation, body),
                );
            }
            return Ok(response);
        }
        if !matches!(
            operation,
            "read" | "inspect" | "accept" | "receipt" | "unlink" | "edit"
        ) {
            return Err(failure(403, "projection_access_denied"));
        }
        if operation == "edit" {
            return self.forward_edit(&actor, record, body, registry);
        }
        self.forward_receiver(&actor, record, operation, body, registry)
    }
}

pub(super) fn remote_call(record: &Record, action: &str, operation: &str, input: Value) -> Value {
    json!({"action": action, "body": {"projectionId": record.envelope.projection_id,
        "sourceEpoch": record.source_epoch, "targetId": record.target_id, "targetEpoch": record.target_epoch,
        "operation": operation, "input": input}})
}
