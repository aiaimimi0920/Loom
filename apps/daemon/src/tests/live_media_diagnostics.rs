mod live_media_diagnostics_tests {
    use super::*;

    fn store() -> LiveSessionStore {
        let store = LiveSessionStore::new();
        store
            .create("device-source", live_start_envelope("live:metrics"))
            .unwrap();
        store
    }

    fn latest(store: &LiveSessionStore) -> StoredLiveFrame {
        store
            .wait_for_frame("live:metrics", 0, 0, Duration::ZERO)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn live_media_diagnostics_separate_source_gaps_evictions_and_socket_writes() {
        let store = store();
        for id in [1, 2, 4] {
            store
                .publish_frame("live:metrics", "device-source", encoded_live_frame(id))
                .unwrap();
        }
        let before = store.get("live:metrics").unwrap();
        assert_eq!(before.buffered_frames, 2);
        assert_eq!(
            before
                .media_diagnostics
                .as_ref()
                .unwrap()
                .received_binary_bytes,
            3 * 80
        );
        assert_eq!(
            before
                .media_diagnostics
                .as_ref()
                .unwrap()
                .source_sequence_gaps,
            1
        );
        assert_eq!(
            before.media_diagnostics.as_ref().unwrap().buffer_evictions,
            1
        );
        assert_eq!(
            before.media_diagnostics.as_ref().unwrap().forwarded_frames,
            0
        );
        assert!(before
            .media_diagnostics
            .as_ref()
            .unwrap()
            .last_forward
            .is_none());

        let frame = latest(&store);
        let mut sample = LiveMediaForwardSample::selected("viewer:a", &frame, 1, 1);
        sample.write_succeeded = true;
        sample.socket_write_ms = 7;
        store.record_media_forward("live:metrics", sample);
        let snapshot = store.get("live:metrics").unwrap();
        let metrics = snapshot.media_diagnostics.unwrap();
        assert_eq!(metrics.forwarded_frames, 1);
        assert_eq!(metrics.forwarded_binary_bytes, 80);
        assert_eq!(metrics.viewer_skipped_frames, 2);
        assert_eq!(metrics.last_forward.unwrap().socket_write_ms, 7);
        // 编码字节、权限、控制序列和帧队列不受观测写入影响。
        assert_eq!(frame.bytes.as_ref(), &encoded_live_frame(4));
        assert_eq!(snapshot.buffered_frames, before.buffered_frames);
        assert_eq!(snapshot.session.controller_device, None);
    }

    #[test]
    fn live_media_diagnostics_count_failed_writes_without_claiming_delivery() {
        let store = store();
        store
            .publish_frame("live:metrics", "device-source", encoded_live_frame(9))
            .unwrap();
        let frame = latest(&store);
        for id in 0..1000 {
            let sample = LiveMediaForwardSample::selected(&format!("viewer:{id}"), &frame, 1, 1);
            store.record_media_forward("live:metrics", sample);
        }
        let metrics = store
            .get("live:metrics")
            .unwrap()
            .media_diagnostics
            .unwrap();
        assert_eq!(metrics.failed_writes, 1000);
        assert_eq!(metrics.forwarded_frames, 0);
        assert_eq!(metrics.forwarded_binary_bytes, 0);
        assert_eq!(metrics.viewer_skipped_frames, 0);
        assert_eq!(
            metrics.last_forward.as_ref().unwrap().viewer_device_id,
            "viewer:999"
        );
        let json = serde_json::to_string(&metrics).unwrap();
        assert!(json.len() < 600);
        assert!(!json.contains("viewer:998"));
    }

    #[test]
    fn live_media_diagnostics_do_not_confuse_late_join_or_epoch_with_skips() {
        let store = store();
        store
            .publish_frame("live:metrics", "device-source", encoded_live_frame(9))
            .unwrap();
        let frame = latest(&store);
        for (epoch, id) in [(0, 0), (1, 0), (0, 5), (2, 5)] {
            let sample = LiveMediaForwardSample::selected("viewer:a", &frame, epoch, id);
            assert_eq!(sample.skipped_frames, 0);
        }
        let mut sample = LiveMediaForwardSample::selected("viewer:a", &frame, 1, 3);
        sample.epoch = 2;
        sample.write_succeeded = true;
        store.record_media_forward("live:metrics", sample);
        assert_eq!(
            store
                .get("live:metrics")
                .unwrap()
                .media_diagnostics
                .unwrap()
                .forwarded_frames,
            0
        );
    }

    #[test]
    fn live_media_diagnostics_ignore_rejected_source_frames() {
        let store = store();
        assert!(store
            .publish_frame("live:metrics", "wrong-source", encoded_live_frame(1))
            .is_err());
        assert!(store
            .publish_frame("live:metrics", "device-source", vec![0; 80])
            .is_err());
        let metrics = store
            .get("live:metrics")
            .unwrap()
            .media_diagnostics
            .unwrap();
        assert_eq!(metrics.received_binary_bytes, 0);
        assert_eq!(metrics.source_sequence_gaps, 0);
    }

    #[test]
    fn live_media_diagnostics_are_member_only_and_ignore_closed_results() {
        let store = store();
        store
            .publish_frame("live:metrics", "device-source", encoded_live_frame(1))
            .unwrap();
        assert!(store.get_for_member("live:metrics", "outsider").is_err());
        assert!(store
            .get_for_member("live:metrics", "device-source")
            .unwrap()
            .media_diagnostics
            .is_some());
        let discovery = store.list().unwrap();
        assert!(discovery[0].media_diagnostics.is_none());
        assert!(!serde_json::to_string(&discovery)
            .unwrap()
            .contains("mediaDiagnostics"));
        let mut sample = LiveMediaForwardSample::selected("viewer:a", &latest(&store), 1, 0);
        sample.write_succeeded = true;
        store
            .state
            .lock()
            .unwrap()
            .get_mut("live:metrics")
            .unwrap()
            .closed = true;
        store.record_media_forward("live:metrics", sample);
        assert_eq!(
            store
                .get("live:metrics")
                .unwrap()
                .media_diagnostics
                .unwrap()
                .forwarded_frames,
            0
        );
    }

    #[test]
    fn live_media_diagnostics_follow_real_websocket_writes_and_independent_viewers() {
        let store = Arc::new(store());
        store
            .publish_frame("live:metrics", "device-source", encoded_live_frame(1))
            .unwrap();
        for viewer in ["viewer:a", "viewer:b"] {
            store
                .attach_viewer(viewer, live_viewer_envelope("live:metrics", viewer))
                .unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            client
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let (server, _) = listener.accept().unwrap();
            server
                .set_write_timeout(Some(LIVE_MEDIA_SOCKET_TIMEOUT))
                .unwrap();
            let sessions = Arc::clone(&store);
            let permit = reserve_live_media_connection(&store).unwrap();
            let (done, completed) = std::sync::mpsc::channel();
            let worker = thread::spawn(move || {
                let socket = tungstenite::WebSocket::from_raw_socket(
                    server,
                    tungstenite::protocol::Role::Server,
                    Some(live_media_websocket_config()),
                );
                run_live_media_socket(
                    socket,
                    sessions,
                    "live:metrics".to_owned(),
                    viewer.to_owned(),
                    LiveDeviceRole::Viewer,
                    0,
                    0,
                    permit,
                );
                let _ = done.send(());
            });
            let mut client = tungstenite::WebSocket::from_raw_socket(
                client,
                tungstenite::protocol::Role::Client,
                Some(live_media_websocket_config()),
            );
            let received = client.read();
            let _ = client.close(None);
            // 断言前清理本测试的 worker，失败也不能遗留等待中的 socket。
            let stopped = completed.recv_timeout(Duration::from_secs(3)).is_ok();
            if !stopped {
                store.media_cancelled.store(true, Ordering::SeqCst);
                store.changed.notify_all();
            }
            worker.join().unwrap();
            assert!(stopped);
            assert!(
                matches!(received, Ok(tungstenite::Message::Binary(bytes)) if bytes == encoded_live_frame(1))
            );
            assert_eq!(store.media_connections.load(Ordering::SeqCst), 0);
        }
        let snapshot = store.get("live:metrics").unwrap();
        assert!(snapshot.viewer_connections.is_empty());
        assert_eq!(
            snapshot
                .media_diagnostics
                .as_ref()
                .unwrap()
                .received_binary_bytes,
            80
        );
        assert_eq!(
            snapshot
                .media_diagnostics
                .as_ref()
                .unwrap()
                .forwarded_frames,
            2
        );
        assert_eq!(
            snapshot
                .media_diagnostics
                .as_ref()
                .unwrap()
                .forwarded_binary_bytes,
            160
        );
        assert_eq!(
            snapshot.media_diagnostics.as_ref().unwrap().failed_writes,
            0
        );
        assert_eq!(
            snapshot
                .media_diagnostics
                .unwrap()
                .last_forward
                .unwrap()
                .viewer_device_id,
            "viewer:b"
        );
    }
}
