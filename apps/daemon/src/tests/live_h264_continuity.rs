mod live_h264_continuity_tests {
    use super::*;
    const SESSION: &str = "live:h264-continuity";
    fn store() -> LiveSessionStore {
        let store = LiveSessionStore::new();
        store.create("device-source", live_start_envelope(SESSION)).unwrap();
        store
    }
    pub(super) fn frame(id: u64, keyframe: bool) -> Vec<u8> {
        let mut frame = LiveBinaryFrame::decode(&encoded_live_frame(id)).unwrap();
        frame.metadata.codec = loom_protocol::LiveCodec::H264;
        frame.metadata.keyframe = keyframe;
        // Syntax fixture only; actual hardware AU is exercised independently in Hook.
        frame.payload = if keyframe { vec![0,0,1,0x67,0x42,0,30,0,0,1,0x68,1,0,0,1,0x65,1] }
            else { vec![0,0,1,0x41,1] };
        frame.encode().unwrap()
    }
    fn publish(store: &LiveSessionStore, id: u64, keyframe: bool) {
        store.publish_frame(SESSION, "device-source", frame(id, keyframe)).unwrap();
    }
    fn next(store: &LiveSessionStore, epoch: u64, id: u64) -> Option<u64> {
        store.wait_for_frame(SESSION, epoch, id, Duration::ZERO).unwrap().map(|frame| frame.frame_id)
    }
    #[test]
    fn h264_source_requires_initial_idr_and_rejects_gaps_before_state_mutation() {
        let store = store();
        assert_eq!(store.publish_frame(SESSION, "device-source", frame(1, false)).unwrap_err().code, "live_h264_keyframe_required");
        publish(&store, 1, true);
        publish(&store, 2, false);
        assert!(store.publish_frame(SESSION, "device-source", frame(4, false)).is_err());
        assert_eq!(store.lock_state().unwrap()[SESSION].last_frame_id, 2);
        publish(&store, 5, true);
        assert_eq!(store.lock_state().unwrap()[SESSION].frames.len(), 1);
        assert_eq!(next(&store, 1, 2), Some(5));
    }
    #[test]
    fn h264_cursor_advances_in_order_and_overflow_waits_for_idr_without_busy_wakeup() {
        let store = store();
        publish(&store, 1, true);
        publish(&store, 2, false);
        assert_eq!(next(&store, 0, 0), Some(1));
        publish(&store, 3, false);
        assert_eq!(next(&store, 1, 1), Some(2));
        assert_eq!(next(&store, 1, 2), Some(3));
        assert_eq!(next(&store, 0, 0), None);
        assert!(!has_newer_frame(&store.lock_state().unwrap()[SESSION], 0, 0));
        publish(&store, 4, false);
        assert_eq!(next(&store, 1, 1), None);
        publish(&store, 5, true);
        assert_eq!(next(&store, 1, 1), Some(5));
        assert_eq!(next(&store, 2, 0), None);
        assert!(store.lock_state().unwrap()[SESSION].frames.len() <= 2);
    }
    #[test]
    fn h264_dimension_change_needs_idr_and_raw_transition_preserves_latest_wins() {
        let store = store();
        publish(&store, 1, true);
        let mut changed = LiveBinaryFrame::decode(&frame(2, false)).unwrap();
        changed.metadata.width = 4;
        assert!(store.publish_frame(SESSION, "device-source", changed.encode().unwrap()).is_err());
        let mut keyframe = LiveBinaryFrame::decode(&frame(2, true)).unwrap();
        keyframe.metadata.width = 4;
        store.publish_frame(SESSION, "device-source", keyframe.encode().unwrap()).unwrap();
        assert_eq!(store.lock_state().unwrap()[SESSION].frames.len(), 1);
        for id in [3, 4] { store.publish_frame(SESSION, "device-source", encoded_live_frame(id)).unwrap(); }
        assert_eq!(next(&store, 0, 0), Some(4));
        assert_eq!(next(&store, 1, 2), Some(4));
        assert!(store.publish_frame(SESSION, "device-source", frame(5, false)).is_err());
    }
    #[test]
    fn h264_cannot_enter_legacy_profiles_or_bypass_source_identity() {
        let store = store();
        assert!(store.publish_frame(SESSION, "another-device", frame(1, true)).is_err());
        assert_eq!(store.lock_state().unwrap()[SESSION].last_frame_id, 0);
        publish(&store, 1, true);
        let stored = store.wait_for_frame(SESSION, 0, 0, Duration::ZERO).unwrap().unwrap();
        for profile in [LiveMediaProfile::Legacy, LiveMediaProfile::Jpeg] {
            assert!(!profile.accepts(&stored.bytes));
            assert_eq!(stored.representation(profile).unwrap_err(), "live_h264_not_negotiated");
        }
    }
}
