// Shares the existing single-controller authority and reliable source event stream.
impl WallController {
    fn matches(&self, actor: &str, reference: &WallControlReference) -> bool {
        self.device_id == actor
            && self.control_id == reference.control_id
            && self.binding.endpoint_id == reference.binding.endpoint_id
            && self.binding.lease_id == reference.binding.lease_id
            && self.binding.revision == reference.binding.revision
    }

    fn device_valid(&self, devices: &SharedDeviceRegistryStore) -> bool {
        devices.lock().ok().is_some_and(|store| {
            store.sessions.get(&self.token_hash).is_some_and(|session| {
                session.device_id == self.device_id
                    && session.expires_at_ms > unix_time_millis()
                    && store.devices.get(&self.device_id).is_some_and(|device| {
                        device.enabled
                            && device.approval == "approved"
                            && device.session_epoch == session.session_epoch
                    })
            })
        })
    }
}

fn clear_wall_controller(record: &mut LiveSessionRecord, reason: &str) {
    if record.wall_controller.take().is_some() {
        record.session.controller_device = None;
        record.controller_expires_at_ms = None;
        record.session.revision = record.session.revision.saturating_add(1);
        // This ordered authority edge makes the source release all held keys/buttons.
        push_state_event(record, reason);
    }
}

impl LiveSessionStore {
    fn acquire_wall_controller(
        &self,
        actor: &str,
        token_hash: &str,
        input: &WallControlAcquire,
        target: WallInputTarget,
    ) -> std::result::Result<(u16, String), WallStoreError> {
        let mut sessions = self.lock_state().map_err(wall_live_error)?;
        for record in sessions.values_mut() {
            expire_controller(record);
        }
        if sessions.values().any(|record| {
            record
                .wall_controller
                .as_ref()
                .is_some_and(|owner| owner.binding.endpoint_id == input.binding.endpoint_id)
        }) {
            return Err(WallStoreError::new(
                409,
                "wall_control_conflict",
                "endpoint already controls content; release it first",
            ));
        }
        let record =
            active_record_mut(&mut sessions, &target.session_id).map_err(wall_live_error)?;
        if record.session.controller_device.is_some() {
            return Err(WallStoreError::new(
                409,
                "wall_control_conflict",
                "another controller owns this source",
            ));
        }
        ensure_wall_source_ready(record)?;
        ensure_input_capability(record, &loom_protocol::LiveInputKind::Cancel)
            .map_err(wall_live_error)?;
        let control_id = Uuid::new_v4().to_string();
        let response = wall_json(
            &json!({"protocolVersion": "loom.wall.v1", "controlId": control_id,
            "endpointId": input.binding.endpoint_id, "revision": input.binding.revision,
            "placementId": target.placement_id, "sessionId": target.session_id, "leaseTtlMs": WALL_CONTROL_TTL_MS}),
        )?;
        record.wall_controller = Some(WallController {
            binding: input.binding.clone(),
            control_id,
            device_id: actor.to_owned(),
            token_hash: token_hash.to_owned(),
            placement_id: target.placement_id,
            session_id: target.session_id,
            pointer_id: input.pointer_id,
            sequence: 0,
            deadline: Instant::now() + Duration::from_millis(WALL_CONTROL_TTL_MS),
            buttons: 0,
            keys: BTreeSet::new(),
        });
        record.session.controller_device = Some(actor.to_owned());
        record.controller_expires_at_ms =
            Some(unix_time_millis().saturating_add(WALL_CONTROL_TTL_MS));
        record.session.revision = record.session.revision.saturating_add(1);
        push_state_event(record, "wall_controller_acquired");
        self.changed.notify_all();
        Ok(response)
    }

    fn wall_controller(
        &self,
        actor: &str,
        reference: &WallControlReference,
    ) -> std::result::Result<WallController, WallStoreError> {
        let mut sessions = self.lock_state().map_err(wall_live_error)?;
        for record in sessions.values_mut() {
            expire_controller(record);
        }
        sessions
            .values()
            .filter_map(|record| record.wall_controller.as_ref())
            .find(|owner| owner.matches(actor, reference))
            .cloned()
            .ok_or_else(wall_control_invalid)
    }

