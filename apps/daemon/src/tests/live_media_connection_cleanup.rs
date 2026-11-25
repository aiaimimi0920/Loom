// Actual guard/permit drops must retire closed sessions, never resurrect or impersonate streams.
mod live_media_connection_cleanup_tests {
    use super::*;
    const SESSION: &str = "live:closed-media-cleanup";
    fn fixture() -> SharedLiveSessionStore {
        let store = Arc::new(LiveSessionStore::new());
        store
            .create("device-source", live_start_envelope(SESSION))
            .unwrap();
        store
            .attach_viewer(
                "device-viewer",
                live_viewer_envelope(SESSION, "device-viewer"),
            )
            .unwrap();
        store
    }
    fn connection(
        store: &SharedLiveSessionStore,
        device: &str,
        role: LiveDeviceRole,
    ) -> (LiveMediaRoleGuard, LiveMediaConnectionPermit) {
        let permit = reserve_live_media_connection(store).unwrap();
        store
            .set_media_connected(SESSION, device, role, true)
            .unwrap();
        (
            LiveMediaRoleGuard {
                sessions: store.clone(),
                session_id: SESSION.into(),
                device_id: device.into(),
                role,
            },
            permit,
        )
    }
    fn close(store: &SharedLiveSessionStore) {
        store
            .close(
                "device-source",
                LiveControlEnvelope {
                    protocol_version: loom_protocol::LIVE_PROTOCOL_VERSION.into(),
                    session_id: SESSION.into(),
                    epoch: 1,
                    sequence: 2,
                    message: LiveControlMessage::SessionEnd(loom_protocol::LiveSessionEnd {
                        ended_by_device_id: "device-source".into(),
                        reason: loom_protocol::LiveSessionEndReason::Closed,
                        detail: None,
                    }),
                },
            )
            .unwrap();
    }
    #[test]
    fn live_media_closed_session_actual_guards_retire_source_and_each_viewer() {
        let store = fixture();
        let source = connection(&store, "device-source", LiveDeviceRole::Source);
        let first = connection(&store, "device-viewer", LiveDeviceRole::Viewer);
        let second = connection(&store, "device-viewer", LiveDeviceRole::Viewer);
        close(&store);
        let closed = store.get(SESSION).unwrap();
        assert!(closed.closed && closed.source_connected);
        assert_eq!(closed.viewer_connections.get("device-viewer"), Some(&2));
        assert_eq!(store.media_connections.load(Ordering::SeqCst), 3);
        drop(first);
        assert_eq!(
            store
                .get(SESSION)
                .unwrap()
                .viewer_connections
                .get("device-viewer"),
            Some(&1)
        );
        drop(source);
        assert!(!store.get(SESSION).unwrap().source_connected);
        drop(second);
        let retired = store.get(SESSION).unwrap();
        assert!(retired.closed && retired.viewer_connections.is_empty());
        assert!(retired.session.controller_device.is_none());
        assert_eq!(
            retired.session.visibility_state,
            LiveVisibilityState::Closed
        );
        assert_eq!(store.media_connections.load(Ordering::SeqCst), 0);
        assert!(store.list().unwrap().is_empty());
    }
    #[test]
    fn live_media_closed_retirement_is_identity_checked_and_never_reopens() {
        let store = fixture();
        let source = connection(&store, "device-source", LiveDeviceRole::Source);
        let viewer = connection(&store, "device-viewer", LiveDeviceRole::Viewer);
        close(&store);
        for role in [LiveDeviceRole::Source, LiveDeviceRole::Viewer] {
            assert_eq!(
                store
                    .set_media_connected(SESSION, "outsider", role, false)
                    .unwrap_err()
                    .status,
                403
            );
        }
        for (device, role) in [
            ("device-source", LiveDeviceRole::Source),
            ("device-viewer", LiveDeviceRole::Viewer),
        ] {
            assert_eq!(
                store
                    .set_media_connected(SESSION, device, role, true)
                    .unwrap_err()
                    .status,
                404
            );
        }
        let unchanged = store.get(SESSION).unwrap();
        assert!(unchanged.closed && unchanged.source_connected);
        assert_eq!(unchanged.viewer_connections.get("device-viewer"), Some(&1));
        assert_eq!(store.media_connections.load(Ordering::SeqCst), 2);
        drop((source, viewer));
        let retired = store.get(SESSION).unwrap();
        assert!(
            retired.closed && !retired.source_connected && retired.viewer_connections.is_empty()
        );
        assert_eq!(store.media_connections.load(Ordering::SeqCst), 0);
        assert!(store
            .authorize_media(SESSION, "device-source", LiveDeviceRole::Source)
            .is_err());
        assert!(store
            .authorize_media(SESSION, "device-viewer", LiveDeviceRole::Viewer)
            .is_err());
    }
}
