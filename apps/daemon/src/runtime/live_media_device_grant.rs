// Retain only a session digest after the one-use handshake nonce has been consumed.
struct LiveMediaDeviceGrant {
    device_id: String,
    device_session: Option<(SharedDeviceRegistryStore, String, Arc<AtomicBool>)>,
}

impl LiveMediaDeviceGrant {
    fn new(
        request: &ParsedHttpRequest,
        devices: &SharedDeviceRegistryStore,
        device_id: String,
    ) -> Self {
        Self {
            device_id,
            device_session: request.authorization_credential("Device").map(|token| {
                let hash = sha256_bytes(token.as_bytes());
                let revoked = devices
                    .lock()
                    .ok()
                    .and_then(|store| store.sessions.get(&hash).map(|s| Arc::clone(&s.revoked)))
                    .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
                (Arc::clone(devices), hash, revoked)
            }),
        }
    }

    fn valid(&self) -> bool {
        // Administrator admission remains separate; never reconsume the handshake nonce.
        let Some((devices, token_hash, revoked)) = &self.device_session else {
            return true;
        };
        if revoked.load(Ordering::SeqCst) {
            return false;
        }
        devices.lock().ok().is_some_and(|store| {
            store.sessions.get(token_hash).is_some_and(|session| {
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

    fn revocation_close(&self) -> Option<tungstenite::protocol::CloseFrame<'static>> {
        let (_, _, revoked) = self.device_session.as_ref()?;
        // This sticky provenance belongs to this admitted session, not to all missing tokens.
        revoked
            .load(Ordering::SeqCst)
            .then(|| tungstenite::protocol::CloseFrame {
                code: tungstenite::protocol::frame::coding::CloseCode::Policy,
                reason: "live_media_device_revoked".into(),
            })
    }

    fn authorize(
        &self,
        sessions: &SharedLiveSessionStore,
        session_id: &str,
        role: LiveDeviceRole,
    ) -> std::result::Result<(), LiveRuntimeError> {
        if !self.valid() {
            return Err(LiveRuntimeError::new(
                403,
                if self.revocation_close().is_some() {
                    "live_media_device_revoked"
                } else {
                    "live_media_device_session_invalid"
                },
                "paired device session is no longer valid",
            ));
        }
        sessions.authorize_media(session_id, &self.device_id, role)
    }
}
