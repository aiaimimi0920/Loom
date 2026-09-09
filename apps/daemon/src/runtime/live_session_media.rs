// Bounded binary-frame relay and tracked WebSocket worker lifecycle.
impl LiveSessionStore {
    fn publish_frame(
        &self,
        session_id: &str,
        source_device_id: &str,
        bytes: Vec<u8>,
    ) -> std::result::Result<(), LiveRuntimeError> {
        let frame = LiveBinaryFrame::decode(&bytes)
            .map_err(|error| LiveRuntimeError::new(400, "live_frame_invalid", error.to_string()))?;
        let mut sessions = self.lock_state()?;
        let record = active_record_mut(&mut sessions, session_id)?;
        if record.session.source_device_id != source_device_id {
            return Err(LiveRuntimeError::new(
                403,
                "live_source_identity_mismatch",
                "only the source device may publish live frames",
            ));
        }
        let capacity = usize::from(record.session.frame_stream.max_buffered_frames);
        if !(2..=3).contains(&capacity) {
            return Err(LiveRuntimeError::new(
                500,
                "live_frame_buffer_invalid",
                "live session frame buffer capacity must stay between 2 and 3",
            ));
        }
        if frame.epoch != record.epoch || frame.metadata.frame_id <= record.last_frame_id {
            return Err(LiveRuntimeError::new(
                409,
                "live_frame_sequence_invalid",
                "the live frame epoch or sequence is stale",
            ));
        }
        let gap = frame
            .metadata
            .frame_id
            .saturating_sub(record.last_frame_id.saturating_add(1));
        record.relay_dropped_frames = record.relay_dropped_frames.saturating_add(gap);
        record.last_frame_id = frame.metadata.frame_id;
        record.published_frames = record.published_frames.saturating_add(1);
        record.session.last_seen_at_ms = unix_time_millis();
        if record.frames.len() == capacity {
            record.frames.pop_front();
            record.relay_dropped_frames = record.relay_dropped_frames.saturating_add(1);
        }
        record.frames.push_back(StoredLiveFrame {
            epoch: frame.epoch,
            frame_id: frame.metadata.frame_id,
            bytes: Arc::new(bytes),
        });
        self.changed.notify_all();
        Ok(())
    }

    fn wait_for_frame(
        &self,
        session_id: &str,
        after_epoch: u64,
        after_frame_id: u64,
        timeout: Duration,
    ) -> std::result::Result<Option<StoredLiveFrame>, LiveRuntimeError> {
        let sessions = self.lock_state()?;
        let (sessions, _) = self
            .changed
            .wait_timeout_while(sessions, timeout, |sessions| {
                sessions.get(session_id).is_some_and(|record| {
                    !record.closed
                        && !has_newer_frame(record, after_epoch, after_frame_id)
                        && !self.media_cancelled.load(Ordering::SeqCst)
                })
            })
            .map_err(|_| unavailable())?;
        let record = sessions
            .get(session_id)
            .ok_or_else(|| not_found(session_id))?;
        if record.closed {
            return Err(not_found(session_id));
        }
        Ok(record
            .frames
            .back()
            .filter(|frame| {
                frame.epoch > after_epoch
                    || (frame.epoch == after_epoch && frame.frame_id > after_frame_id)
            })
            .cloned())
    }

