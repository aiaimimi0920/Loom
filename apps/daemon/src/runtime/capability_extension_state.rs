// Per-connection extension session state and replay protection.
#[derive(Default)]
struct ExtensionConnectionState {
    hook_session_id: Option<String>,
    extension_session_id: Option<String>,
    negotiated_features: HashSet<String>,
    consumed_gestures: HashSet<String>,
}

/// Maximum gesture-bearing commands accepted during one extension session.
///
/// The set deliberately fails closed at capacity. Evicting old tokens would
/// make them replayable, while a new extension handshake creates a fresh
/// session identifier and resets the bounded replay cache safely.
const MAX_CONSUMED_GESTURES: usize = 1_024;

impl ExtensionConnectionState {
    fn record_hook_handshake(&mut self, response: &str) {
        let Ok(response) = serde_json::from_str::<HookHandshakeResponse>(response) else {
            return;
        };
        self.hook_session_id = Some(response.session_id);
        self.extension_session_id = None;
        self.negotiated_features.clear();
        self.consumed_gestures.clear();
    }

    fn begin_extension_session(&mut self, session_id: String, features: &[String]) {
        self.extension_session_id = Some(session_id);
        self.negotiated_features = features.iter().cloned().collect();
        self.consumed_gestures.clear();
    }

    fn extension_session_matches(&self, session_id: &str) -> bool {
        self.extension_session_id.as_deref() == Some(session_id)
    }

    fn has_feature(&self, feature: &str) -> bool {
        self.negotiated_features.contains(feature)
    }

    fn client_gesture_is_available(&self, token: &str) -> bool {
        self.consumed_gestures.len() < MAX_CONSUMED_GESTURES
            && !self.consumed_gestures.contains(token)
    }

    fn record_client_gesture(&mut self, token: &str) -> bool {
        if !self.client_gesture_is_available(token) {
            return false;
        }
        self.consumed_gestures.insert(token.to_owned())
    }
}

#[cfg(test)]
mod capability_extension_state_tests {
    use super::*;

    #[test]
    fn gesture_replay_cache_fails_closed_at_capacity() {
        let mut state = ExtensionConnectionState::default();
        for index in 0..MAX_CONSUMED_GESTURES {
            assert!(state.record_client_gesture(&format!("gesture-{index}")));
        }

        assert!(!state.client_gesture_is_available("gesture-0"));
        assert!(!state.client_gesture_is_available("gesture-new"));
        assert!(!state.record_client_gesture("gesture-new"));
        assert_eq!(state.consumed_gestures.len(), MAX_CONSUMED_GESTURES);
    }

    #[test]
    fn new_extension_session_resets_the_gesture_epoch() {
        let mut state = ExtensionConnectionState::default();
        assert!(state.record_client_gesture("gesture-0"));

        state.begin_extension_session("extension:new".to_owned(), &[]);

        assert!(state.client_gesture_is_available("gesture-0"));
        assert!(state.extension_session_matches("extension:new"));
    }
}
