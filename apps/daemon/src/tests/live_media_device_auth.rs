// Real paired-device sockets: revocation must stop existing media, not just new upgrades.
mod live_media_device_auth {
    use super::*;
    include!("live_media_device_revocation.rs");
    include!("live_media_device_renewal.rs");
    include!("live_viewer_renewal_policy.rs");
    include!("device_disabled_approval.rs");
    type Socket = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>;

    struct Fixture {
        port: u16,
        sessions: SharedLiveSessionStore,
        devices: SharedDeviceRegistryStore,
        server: ConcurrencyTestFixture,
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = unique_temp_dir("live-media-device-auth");
            let daemon =
                LoomDaemon::bind(DaemonConfig::localhost(0).with_control_plane_root(&root))
                    .unwrap();
            let port = daemon.local_addr().unwrap().port();
            let sessions = Arc::clone(&daemon.runtime.live_sessions);
            let devices = Arc::clone(&daemon.runtime.device_registry);
            let (tx, rx) = mpsc::channel();
            let worker = thread::spawn(move || daemon.serve_until(rx));
            Self {
                port,
                sessions,
                devices,
                server: ConcurrencyTestFixture::new(tx, worker),
                root,
            }
        }

        fn create(&self, source: &str, viewers: &[&str]) {
            let mut start = live_start_envelope("live:device-auth");
            let LiveControlMessage::SessionStart(message) = &mut start.message else {
                panic!("source fixture")
            };
            message.session.source_device_id = source.to_owned();
            message.requested_by_device_id = source.to_owned();
            self.sessions.create(source, start).unwrap();
            for viewer in viewers {
                self.sessions
                    .attach_viewer(viewer, live_viewer_envelope("live:device-auth", viewer))
                    .unwrap();
            }
        }

        fn connect(
            &self,
            token: &str,
            role: &str,
        ) -> std::result::Result<Socket, Box<tungstenite::Error>> {
            use tungstenite::{client::IntoClientRequest, http::HeaderValue};
            let mut request = format!(
                "ws://127.0.0.1:{}/v1/live/media?sessionId=live%3Adevice-auth&role={role}",
                self.port
            )
            .into_client_request()
            .unwrap();
            request.headers_mut().insert(
                "authorization",
                HeaderValue::from_str(&format!("Device {token}")).unwrap(),
            );
            request.headers_mut().insert(
                "x-loom-device-nonce",
                HeaderValue::from_str(&Uuid::new_v4().to_string()).unwrap(),
            );
            request.headers_mut().insert(
                "sec-websocket-protocol",
                HeaderValue::from_static(loom_protocol::LIVE_PROTOCOL_VERSION),
            );
            let (mut socket, _) = tungstenite::connect(request).map_err(Box::new)?;
            if let tungstenite::stream::MaybeTlsStream::Plain(tcp) = socket.get_mut() {
                tcp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                tcp.set_write_timeout(Some(Duration::from_secs(2))).unwrap();
            }
            Ok(socket)
        }