    fn set_media_connected(
        &self,
        session_id: &str,
        device_id: &str,
        role: LiveDeviceRole,
        connected: bool,
    ) -> std::result::Result<(), LiveRuntimeError> {
        let mut sessions = self.lock_state()?;
        let record = active_record_mut(&mut sessions, session_id)?;
        let mut revoke_reason = None;
        match role {
            LiveDeviceRole::Source => {
                if record.session.source_device_id != device_id {
                    return Err(LiveRuntimeError::new(
                        403,
                        "live_source_identity_mismatch",
                        "source media identity mismatch",
                    ));
                }
                if connected && record.source_connected {
                    return Err(LiveRuntimeError::new(
                        409,
                        "live_source_connected",
                        "the live source already has a media connection",
                    ));
                }
                record.source_connected = connected;
                if !connected && record.session.controller_device.is_some() {
                    revoke_reason = Some("controller_revoked_source_disconnected".to_owned());
                }
            }
            LiveDeviceRole::Viewer => {
                ensure_viewer(record, device_id)?;
                let count = record
                    .viewer_connections
                    .entry(device_id.to_owned())
                    .or_default();
                if connected {
                    *count = count.saturating_add(1);
                } else {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        record.viewer_connections.remove(device_id);
                        if record.session.controller_device.as_deref() == Some(device_id) {
                            revoke_reason = Some(format!(
                                "controller_revoked_viewer_disconnected:{device_id}"
                            ));
                        }
                    }
                }
            }
            LiveDeviceRole::Controller => {
                return Err(LiveRuntimeError::new(
                    400,
                    "live_media_role_invalid",
                    "controllers do not own a media stream",
                ));
            }
        }
        if let Some(reason) = revoke_reason {
            record.session.controller_device = None;
            record.controller_expires_at_ms = None;
            record.session.revision = record.session.revision.saturating_add(1);
            push_state_event(record, &reason);
        }
        push_state_event(
            record,
            if connected {
                "media_connected"
            } else {
                "media_disconnected"
            },
        );
        self.changed.notify_all();
        Ok(())
    }

    fn authorize_media(
        &self,
        session_id: &str,
        device_id: &str,
        role: LiveDeviceRole,
    ) -> std::result::Result<(), LiveRuntimeError> {
        let sessions = self.lock_state()?;
        let record = sessions
            .get(session_id)
            .ok_or_else(|| not_found(session_id))?;
        if record.closed {
            return Err(not_found(session_id));
        }
        match role {
            LiveDeviceRole::Source if record.session.source_device_id == device_id => Ok(()),
            LiveDeviceRole::Viewer
                if record
                    .session
                    .viewer_devices
                    .iter()
                    .any(|id| id == device_id) =>
            {
                Ok(())
            }
            LiveDeviceRole::Controller => Err(LiveRuntimeError::new(
                400,
                "live_media_role_invalid",
                "controllers do not own a media stream",
            )),
            _ => Err(LiveRuntimeError::new(
                403,
                "live_media_denied",
                "the device is not authorized for this media role",
            )),
        }
    }

    fn status(&self) -> LiveRelayStatus {
        let Ok(sessions) = self.state.lock() else {
            return LiveRelayStatus::default();
        };
        let mut status = LiveRelayStatus {
            media_connections: self.media_connections.load(Ordering::SeqCst),
            ..LiveRelayStatus::default()
        };
        for record in sessions.values().filter(|record| !record.closed) {
            status.active_sessions += 1;
            status.connected_sources += usize::from(record.source_connected);
            status.connected_viewers += record.viewer_connections.values().sum::<usize>();
            status.controller_leases += usize::from(record.session.controller_device.is_some());
            status.buffered_frames += record.frames.len();
            status.published_frames = status
                .published_frames
                .saturating_add(record.published_frames);
            status.relay_dropped_frames = status
                .relay_dropped_frames
                .saturating_add(record.relay_dropped_frames);
        }
        status
    }

    fn track_media_worker(&self, worker: JoinHandle<()>) {
        self.reap_media_workers();
        self.media_workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(worker);
    }

    fn reap_media_workers(&self) {
        let finished = {
            let mut workers = self
                .media_workers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut finished = Vec::new();
            let mut index = 0;
            while index < workers.len() {
                if workers[index].is_finished() {
                    finished.push(workers.swap_remove(index));
                } else {
                    index += 1;
                }
            }
            finished
        };
        for worker in finished {
            let _ = worker.join();
        }
    }

    fn shutdown_media_workers(&self) {
        self.media_cancelled.store(true, Ordering::SeqCst);
        self.changed.notify_all();
        let workers = std::mem::take(
            &mut *self
                .media_workers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for worker in workers {
            let _ = worker.join();
        }
    }
}