    fn release_wall_controller(&self, actor: &str, reference: &WallControlReference) {
        if let Ok(mut sessions) = self.state.lock() {
            for record in sessions.values_mut() {
                if record
                    .wall_controller
                    .as_ref()
                    .is_some_and(|owner| owner.matches(actor, reference))
                {
                    clear_wall_controller(record, "wall_controller_released");
                }
            }
            self.changed.notify_all();
        }
    }

    fn renew_wall_controller(
        &self,
        actor: &str,
        reference: &WallControlReference,
        target: WallInputTarget,
    ) -> std::result::Result<(u16, String), WallStoreError> {
        let mut sessions = self.lock_state().map_err(wall_live_error)?;
        let record =
            active_record_mut(&mut sessions, &target.session_id).map_err(wall_live_error)?;
        ensure_wall_source_ready(record)?;
        let owner = record
            .wall_controller
            .as_mut()
            .filter(|owner| owner.matches(actor, reference))
            .ok_or_else(wall_control_invalid)?;
        owner.deadline = Instant::now() + Duration::from_millis(WALL_CONTROL_TTL_MS);
        record.controller_expires_at_ms =
            Some(unix_time_millis().saturating_add(WALL_CONTROL_TTL_MS));
        wall_accepted()
    }

    fn forward_wall_input(
        &self,
        actor: &str,
        input: &WallInputRequest,
        target: WallInputTarget,
    ) -> std::result::Result<(u16, String), WallStoreError> {
        let mut sessions = self.lock_state().map_err(wall_live_error)?;
        let record =
            active_record_mut(&mut sessions, &target.session_id).map_err(wall_live_error)?;
        ensure_wall_source_ready(record)?;
        let mut owner = record
            .wall_controller
            .as_ref()
            .filter(|owner| owner.matches(actor, &input.control))
            .cloned()
            .ok_or_else(wall_control_invalid)?;
        if input.sequence != owner.sequence + 1
            || input.sequence > loom_protocol::wall::WALL_MAX_REVISION
        {
            return Err(WallStoreError::new(
                409,
                "wall_input_sequence_invalid",
                "input sequence must be exactly next",
            ));
        }
        let kind = input.event.translate(&target, &mut owner)?;
        ensure_input_capability(record, &kind).map_err(wall_live_error)?;
        let envelope = LiveControlEnvelope {
            protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.into(),
            session_id: target.session_id,
            epoch: record.epoch,
            sequence: record.next_event_sequence,
            message: LiveControlMessage::InputEvent(loom_protocol::LiveInputEvent {
                input_sequence: input.sequence,
                issued_at_ms: unix_time_millis(),
                source_device_id: actor.to_owned(),
                kind,
            }),
        };
        loom_protocol::validate_control_envelope(&envelope).map_err(|_| {
            WallStoreError::new(
                400,
                "wall_input_invalid",
                "translated input violates Live contract",
            )
        })?;
        owner.sequence = input.sequence;
        record.wall_controller = Some(owner);
        push_live_event(record, envelope);
        self.changed.notify_all();
        wall_accepted()
    }

    fn prune_wall_controllers(&self, walls: &SharedWallStore, devices: &SharedDeviceRegistryStore) {
        let owners: Vec<_> = self
            .state
            .lock()
            .map(|sessions| {
                sessions
                    .values()
                    .filter_map(|record| record.wall_controller.clone())
                    .collect()
            })
            .unwrap_or_default();
        // Never take a wall/registry lock while holding the Live lock. Input takes wall -> Live.
        for owner in owners {
            let valid = owner.deadline > Instant::now()
                && owner.device_valid(devices)
                && self.wall_source_active(&owner.session_id)
                && walls
                    .with_input_target(
                        &owner.device_id,
                        &owner.binding,
                        Some(&owner.placement_id),
                        None,
                        TileInputCapability::Pointer,
                        |_| Ok(()),
                    )
                    .is_ok();
            if !valid {
                self.release_wall_controller(
                    &owner.device_id,
                    &WallControlReference {
                        binding: owner.binding,
                        control_id: owner.control_id,
                    },
                );
            }
        }
    }
}

fn ensure_wall_source_ready(record: &LiveSessionRecord) -> std::result::Result<(), WallStoreError> {
    if !record.source_connected
        || record
            .frames
            .back()
            .is_none_or(|frame| frame.received_at.elapsed() >= Duration::from_secs(5))
    {
        Err(WallStoreError::new(
            409,
            "wall_live_source_unavailable",
            "input source is disconnected or stale",
        ))
    } else {
        Ok(())
    }
}