        fn wait(&self, accept: impl Fn(&LiveSessionRuntimeSnapshot) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if accept(&self.sessions.get("live:device-auth").unwrap()) {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "media connection cleanup deadline"
                );
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let stopped = self.server.finish();
            remove_test_dir(&self.root);
            if !thread::panicking() {
                stopped.expect("live media fixture shutdown failed");
            }
        }
    }

    fn binary(socket: &mut Socket) -> u64 {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            assert!(Instant::now() < deadline, "relayed binary frame deadline");
            match socket.read().unwrap() {
                tungstenite::Message::Binary(bytes) => {
                    return LiveBinaryFrame::decode(&bytes).unwrap().metadata.frame_id
                }
                tungstenite::Message::Ping(bytes) => {
                    socket.send(tungstenite::Message::Pong(bytes)).unwrap()
                }
                _ => panic!("expected real relayed binary frame"),
            }
        }
    }

    fn closed(
        socket: &mut Socket,
    ) -> Option<(tungstenite::protocol::frame::coding::CloseCode, String)> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            assert!(
                Instant::now() < deadline,
                "revoked media socket did not close"
            );
            match socket.read() {
                Ok(tungstenite::Message::Close(frame)) => {
                    return frame.map(|frame| (frame.code, frame.reason.into_owned()));
                }
                Err(tungstenite::Error::ConnectionClosed) => return None,
                Ok(tungstenite::Message::Ping(bytes)) => {
                    let _ = socket.send(tungstenite::Message::Pong(bytes));
                }
                other => panic!("revoked media must close without new binary frames: {other:?}"),
            }
        }
    }

    #[test]
    fn source_token_revocation_rejects_old_socket_publication() {
        let f = Fixture::new();
        let (owner, token) = wall_http::pair(f.port, "Live source grant");
        let (viewer, viewer_token) = wall_http::pair(f.port, "Live viewer grant");
        f.create(&owner, &[&viewer]);
        let mut source = f.connect(&token, "source").unwrap();
        let mut receiver = f.connect(&viewer_token, "viewer").unwrap();
        source
            .send(tungstenite::Message::Binary(encoded_live_frame(1)))
            .unwrap();
        assert_eq!(binary(&mut receiver), 1);
        f.devices.lock().unwrap().revoke_device_sessions(&owner);
        assert!(f.devices.lock().unwrap().devices[&owner].enabled);
        // send() may succeed into TCP buffering; daemon acceptance and close are the proof.
        let _ = source.send(tungstenite::Message::Binary(encoded_live_frame(2)));
        assert_eq!(closed(&mut source).unwrap().1, "live_media_device_revoked");
        f.wait(|s| !s.source_connected);
        let state = f.sessions.get("live:device-auth").unwrap();
        assert_eq!(state.last_frame_id, 1);
        assert!(!state.closed);
        assert_eq!(state.viewer_connections.get(&viewer), Some(&1));
        assert!(f.connect(&token, "source").is_err());
    }

    #[test]
    fn viewer_token_revocation_closes_idle_socket_and_preserves_peer() {
        let f = Fixture::new();
        let (owner, token) = wall_http::pair(f.port, "Live source peer");
        let (revoked, revoked_token) = wall_http::pair(f.port, "Revoked live viewer");
        let (peer, peer_token) = wall_http::pair(f.port, "Retained live viewer");
        f.create(&owner, &[&revoked, &peer]);
        let mut source = f.connect(&token, "source").unwrap();
        let mut receiver = f.connect(&revoked_token, "viewer").unwrap();
        let mut retained = f.connect(&peer_token, "viewer").unwrap();
        source
            .send(tungstenite::Message::Binary(encoded_live_frame(1)))
            .unwrap();
        assert_eq!(binary(&mut receiver), 1);
        assert_eq!(binary(&mut retained), 1);
        f.devices.lock().unwrap().revoke_device_sessions(&revoked);
        assert_eq!(
            closed(&mut receiver).unwrap().1,
            "live_media_device_revoked"
        );
        f.wait(|s| !s.viewer_connections.contains_key(&revoked));
        source
            .send(tungstenite::Message::Binary(encoded_live_frame(2)))
            .unwrap();
        assert_eq!(binary(&mut retained), 2);
        assert!(f.connect(&revoked_token, "viewer").is_err());
        assert!(f.sessions.get("live:device-auth").unwrap().source_connected);
    }

    #[test]
    fn device_disable_and_removal_close_existing_source_through_http() {
        for method in ["PUT", "DELETE"] {
            let f = Fixture::new();
            let (owner, token) = wall_http::pair(f.port, "Managed live source");
            f.create(&owner, &[]);
            let mut source = f.connect(&token, "source").unwrap();
            f.wait(|s| s.source_connected);
            let body = json!({"name":"Managed live source", "kind":"computer", "address":"127.0.0.1", "enabled":false}).to_string();
            let response = http_request(
                f.port,
                method,
                &format!("/v1/devices/{owner}"),
                if method == "PUT" { Some(&body) } else { None },
            );
            assert_eq!(response.split_whitespace().nth(1), Some("200"));
            assert_eq!(
                closed(&mut source),
                Some((
                    tungstenite::protocol::frame::coding::CloseCode::Policy,
                    "live_media_device_revoked".to_owned(),
                ))
            );
            f.wait(|s| !s.source_connected);
            assert!(f.connect(&token, "source").is_err());
            assert!(!f.sessions.get("live:device-auth").unwrap().closed);
        }
    }

    #[test]
    fn device_grant_checks_current_session_and_device_without_consuming_nonces() {
        let f = Fixture::new();
        let (owner, token) = wall_http::pair(f.port, "Grant validation");
        let hash = sha256_bytes(token.as_bytes());
        let grant = LiveMediaDeviceGrant {
            device_id: owner.clone(),
            device_session: Some((
                Arc::clone(&f.devices),
                hash.clone(),
                Arc::clone(&f.devices.lock().unwrap().sessions[&hash].revoked),
            )),
        };
        let (original, expiry, epoch, used) = {
            let store = f.devices.lock().unwrap();
            let session = &store.sessions[&hash];
            (
                store.devices[&owner].clone(),
                session.expires_at_ms,
                session.session_epoch,
                session.used_nonces.clone(),
            )
        };
        for state in [
            "missing-token",
            "expired",
            "foreign-session",
            "epoch",
            "disabled",
            "unapproved",
            "removed",
        ] {
            assert!(grant.valid());
            {
                let mut store = f.devices.lock().unwrap();
                match state {
                    "missing-token" => {
                        store.sessions.remove(&hash);
                    }
                    "expired" => {
                        store.sessions.get_mut(&hash).unwrap().expires_at_ms = unix_time_millis()
                    }
                    "foreign-session" => {
                        store.sessions.get_mut(&hash).unwrap().device_id = "foreign-device".into()
                    }
                    "epoch" => store.devices.get_mut(&owner).unwrap().session_epoch += 1,
                    "disabled" => store.devices.get_mut(&owner).unwrap().enabled = false,
                    "unapproved" => {
                        store.devices.get_mut(&owner).unwrap().approval = "pending".into()
                    }
                    "removed" => {
                        store.devices.remove(&owner);
                    }
                    _ => unreachable!(),
                }
            }
            assert!(!grant.valid(), "obsolete grant accepted: {state}");
            let mut store = f.devices.lock().unwrap();
            store.devices.insert(owner.clone(), original.clone());
            store.sessions.insert(
                hash.clone(),
                ActiveDeviceSession {
                    device_id: owner.clone(),
                    expires_at_ms: expiry,
                    session_epoch: epoch,
                    used_nonces: used.clone(),
                    revoked: Arc::new(AtomicBool::new(false)),
                },
            );
        }
        assert!(grant.valid());
        assert_eq!(f.devices.lock().unwrap().sessions[&hash].used_nonces, used);
        assert!(LiveMediaDeviceGrant {
            device_id: "admin-local".into(),
            device_session: None
        }
        .valid());
    }

    #[test]
    fn device_grant_registry_lock_failure_is_denied() {
        let f = Fixture::new();
        let (owner, token) = wall_http::pair(f.port, "Unavailable registry");
        let grant = LiveMediaDeviceGrant {
            device_id: owner,
            device_session: Some((
                Arc::clone(&f.devices),
                sha256_bytes(token.as_bytes()),
                Arc::new(AtomicBool::new(false)),
            )),
        };
        let devices = Arc::clone(&f.devices);
        assert!(thread::spawn(move || {
            let _held = devices.lock().unwrap();
            panic!("poison test registry");
        })
        .join()
        .is_err());
        assert!(!grant.valid());
    }
}
