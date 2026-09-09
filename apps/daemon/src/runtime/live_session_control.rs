// Single-controller lease mutations kept separate from media and session lifecycle ownership.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LiveControlLeaseRequest {
    protocol_version: String,
    surface_instance_id: String,
    attachment_id: String,
    action: LiveControlLeaseAction,
    sequence: u64,
    epoch: u64,
    #[serde(default)]
    lease_duration_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LiveControlLeaseAction {
    Acquire,
    Release,
    Revoke,
}

impl LiveSessionStore {
    fn change_controller(
        &self,
        actor_device_id: &str,
        session_id: &str,
        request: &LiveControlLeaseRequest,
    ) -> std::result::Result<LiveSessionRuntimeSnapshot, LiveRuntimeError> {
        if request.protocol_version != loom_protocol::LIVE_PROTOCOL_VERSION {
            return Err(LiveRuntimeError::new(
                400,
                "live_protocol_unsupported",
                "the live control lease protocol version is unsupported",
            ));
        }
        let mut sessions = self.lock_state()?;
        let record = active_record_mut(&mut sessions, session_id)?;
        match request.action {
            LiveControlLeaseAction::Acquire => {
                ensure_viewer(record, actor_device_id)?;
                let duration = request.lease_duration_ms.unwrap_or(30_000);
                if !(1_000..=300_000).contains(&duration) {
                    return Err(LiveRuntimeError::new(
                        400,
                        "live_control_lease_invalid",
                        "the controller lease must be between 1000 and 300000 milliseconds",
                    ));
                }
                if record
                    .session
                    .controller_device
                    .as_deref()
                    .is_some_and(|controller| controller != actor_device_id)
                {
                    push_state_event(
                        record,
                        &format!("controller_rejected_conflict:{actor_device_id}"),
                    );
                    return Err(LiveRuntimeError::new(
                        409,
                        "live_controller_conflict",
                        "another viewer currently owns the controller lease",
                    ));
                }
                accept_control_sequence(record, actor_device_id, request.epoch, request.sequence)?;
                record.session.controller_device = Some(actor_device_id.to_owned());
                record.controller_expires_at_ms = Some(unix_time_millis().saturating_add(duration));
                record.session.revision = record.session.revision.saturating_add(1);
                push_state_event(record, &format!("controller_acquired:{actor_device_id}"));
            }
            LiveControlLeaseAction::Release => {
                ensure_viewer(record, actor_device_id)?;
                if record.session.controller_device.as_deref() != Some(actor_device_id) {
                    push_state_event(
                        record,
                        &format!("controller_rejected_release:{actor_device_id}"),
                    );
                    return Err(LiveRuntimeError::new(
                        403,
                        "live_controller_release_denied",
                        "only the active controller can release its lease",
                    ));
                }
                accept_control_sequence(record, actor_device_id, request.epoch, request.sequence)?;
                record.session.controller_device = None;
                record.controller_expires_at_ms = None;
                record.session.revision = record.session.revision.saturating_add(1);
                push_state_event(record, &format!("controller_released:{actor_device_id}"));
            }
            LiveControlLeaseAction::Revoke => {
                if record.session.source_device_id != actor_device_id {
                    return Err(LiveRuntimeError::new(
                        403,
                        "live_controller_revoke_denied",
                        "only the source device can revoke remote control",
                    ));
                }
                accept_control_sequence(record, actor_device_id, request.epoch, request.sequence)?;
                record.session.controller_device = None;
                record.controller_expires_at_ms = None;
                record.session.revision = record.session.revision.saturating_add(1);
                push_state_event(
                    record,
                    &format!("controller_revoked_by_source:{actor_device_id}"),
                );
            }
        }
        self.changed.notify_all();
        Ok(snapshot(record))
    }
}
