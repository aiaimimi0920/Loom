// In-memory loom.live.v1 session authority, bounded frame rings, control events, and media workers.
type SharedLiveSessionStore = Arc<LiveSessionStore>;

const LIVE_SESSION_LIMIT: usize = 32;
const LIVE_CONTROL_HISTORY_LIMIT: usize = 256;
const LIVE_OBSERVATION_LIMIT: usize = 256;
const LIVE_TRIGGER_AUDIT_LIMIT: usize = 256;
const LIVE_TRIGGER_IDEMPOTENCY_LIMIT: usize = 1_024;
const LIVE_MEDIA_CONNECTION_LIMIT: usize = 65;

#[derive(Debug)]
struct LiveRuntimeError {
    status: u16,
    code: &'static str,
    message: String,
}

impl LiveRuntimeError {
    fn new(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
}

#[derive(Clone)]
struct StoredLiveFrame {
    epoch: u64,
    frame_id: u64,
    bytes: Arc<Vec<u8>>,
}

struct LiveSessionRecord {
    session: LiveScreenshotSession,
    request_nonce: String,
    epoch: u64,
    last_frame_id: u64,
    published_frames: u64,
    relay_dropped_frames: u64,
    source_connected: bool,
    viewer_connections: BTreeMap<String, usize>,
    controller_expires_at_ms: Option<u64>,
    frames: VecDeque<StoredLiveFrame>,
    events: VecDeque<LiveControlEnvelope>,
    next_event_sequence: u64,
    inbound_sequences: BTreeMap<String, (u64, u64)>,
    inbound_input_sequences: BTreeMap<String, (u64, u64)>,
    inbound_observation_sequences: BTreeMap<String, (u64, u64)>,
    observations: BTreeMap<String, LiveObservation>,
    trigger_registrations: BTreeMap<String, LiveTriggerRegistration>,
    trigger_audits: VecDeque<LiveTriggerAudit>,
    trigger_idempotency_keys: BTreeSet<String>,
    trigger_idempotency_order: VecDeque<String>,
    pending_trigger_dispatches: BTreeSet<String>,
    closed: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveSessionRuntimeSnapshot {
    session: LiveScreenshotSession,
    epoch: u64,
    last_frame_id: u64,
    published_frames: u64,
    relay_dropped_frames: u64,
    buffered_frames: usize,
    source_connected: bool,
    viewer_connections: BTreeMap<String, usize>,
    controller_expires_at_ms: Option<u64>,
    observations: Vec<LiveObservation>,
    triggers: Vec<LiveTriggerRegistrationSnapshot>,
    trigger_audits: Vec<LiveTriggerAudit>,
    closed: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveRelayStatus {
    protocol_version: &'static str,
    active_sessions: usize,
    connected_sources: usize,
    connected_viewers: usize,
    controller_leases: usize,
    buffered_frames: usize,
    published_frames: u64,
    relay_dropped_frames: u64,
    media_connections: usize,
}

impl Default for LiveRelayStatus {
    fn default() -> Self {
        Self {
            protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION,
            active_sessions: 0,
            connected_sources: 0,
            connected_viewers: 0,
            controller_leases: 0,
            buffered_frames: 0,
            published_frames: 0,
            relay_dropped_frames: 0,
            media_connections: 0,
        }
    }
}

struct LiveSessionStore {
    state: Mutex<BTreeMap<String, LiveSessionRecord>>,
    changed: Condvar,
    media_cancelled: Arc<AtomicBool>,
    media_connections: Arc<AtomicUsize>,
    media_workers: Mutex<Vec<JoinHandle<()>>>,
}

impl LiveSessionStore {
    fn new() -> Self {
        Self {
            state: Mutex::new(BTreeMap::new()),
            changed: Condvar::new(),
            media_cancelled: Arc::new(AtomicBool::new(false)),
            media_connections: Arc::new(AtomicUsize::new(0)),
            media_workers: Mutex::new(Vec::new()),
        }
    }

    fn create(
        &self,
        actor_device_id: &str,
        envelope: LiveControlEnvelope,
    ) -> std::result::Result<(bool, LiveSessionRuntimeSnapshot), LiveRuntimeError> {
        let LiveControlMessage::SessionStart(start) = envelope.message else {
            return Err(LiveRuntimeError::new(
                400,
                "live_message_invalid",
                "live session creation requires session_start",
            ));
        };
        if actor_device_id != start.requested_by_device_id
            || actor_device_id != start.session.source_device_id
        {
            return Err(LiveRuntimeError::new(
                403,
                "live_source_identity_mismatch",
                "the authenticated device must own the live source",
            ));
        }
        if !start.session.viewer_devices.is_empty()
            || start.session.controller_device.is_some()
            || !start.session.trigger_bindings.is_empty()
        {
            return Err(LiveRuntimeError::new(
                400,
                "live_initial_authority_invalid",
                "a new live session cannot pre-authorize viewers or a controller",
            ));
        }
        let mut sessions = self.lock_state()?;
        if let Some(existing) = sessions.get(&start.session.session_id) {
            if !existing.closed
                && existing.session.source_device_id == actor_device_id
                && existing.request_nonce == start.request_nonce
            {
                return Ok((false, snapshot(existing)));
            }
            return Err(LiveRuntimeError::new(
                409,
                "live_session_conflict",
                "the live session id already belongs to another request",
            ));
        }
        if sessions.values().filter(|record| !record.closed).count() >= LIVE_SESSION_LIMIT {
            return Err(LiveRuntimeError::new(
                429,
                "live_session_limit",
                "the live session limit has been reached",
            ));
        }
        let session_id = start.session.session_id.clone();
        let mut record = LiveSessionRecord {
            session: start.session,
            request_nonce: start.request_nonce,
            epoch: envelope.epoch,
            last_frame_id: 0,
            published_frames: 0,
            relay_dropped_frames: 0,
            source_connected: false,
            viewer_connections: BTreeMap::new(),
            controller_expires_at_ms: None,
            frames: VecDeque::with_capacity(3),
            events: VecDeque::with_capacity(LIVE_CONTROL_HISTORY_LIMIT),
            next_event_sequence: 1,
            inbound_sequences: BTreeMap::new(),
            inbound_input_sequences: BTreeMap::new(),
            inbound_observation_sequences: BTreeMap::new(),
            observations: BTreeMap::new(),
            trigger_registrations: BTreeMap::new(),
            trigger_audits: VecDeque::with_capacity(LIVE_TRIGGER_AUDIT_LIMIT),
            trigger_idempotency_keys: BTreeSet::new(),
            trigger_idempotency_order: VecDeque::with_capacity(LIVE_TRIGGER_IDEMPOTENCY_LIMIT),
            pending_trigger_dispatches: BTreeSet::new(),
            closed: false,
        };
        accept_control_sequence(
            &mut record,
            actor_device_id,
            envelope.epoch,
            envelope.sequence,
        )?;
        push_state_event(&mut record, "session_created");
        let result = snapshot(&record);
        sessions.insert(session_id, record);
        self.changed.notify_all();
        Ok((true, result))
    }

    fn attach_viewer(
        &self,
        actor_device_id: &str,
        envelope: LiveControlEnvelope,
    ) -> std::result::Result<LiveSessionRuntimeSnapshot, LiveRuntimeError> {
        let LiveControlMessage::SessionAck(ack) = &envelope.message else {
            return Err(LiveRuntimeError::new(
                400,
                "live_message_invalid",
                "viewer attachment requires session_ack",
            ));
        };
        if !ack.accepted || ack.responder_device_id != actor_device_id {
            return Err(LiveRuntimeError::new(
                403,
                "live_viewer_identity_mismatch",
                "the authenticated viewer must acknowledge itself",
            ));
        }
        let mut sessions = self.lock_state()?;
        let record = active_record_mut(&mut sessions, &envelope.session_id)?;
        if record.session.source_device_id == actor_device_id {
            return Err(LiveRuntimeError::new(
                409,
                "live_viewer_is_source",
                "the source device cannot join as a remote viewer",
            ));
        }
        accept_control_sequence(record, actor_device_id, envelope.epoch, envelope.sequence)?;
        if !record
            .session
            .viewer_devices
            .iter()
            .any(|viewer| viewer == actor_device_id)
        {
            if record.session.viewer_devices.len() >= loom_protocol::LIVE_MAX_VIEWERS {
                return Err(LiveRuntimeError::new(
                    429,
                    "live_viewer_limit",
                    "the live viewer limit has been reached",
                ));
            }
            record
                .session
                .viewer_devices
                .push(actor_device_id.to_owned());
            record.session.viewer_devices.sort();
            record.session.revision = record.session.revision.saturating_add(1);
            record.session.last_seen_at_ms = unix_time_millis();
            push_state_event(record, "viewer_joined");
        }
        self.changed.notify_all();
        Ok(snapshot(record))
    }

    fn resume(
        &self,
        actor_device_id: &str,
        envelope: LiveControlEnvelope,
    ) -> std::result::Result<LiveSessionRuntimeSnapshot, LiveRuntimeError> {
        let LiveControlMessage::ResumeRequest(resume) = &envelope.message else {
            return Err(LiveRuntimeError::new(
                400,
                "live_message_invalid",
                "live resume requires resume_request",
            ));
        };
        if resume.requester_device_id != actor_device_id {
            return Err(LiveRuntimeError::new(
                403,
                "live_resume_identity_mismatch",
                "the authenticated device must resume itself",
            ));
        }
        let mut sessions = self.lock_state()?;
        let record = active_record_mut(&mut sessions, &envelope.session_id)?;
        ensure_member(record, actor_device_id)?;
        accept_control_sequence(record, actor_device_id, envelope.epoch, envelope.sequence)?;
        push_state_event(record, "device_resumed");
        Ok(snapshot(record))
    }

    fn close(
        &self,
        actor_device_id: &str,
        envelope: LiveControlEnvelope,
    ) -> std::result::Result<LiveSessionRuntimeSnapshot, LiveRuntimeError> {
        let LiveControlMessage::SessionEnd(end) = &envelope.message else {
            return Err(LiveRuntimeError::new(
                400,
                "live_message_invalid",
                "live close requires session_end",
            ));
        };
        if end.ended_by_device_id != actor_device_id {
            return Err(LiveRuntimeError::new(
                403,
                "live_close_identity_mismatch",
                "the authenticated device must close as itself",
            ));
        }
        let mut sessions = self.lock_state()?;
        let record = active_record_mut(&mut sessions, &envelope.session_id)?;
        if record.session.source_device_id != actor_device_id {
            return Err(LiveRuntimeError::new(
                403,
                "live_close_denied",
                "only the source device can close the live session",
            ));
        }
        if !record.pending_trigger_dispatches.is_empty() {
            return Err(LiveRuntimeError::new(
                409,
                "live_trigger_dispatch_pending",
                "the live session has trigger actions awaiting dispatch",
            ));
        }
        accept_control_sequence(record, actor_device_id, envelope.epoch, envelope.sequence)?;
        record.closed = true;
        record.session.visibility_state = LiveVisibilityState::Closed;
        record.session.controller_device = None;
        record.controller_expires_at_ms = None;
        record.frames.clear();
        record.observations.clear();
        record.trigger_registrations.clear();
        record.trigger_audits.clear();
        record.trigger_idempotency_keys.clear();
        record.trigger_idempotency_order.clear();
        record.pending_trigger_dispatches.clear();
        record.session.revision = record.session.revision.saturating_add(1);
        push_state_event(record, "session_closed");
        self.changed.notify_all();
        Ok(snapshot(record))
    }

    fn list(&self) -> std::result::Result<Vec<LiveSessionRuntimeSnapshot>, LiveRuntimeError> {
        let mut sessions = self.lock_state()?;
        for record in sessions.values_mut() {
            expire_controller(record);
        }
        Ok(sessions
            .values()
            .filter(|record| !record.closed)
            .map(|record| {
                let mut discovery = snapshot(record);
                discovery.observations.clear();
                discovery.triggers.clear();
                discovery.trigger_audits.clear();
                discovery
            })
            .collect())
    }

    fn get(
        &self,
        session_id: &str,
    ) -> std::result::Result<LiveSessionRuntimeSnapshot, LiveRuntimeError> {
        let mut sessions = self.lock_state()?;
        let record = sessions
            .get_mut(session_id)
            .ok_or_else(|| not_found(session_id))?;
        expire_controller(record);
        Ok(snapshot(record))
    }

    fn get_for_member(
        &self,
        session_id: &str,
        actor_device_id: &str,
    ) -> std::result::Result<LiveSessionRuntimeSnapshot, LiveRuntimeError> {
        let mut sessions = self.lock_state()?;
        let record = sessions
            .get_mut(session_id)
            .ok_or_else(|| not_found(session_id))?;
        expire_controller(record);
        ensure_member(record, actor_device_id)?;
        Ok(snapshot(record))
    }

    fn lock_state(
        &self,
    ) -> std::result::Result<
        std::sync::MutexGuard<'_, BTreeMap<String, LiveSessionRecord>>,
        LiveRuntimeError,
    > {
        self.state.lock().map_err(|_| unavailable())
    }
}
