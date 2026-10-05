// Retain only a session digest after the one-use handshake nonce has been consumed.
struct LiveMediaDeviceGrant {
    device_id: String,
    device_session: Option<(SharedDeviceRegistryStore, String)>,
}

impl LiveMediaDeviceGrant {
    fn new(
        request: &ParsedHttpRequest,
        devices: &SharedDeviceRegistryStore,
        device_id: String,
    ) -> Self {
        Self {
            device_id,
            device_session: request
                .authorization_credential("Device")
                .map(|token| (Arc::clone(devices), sha256_bytes(token.as_bytes()))),
        }
    }

    fn valid(&self) -> bool {
        // Administrator admission remains separate; never reconsume the handshake nonce.
        let Some((devices, token_hash)) = &self.device_session else {
            return true;
        };
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

    fn authorize(
        &self,
        sessions: &SharedLiveSessionStore,
        session_id: &str,
        role: LiveDeviceRole,
    ) -> std::result::Result<(), LiveRuntimeError> {
        if !self.valid() {
            return Err(LiveRuntimeError::new(
                403,
                "live_media_device_revoked",
                "paired device session is no longer valid",
            ));
        }
        sessions.authorize_media(session_id, &self.device_id, role)
    }
}
