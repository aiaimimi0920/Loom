mod live_video_negotiation_tests {
    use super::*;
    use loom_protocol::{LiveVideoSourceControl, LiveVideoViewerControl};

    fn store() -> SharedLiveSessionStore {
        let store = Arc::new(LiveSessionStore::new());
        store
            .create("device-source", live_start_envelope("live:video"))
            .unwrap();
        store
    }

    fn allowed(store: &LiveSessionStore) -> bool {
        let LiveVideoSourceControl::VideoPolicy { h264_allowed, .. } =
            store.video_policy("live:video").unwrap();
        h264_allowed
    }

    #[test]
    fn video_policy_requires_every_actual_consumer_and_releases_wall_demand() {
        let store = store();
        assert!(!allowed(&store));
        let first = LiveVideoViewerLease::acquire(&store, "live:video", true).unwrap();
        assert!(allowed(&store));
        let old_or_wall = LiveVideoViewerLease::acquire(&store, "live:video", false).unwrap();
        assert!(!allowed(&store));
        drop(old_or_wall);
        assert!(allowed(&store));
        first
            .control(LiveVideoViewerControl::VideoFallback { epoch: 1 })
            .unwrap();
        assert!(!allowed(&store));
        assert!(!first.accepts_h264());
        first
            .control(LiveVideoViewerControl::KeyframeRequest { epoch: 1 })
            .unwrap();
        assert!(!allowed(&store), "fallback is sticky for this socket");
        drop(first);
        assert!(!allowed(&store));
        assert!(store.lock_state().unwrap()["live:video"]
            .video
            .viewers
            .is_empty());
    }

    #[test]
    fn video_control_is_epoch_fenced_coalesced_and_bounded() {
        let store = store();
        let lease = LiveVideoViewerLease::acquire(&store, "live:video", true).unwrap();
        let before = store.video_policy("live:video").unwrap();
        for _ in 0..100 {
            lease
                .control(LiveVideoViewerControl::KeyframeRequest { epoch: 1 })
                .unwrap();
        }
        assert_eq!(store.video_policy("live:video").unwrap(), before);
        assert!(lease
            .control(LiveVideoViewerControl::VideoFallback { epoch: 2 })
            .is_err());
        assert!(allowed(&store));
        let mut leases = Vec::new();
        for _ in 1..LIVE_MEDIA_CONNECTION_LIMIT {
            leases.push(LiveVideoViewerLease::acquire(&store, "live:video", true).unwrap());
        }
        assert!(LiveVideoViewerLease::acquire(&store, "live:video", true).is_err());
        drop(leases);
        assert_eq!(
            store.lock_state().unwrap()["live:video"]
                .video
                .viewers
                .len(),
            1
        );
    }

    #[test]
    fn old_video_lease_cannot_mutate_recreated_session() {
        let store = store();
        let old = LiveVideoViewerLease::acquire(&store, "live:video", false).unwrap();
        // Simulate record replacement; the public create API deliberately rejects reused IDs.
        store.lock_state().unwrap().remove("live:video");
        store
            .create("device-source", live_start_envelope("live:video"))
            .unwrap();
        let new = LiveVideoViewerLease::acquire(&store, "live:video", true).unwrap();
        assert_ne!(old.id, new.id);
        assert!(old
            .control(LiveVideoViewerControl::VideoFallback { epoch: 1 })
            .is_err());
        drop(old);
        assert!(allowed(&store));
    }

    #[test]
    fn video_overflow_requests_idr_but_idle_cursor_does_not() {
        let store = store();
        for id in 1..=4 {
            store
                .publish_frame(
                    "live:video",
                    "device-source",
                    super::live_h264_continuity_tests::frame(id, id == 1),
                )
                .unwrap();
        }
        let before = store.video_policy("live:video").unwrap();
        store.request_video_recovery("live:video", 1, 4);
        assert_eq!(before, store.video_policy("live:video").unwrap());
        store.request_video_recovery("live:video", 0, 0);
        assert_ne!(before, store.video_policy("live:video").unwrap());
    }

    #[test]
    fn video_fast_compatibility_cycle_retains_recovery_and_drops_only_until_idr() {
        let store = store();
        let _viewer = LiveVideoViewerLease::acquire(&store, "live:video", true).unwrap();
        let publish = |id, keyframe| {
            store
                .publish_media_frame(
                    "live:video",
                    "device-source",
                    super::live_h264_continuity_tests::frame(id, keyframe),
                    Some(LiveMediaProfile::H264),
                )
                .unwrap()
        };
        publish(1, true);
        let initial = store.video_policy("live:video").unwrap();
        let legacy = LiveVideoViewerLease::acquire(&store, "live:video", false).unwrap();
        publish(2, false);
        drop(legacy);
        // Even if the source missed the intermediate false policy, it must not be disconnected.
        publish(3, false);
        assert_eq!(store.get("live:video").unwrap().last_frame_id, 1);
        {
            let mut state = store.lock_state().unwrap();
            let video = &mut state.get_mut("live:video").unwrap().video;
            assert!(video.pending_keyframe);
            video.last_keyframe_request = Some(Instant::now() - Duration::from_millis(300));
        }
        assert_ne!(initial, store.video_policy("live:video").unwrap());
        publish(4, true);
        publish(5, false);
        assert_eq!(store.get("live:video").unwrap().published_frames, 3);
        assert_eq!(store.get("live:video").unwrap().last_frame_id, 5);
    }
}
