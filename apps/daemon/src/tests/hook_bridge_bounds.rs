mod hook_bridge_bounds {
    use super::*;

    #[test]
    fn incomplete_upgrade_has_a_total_deadline_even_when_bytes_keep_arriving() {
        use std::io::Write;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let started = Instant::now();
            assert!(accept_authenticated_hook_socket(stream, "fixture").is_err());
            assert!(started.elapsed() < Duration::from_secs(4));
        });
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .set_write_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        stream.write_all(b"GET / HTTP/1.1\r\nX-Slow: ").unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && !server.is_finished() {
            if stream.write_all(b"x").is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        server.join().unwrap();
    }

    #[test]
    fn history_byte_eviction_and_oversized_gaps_force_snapshot_recovery() {
        let hub = HookBridgeBroadcastHub::new();
        let message = "x".repeat(HOOK_BRIDGE_HISTORY_BYTES / 2);
        hub.record(&[message.clone(), message]);
        hub.record(&["next".to_owned()]);
        let (cursor, reset, entries) = hub.wait_after(HOOK_BRIDGE_RECOVERY_CURSOR, Duration::ZERO);
        assert!(reset);
        assert_eq!(entries.len(), 2);
        assert!(hub.history.0.lock().unwrap().bytes <= HOOK_BRIDGE_HISTORY_BYTES);
        hub.record(&["x".repeat(HOOK_BRIDGE_HISTORY_BYTES + 1)]);
        let (next, reset, entries) = hub.wait_after(cursor, Duration::ZERO);
        assert!(reset);
        assert_eq!(next, cursor + 1);
        assert!(entries.is_empty());
        assert_eq!(hub.history.0.lock().unwrap().bytes, 0);
        assert!(!hub.wait_after(next, Duration::ZERO).1);
    }

    #[test]
    fn broadcast_drain_yields_after_one_bounded_batch() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut socket = accept_authenticated_hook_socket(stream, "fixture").unwrap();
            let (tx, rx) = hook_bridge_broadcast_channel(HOOK_BRIDGE_DRAIN_BATCH + 1, 1024);
            for _ in 0..=HOOK_BRIDGE_DRAIN_BATCH {
                assert!(tx.try_send("event"));
            }
            assert!(drain_hook_bridge_broadcasts(&mut socket, &rx));
            assert_eq!(rx.try_recv().unwrap(), "event");
        });
        let (_socket, _) =
            tungstenite::connect(authenticated_hook_test_request(port, "fixture")).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn subscription_releases_bytes_on_receive_and_disconnects_on_byte_overflow() {
        let (tx, rx) = hook_bridge_broadcast_channel(4, 6);
        assert!(tx.try_send("abc"));
        assert_eq!(tx.queued_bytes.load(Ordering::SeqCst), 3);
        assert_eq!(rx.try_recv().unwrap(), "abc");
        assert_eq!(tx.queued_bytes.load(Ordering::SeqCst), 0);
        assert!(tx.try_send("abc"));
        assert!(tx.try_send("def"));
        assert!(!tx.try_send("x"));
        assert_eq!(tx.queued_bytes.load(Ordering::SeqCst), 6);
        assert!(matches!(
            rx.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
        assert!(!tx.try_send("later"));
    }

    #[test]
    fn subscription_count_overflow_and_oversized_events_fail_without_blocking() {
        let (tx, rx) = hook_bridge_broadcast_channel(2, 100);
        assert!(tx.try_send("a"));
        assert!(tx.try_send("b"));
        assert!(!tx.try_send("c"));
        assert_eq!(tx.queued_bytes.load(Ordering::SeqCst), 2);
        assert!(matches!(
            rx.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
        let (tx, rx) = hook_bridge_broadcast_channel(2, 3);
        assert!(!tx.try_send("oversized"));
        assert_eq!(tx.queued_bytes.load(Ordering::SeqCst), 0);
        assert!(matches!(
            rx.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
        let (tx, rx) = hook_bridge_broadcast_channel(2, 100);
        drop(rx);
        assert!(!tx.try_send("closed"));
        assert_eq!(tx.queued_bytes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn slow_subscription_is_removed_without_stalling_healthy_subscribers() {
        let hub = HookBridgeBroadcastHub::new();
        let (slow, _slow_guard) =
            register_hook_bridge_subscription(&hub, vec![HOOK_EVENT_SETTINGS_UPDATED.to_owned()]);
        let (fast, _fast_guard) =
            register_hook_bridge_subscription(&hub, vec![HOOK_EVENT_SETTINGS_UPDATED.to_owned()]);
        let message = hook_protocol_event_json(HOOK_EVENT_SETTINGS_UPDATED, &json!({}));
        for _ in 0..=HOOK_BRIDGE_SUBSCRIPTION_CAPACITY {
            broadcast_hook_bridge_messages(&hub, std::slice::from_ref(&message));
            assert_eq!(fast.try_recv().unwrap(), message);
        }
        assert_eq!(hub.subscriber_count(), 1);
        assert!(matches!(
            slow.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
    }

    #[test]
    fn heartbeat_pings_live_peers_and_retires_unresponsive_peers() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut socket = accept_authenticated_hook_socket(stream, "fixture").unwrap();
            let start = Instant::now();
            let mut heartbeat = HookBridgeHeartbeat::new(start);
            assert!(heartbeat.poll(&mut socket, start + HOOK_BRIDGE_PING_INTERVAL));
            assert!(matches!(
                socket.read().unwrap(),
                tungstenite::Message::Pong(_)
            ));
            heartbeat.received(start + HOOK_BRIDGE_PING_INTERVAL);
            assert!(heartbeat.poll(&mut socket, start + HOOK_BRIDGE_IDLE_TIMEOUT));
            assert!(!heartbeat.poll(
                &mut socket,
                start + HOOK_BRIDGE_IDLE_TIMEOUT + HOOK_BRIDGE_PING_INTERVAL
            ));
        });
        let (mut socket, _) =
            tungstenite::connect(authenticated_hook_test_request(port, "fixture")).unwrap();
        assert!(matches!(
            socket.read().unwrap(),
            tungstenite::Message::Ping(_)
        ));
        socket.flush().unwrap();
        server.join().unwrap();
    }

    #[test]
    fn hook_bridge_connection_cap_reclaims_slots_and_stop_joins_idle_clients() {
        let _guard = lock_ignoring_poison(&ENV_LOCK);
        let root = unique_temp_dir("hook-bridge-bounds");
        let runtime = test_daemon_runtime(&root, None);
        let status = start_test_hook_bridge(&runtime, r#"{"port":0}"#);
        let port = status["port"].as_u64().unwrap() as u16;
        let token = test_bound_daemon_token(port).unwrap();
        let mut sockets = Vec::new();
        for _ in 0..HOOK_BRIDGE_MAX_CONNECTIONS {
            sockets.push(
                tungstenite::connect(authenticated_hook_test_request(port, &token))
                    .unwrap()
                    .0,
            );
        }
        assert!(tungstenite::connect(authenticated_hook_test_request(port, &token)).is_err());
        assert_eq!(
            runtime
                .hook_bridge
                .lock()
                .unwrap()
                .connections
                .workers
                .lock()
                .unwrap()
                .len(),
            HOOK_BRIDGE_MAX_CONNECTIONS
        );
        drop(sockets.pop());
        let deadline = Instant::now() + Duration::from_secs(3);
        while !runtime
            .hook_bridge
            .lock()
            .unwrap()
            .connections
            .has_capacity()
        {
            assert!(Instant::now() < deadline, "connection slot not reclaimed");
            thread::sleep(Duration::from_millis(10));
        }
        sockets.push(
            tungstenite::connect(authenticated_hook_test_request(port, &token))
                .unwrap()
                .0,
        );
        let started = Instant::now();
        stop_test_hook_bridge(&runtime);
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(runtime
            .hook_bridge
            .lock()
            .unwrap()
            .connections
            .workers
            .lock()
            .unwrap()
            .is_empty());
        assert_eq!(
            runtime
                .hook_bridge
                .lock()
                .unwrap()
                .connected_clients
                .load(Ordering::SeqCst),
            0
        );
        drop(sockets);
        drop(runtime);
        fs::remove_dir_all(root).unwrap();
    }
}
