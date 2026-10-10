// Per-socket capabilities, including walls. Membership alone never proves decoder support.
#[derive(Default)]
struct LiveVideoNegotiation {
    viewers: BTreeMap<u64, bool>,
    keyframe_sequence: u64,
    last_keyframe_request: Option<Instant>,
    pending_keyframe: bool,
    requires_idr: bool,
}

impl LiveVideoNegotiation {
    fn h264_allowed(&self) -> bool {
        !self.viewers.is_empty() && self.viewers.values().all(|supported| *supported)
    }

    fn request_keyframe(&mut self) {
        self.pending_keyframe = true;
        self.flush_keyframe_request();
    }

    fn flush_keyframe_request(&mut self) {
        // Coalesce all viewers, including automatic overflow recovery, to at most 4 requests/s.
        if self.pending_keyframe
            && self
                .last_keyframe_request
                .is_none_or(|at| at.elapsed() >= Duration::from_millis(250))
        {
            self.keyframe_sequence = self.keyframe_sequence.saturating_add(1);
            self.last_keyframe_request = Some(Instant::now());
            self.pending_keyframe = false;
        }
    }

    fn demand_changed(&mut self, was_allowed: bool) {
        if was_allowed != self.h264_allowed() {
            self.requires_idr = true;
        }
        self.request_keyframe();
    }
}

struct LiveVideoViewerLease {
    sessions: SharedLiveSessionStore,
    session_id: String,
    id: u64,
}

impl LiveVideoViewerLease {
    // Caller must first authorize the real viewer or wall grant; no identity comes from Text.
    fn acquire(
        sessions: &SharedLiveSessionStore,
        session_id: &str,
        h264: bool,
    ) -> Result<Self, LiveRuntimeError> {
        let mut state = sessions.lock_state()?;
        let record = active_record_mut(&mut state, session_id)?;
        let video = &mut record.video;
        if video.viewers.len() >= LIVE_MEDIA_CONNECTION_LIMIT {
            return Err(LiveRuntimeError::new(
                503,
                "live_media_busy",
                "video viewer limit reached",
            ));
        }
        // Store-wide identities prevent an old socket's Drop from removing a recreated session lease.
        let id = sessions
            .next_video_lease_id
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |id| id.checked_add(1))
            .map_err(|_| unavailable())?;
        let was_allowed = video.h264_allowed();
        video.viewers.insert(id, h264);
        video.demand_changed(was_allowed);
        sessions.changed.notify_all();
        Ok(Self {
            sessions: Arc::clone(sessions),
            session_id: session_id.to_owned(),
            id,
        })
    }

    fn accepts_h264(&self) -> bool {
        self.sessions.state.lock().ok().is_some_and(|state| {
            state
                .get(&self.session_id)
                .filter(|record| !record.closed)
                .and_then(|record| record.video.viewers.get(&self.id))
                .copied()
                .unwrap_or(false)
        })
    }

    fn control(
        &self,
        control: loom_protocol::LiveVideoViewerControl,
    ) -> Result<(), LiveRuntimeError> {
        let mut state = self.sessions.lock_state()?;
        let record = active_record_mut(&mut state, &self.session_id)?;
        if control.epoch() != record.epoch {
            return Err(LiveRuntimeError::new(
                409,
                "live_video_epoch_stale",
                "video control epoch is stale",
            ));
        }
        let was_allowed = record.video.h264_allowed();
        let supported = record
            .video
            .viewers
            .get_mut(&self.id)
            .ok_or_else(unavailable)?;
        match control {
            loom_protocol::LiveVideoViewerControl::VideoFallback { .. } => {
                *supported = false;
                record.video.demand_changed(was_allowed);
            }
            loom_protocol::LiveVideoViewerControl::KeyframeRequest { .. } => {
                if *supported {
                    record.video.request_keyframe();
                }
            }
        }
        self.sessions.changed.notify_all();
        Ok(())
    }
}

impl Drop for LiveVideoViewerLease {
    fn drop(&mut self) {
        if let Ok(mut state) = self.sessions.state.lock() {
            if let Some(record) = state.get_mut(&self.session_id) {
                let was_allowed = record.video.h264_allowed();
                if record.video.viewers.remove(&self.id).is_some() {
                    record.video.demand_changed(was_allowed);
                }
            }
        }
        self.sessions.changed.notify_all();
    }
}

impl LiveSessionStore {
    fn video_policy(
        &self,
        session_id: &str,
    ) -> Result<loom_protocol::LiveVideoSourceControl, LiveRuntimeError> {
        let mut state = self.lock_state()?;
        let record = active_record_mut(&mut state, session_id)?;
        record.video.flush_keyframe_request();
        Ok(loom_protocol::LiveVideoSourceControl::VideoPolicy {
            epoch: record.epoch,
            h264_allowed: record.video.h264_allowed(),
            keyframe_sequence: record.video.keyframe_sequence,
        })
    }

    fn request_video_recovery(&self, session_id: &str, epoch: u64, frame_id: u64) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(record) = state.get_mut(session_id).filter(|record| !record.closed) {
                if record.frames.back().is_some_and(|frame| {
                    frame.bytes.get(57) == Some(&2)
                        && (frame.epoch != epoch || frame.frame_id > frame_id)
                }) && select_live_media_frame(record, epoch, frame_id).is_none()
                {
                    record.video.request_keyframe();
                }
            }
        }
    }
}

fn send_live_video_policy(
    socket: &mut tungstenite::WebSocket<TcpStream>,
    sessions: &LiveSessionStore,
    session_id: &str,
    previous: &mut Option<loom_protocol::LiveVideoSourceControl>,
) -> bool {
    let Ok(policy) = sessions.video_policy(session_id) else {
        return false;
    };
    if previous.as_ref() == Some(&policy) {
        return true;
    }
    let Ok(text) = serde_json::to_string(&policy) else {
        return false;
    };
    if socket.send(tungstenite::Message::Text(text)).is_err() {
        return false;
    }
    *previous = Some(policy);
    true
}

fn service_live_video_viewer_control(
    socket: &mut tungstenite::WebSocket<TcpStream>,
    lease: &LiveVideoViewerLease,
    grant: &LiveMediaDeviceGrant,
    profile: LiveMediaProfile,
) -> bool {
    if profile != LiveMediaProfile::H264 {
        return service_live_viewer_control_messages(socket);
    }
    for _ in 0..4 {
        match socket.read() {
            Ok(tungstenite::Message::Text(text)) => {
                if grant
                    .authorize(&lease.sessions, &lease.session_id, LiveDeviceRole::Viewer)
                    .is_err()
                {
                    return false;
                }
                let Some(control) = loom_protocol::LiveVideoViewerControl::parse(&text) else {
                    return false;
                };
                if lease.control(control).is_err() {
                    return false;
                }
            }
            Ok(tungstenite::Message::Ping(bytes)) => {
                if socket.send(tungstenite::Message::Pong(bytes)).is_err() {
                    return false;
                }
            }
            Ok(tungstenite::Message::Pong(_)) => {}
            Ok(tungstenite::Message::Close(close)) => {
                let _ = socket.close(close);
                return false;
            }
            Err(error) if hook_bridge_read_timed_out(&error) => return true,
            _ => return false,
        }
    }
    true
}
