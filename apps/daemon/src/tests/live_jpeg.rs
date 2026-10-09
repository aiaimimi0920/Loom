mod live_jpeg_tests {
    use super::*;
    const FIXTURE: &[u8] = include_bytes!("../../../../protocol/fixtures/live-jpeg-v1.nllv");
    const SESSION: &str = "live:jpeg";
    type Socket = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>;

    struct TestDaemon {
        port: u16,
        store: SharedLiveSessionStore,
        shutdown: mpsc::Sender<()>,
        worker: Option<JoinHandle<()>>,
        root: PathBuf,
    }
    impl TestDaemon {
        fn new() -> Self {
            let root = unique_temp_dir("live-jpeg");
            let daemon = LoomDaemon::bind(
                DaemonConfig::localhost(0)
                    .with_bearer_token("live-jpeg-test")
                    .with_control_plane_root(&root),
            )
            .unwrap();
            let store = Arc::clone(&daemon.runtime.live_sessions);
            store
                .create("device-source", live_start_envelope(SESSION))
                .unwrap();
            for viewer in ["viewer:new", "viewer:old"] {
                store
                    .attach_viewer(viewer, live_viewer_envelope(SESSION, viewer))
                    .unwrap();
            }
            let port = daemon.local_addr().unwrap().port();
            let (shutdown, receiver) = mpsc::channel();
            let worker = Some(thread::spawn(move || {
                daemon.serve_until(receiver).unwrap();
            }));
            Self {
                port,
                store,
                shutdown,
                worker,
                root,
            }
        }

        fn connect(&self, role: &str, device: &str, offer: &'static str) -> (Socket, String) {
            use tungstenite::client::IntoClientRequest;
            use tungstenite::http::header::{AUTHORIZATION, SEC_WEBSOCKET_PROTOCOL};
            let mut request = format!("ws://127.0.0.1:{}/v1/live/media?sessionId=live%3Ajpeg&role={role}&deviceId={device}", self.port)
                .into_client_request().unwrap();
            request
                .headers_mut()
                .insert(AUTHORIZATION, "Bearer live-jpeg-test".parse().unwrap());
            request
                .headers_mut()
                .insert(SEC_WEBSOCKET_PROTOCOL, offer.parse().unwrap());
            let (mut socket, response) = tungstenite::connect(request).unwrap();
            if let tungstenite::stream::MaybeTlsStream::Plain(tcp) = socket.get_mut() {
                tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                tcp.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
            }
            (
                socket,
                response.headers()[SEC_WEBSOCKET_PROTOCOL]
                    .to_str()
                    .unwrap()
                    .to_owned(),
            )
        }
    }
    impl Drop for TestDaemon {
        fn drop(&mut self) {
            let _ = self.shutdown.send(());
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            let _ = fs::remove_dir_all(&self.root);
        }
    }
    fn read_frame(socket: &mut Socket) -> Vec<u8> {
        let started = Instant::now();
        loop {
            assert!(
                started.elapsed() < Duration::from_secs(4),
                "live JPEG frame deadline exceeded"
            );
            match socket.read().unwrap() {
                tungstenite::Message::Binary(bytes) => return bytes,
                tungstenite::Message::Ping(bytes) => {
                    socket.send(tungstenite::Message::Pong(bytes)).unwrap()
                }
                other => panic!("unexpected live message: {other:?}"),
            }
        }
    }
    fn latest(store: &LiveSessionStore) -> StoredLiveFrame {
        store
            .wait_for_frame(SESSION, 0, 0, Duration::ZERO)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn live_jpeg_in_place_bgra_expansion_preserves_pixels_and_allocation() {
        for channels in 1..=4 {
            for pixels in [1, 2, 7, 257] {
                let compact: Vec<u8> = (0..pixels * channels)
                    .map(|index| (index % 251) as u8)
                    .collect();
                let mut output = vec![0xa5; 64 + pixels * 4];
                output[64..64 + compact.len()].copy_from_slice(&compact);
                let pointer = output.as_ptr();
                let capacity = output.capacity();
                expand_live_bgra_in_place(&mut output[64..], channels);
                assert_eq!(output.as_ptr(), pointer);
                assert_eq!(output.capacity(), capacity);
                assert_eq!(&output[..64], &[0xa5; 64]);
                for (source, actual) in compact
                    .chunks_exact(channels)
                    .zip(output[64..].chunks_exact(4))
                {
                    let expected = match channels {
                        1 => [source[0], source[0], source[0], 255],
                        2 => [source[0], source[0], source[0], source[1]],
                        3 => [source[2], source[1], source[0], 255],
                        4 => [source[2], source[1], source[0], source[3]],
                        _ => unreachable!(),
                    };
                    assert_eq!(actual, expected);
                }
            }
        }
    }

    #[test]
    fn live_jpeg_direct_buffer_matches_previous_conversion_for_rgb_and_gray() {
        for color in [image::ExtendedColorType::Rgb8, image::ExtendedColorType::L8] {
            for (width, height) in [(1, 1), (2, 3), (17, 9), (64, 32)] {
                let channels = if color == image::ExtendedColorType::Rgb8 {
                    3
                } else {
                    1
                };
                let pixels: Vec<u8> = (0..width * height * channels)
                    .map(|index| ((index * 29) % 256) as u8)
                    .collect();
                let mut jpeg = Vec::new();
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85)
                    .encode(&pixels, width, height, color)
                    .unwrap();
                let mut wire = FIXTURE[..64].to_vec();
                wire[40..44].copy_from_slice(&width.to_be_bytes());
                wire[44..48].copy_from_slice(&height.to_be_bytes());
                wire[52..56].copy_from_slice(&(jpeg.len() as u32).to_be_bytes());
                wire.extend_from_slice(&jpeg);
                let store = LiveSessionStore::new();
                store
                    .create("device-source", live_start_envelope(SESSION))
                    .unwrap();
                store
                    .publish_frame(SESSION, "device-source", wire.clone())
                    .unwrap();
                let frame = latest(&store);
                let actual = frame.decode_legacy().unwrap();
                let mut expected_pixels = image::load_from_memory(&jpeg)
                    .unwrap()
                    .into_rgba8()
                    .into_raw();
                for pixel in expected_pixels.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
                let mut expected = wire[..64].to_vec();
                expected[52..56].copy_from_slice(&(expected_pixels.len() as u32).to_be_bytes());
                expected[57] = 1;
                expected.extend_from_slice(&expected_pixels);
                assert_eq!(actual.as_ref(), &expected);
                assert_eq!(frame.bytes.as_ref(), &wire);
            }
        }
    }

    #[test]
    fn live_jpeg_real_upgrade_mixed_viewers_preserve_bytes_and_legacy_pixels() {
        assert_eq!(LiveBinaryFrame::decode_header(FIXTURE).unwrap().0, 1);
        let daemon = TestDaemon::new();
        let offer = "loom.live.jpeg.v1, loom.live.v1";
        let (mut source, source_protocol) = daemon.connect("source", "device-source", offer);
        let (mut new, new_protocol) = daemon.connect("viewer", "viewer:new", offer);
        let (mut old, old_protocol) = daemon.connect("viewer", "viewer:old", "loom.live.v1");
        assert_eq!(source_protocol, loom_protocol::LIVE_JPEG_PROTOCOL_VERSION);
        assert_eq!(new_protocol, source_protocol);
        assert_eq!(old_protocol, loom_protocol::LIVE_PROTOCOL_VERSION);
        source
            .send(tungstenite::Message::Binary(FIXTURE.to_vec()))
            .unwrap();
        assert_eq!(read_frame(&mut new), FIXTURE);
        let legacy = read_frame(&mut old);
        let decoded = LiveBinaryFrame::decode(&legacy).unwrap();
        assert_eq!(decoded.metadata.codec, loom_protocol::LiveCodec::RawBgra);
        assert_eq!((decoded.epoch, decoded.metadata.frame_id), (1, 9));
        let mut expected = image::load_from_memory(&FIXTURE[64..])
            .unwrap()
            .into_rgba8()
            .into_raw();
        for pixel in expected.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        assert_eq!(decoded.payload, expected);
        let stored = latest(&daemon.store);
        let cached = stored
            .representation(LiveMediaProfile::Legacy)
            .unwrap()
            .unwrap();
        assert_eq!(cached.as_ref(), &legacy);
        assert!(Arc::ptr_eq(
            &cached,
            &stored
                .representation(LiveMediaProfile::Legacy)
                .unwrap()
                .unwrap()
        ));
        assert!(Arc::ptr_eq(
            &stored.bytes,
            &stored
                .representation(LiveMediaProfile::Jpeg)
                .unwrap()
                .unwrap()
        ));
        // JPEG-capable viewers also accept raw when a source uses the legacy path.
        let mut raw = legacy;
        raw[16..24].copy_from_slice(&10_u64.to_be_bytes());
        source
            .send(tungstenite::Message::Binary(raw.clone()))
            .unwrap();
        assert_eq!(read_frame(&mut new), raw);
        assert_eq!(read_frame(&mut old), raw);
        assert_eq!(
            daemon.store.list().unwrap()[0].session.frame_stream.codec,
            loom_protocol::LiveCodec::RawBgra
        );
    }

    #[test]
    fn live_jpeg_legacy_source_serves_new_viewer_but_rejects_unnegotiated_jpeg() {
        let daemon = TestDaemon::new();
        let (mut source, protocol) = daemon.connect("source", "device-source", "loom.live.v1");
        assert_eq!(protocol, "loom.live.v1");
        let (mut viewer, selected) = daemon.connect("viewer", "viewer:new", "loom.live.jpeg.v1");
        assert_eq!(selected, loom_protocol::LIVE_JPEG_PROTOCOL_VERSION);
        let raw = encoded_live_frame(1);
        source
            .send(tungstenite::Message::Binary(raw.clone()))
            .unwrap();
        assert_eq!(read_frame(&mut viewer), raw);
        source
            .send(tungstenite::Message::Binary(FIXTURE.to_vec()))
            .unwrap();
        let reply = source.read();
        assert!(matches!(reply, Ok(tungstenite::Message::Close(_)) | Err(_)));
        assert_eq!(daemon.store.get(SESSION).unwrap().published_frames, 1);
    }

    #[test]
    fn live_jpeg_bounds_fail_before_storage_and_cache_never_blocks_direct_delivery() {
        let daemon = TestDaemon::new();
        for (offset, value) in [(5, 0), (40, 1), (44, 1), (56, 2), (64, 0)] {
            let mut bytes = FIXTURE.to_vec();
            bytes[offset] = value;
            assert!(daemon
                .store
                .publish_frame(SESSION, "device-source", bytes)
                .is_err());
        }
        assert_eq!(daemon.store.get(SESSION).unwrap().buffered_frames, 0);
        daemon
            .store
            .publish_frame(SESSION, "device-source", FIXTURE.to_vec())
            .unwrap();
        let frame = latest(&daemon.store);
        let guard = frame.legacy.lock().unwrap();
        assert!(frame
            .representation(LiveMediaProfile::Legacy)
            .unwrap()
            .is_none());
        assert!(Arc::ptr_eq(
            &frame.bytes,
            &frame
                .representation(LiveMediaProfile::Jpeg)
                .unwrap()
                .unwrap()
        ));
        drop(guard);
        assert!(daemon
            .store
            .viewer_frame_authorized(SESSION, "viewer:new", 1));
        assert!(!daemon
            .store
            .viewer_frame_authorized(SESSION, "viewer:new", 2));
        assert!(!daemon
            .store
            .viewer_frame_authorized(SESSION, "viewer:uninvited", 1));
        daemon
            .store
            .state
            .lock()
            .unwrap()
            .get_mut(SESSION)
            .unwrap()
            .closed = true;
        assert!(!daemon
            .store
            .viewer_frame_authorized(SESSION, "viewer:new", 1));
    }

    #[test]
    fn live_jpeg_wall_adapter_preserves_original_frame_identity_and_time() {
        let store = LiveSessionStore::new();
        store
            .create("device-source", live_start_envelope(SESSION))
            .unwrap();
        store
            .publish_frame(SESSION, "device-source", FIXTURE.to_vec())
            .unwrap();
        let frame = latest(&store);
        let raw = frame
            .representation(LiveMediaProfile::Legacy)
            .unwrap()
            .unwrap();
        let (_, metadata) = LiveBinaryFrame::decode_header(&raw).unwrap();
        assert_eq!(metadata.capture_timestamp_ms, 100);
        assert_eq!(metadata.encode_timestamp_ms, 110);
        assert_eq!((frame.epoch, frame.frame_id), (1, 9));
        assert_eq!(raw[56..58], [1, 1]);
        let (width, height, rgb) = scale_wall_bgra(&raw[64..], 64, 32, 32, 32).unwrap();
        assert_eq!((width, height, rgb.len()), (32, 16, 32 * 16 * 3));
        assert!(rgb[0] > 220 && rgb[1] < 25 && rgb[2] < 30);
        let root = unique_temp_dir("live-jpeg-wall");
        {
            let walls = Arc::new(WallStore::open(&root).unwrap());
            for codec in [WallMediaCodec::RawBgra, WallMediaCodec::Png] {
                let profile = WallMediaProfile {
                    codec,
                    width: 32,
                    height: 32,
                    fps: 10,
                };
                let bytes = encode_wall_media_frame(&frame, &walls, profile)
                    .unwrap()
                    .unwrap();
                let decoded = WallMediaFrame::decode(&bytes).unwrap();
                assert_eq!((decoded.metadata.epoch, decoded.metadata.frame_id), (1, 9));
                assert_eq!(
                    decoded.metadata.received_timestamp_ms,
                    walls.media_timestamp(frame.received_at).unwrap()
                );
                assert_eq!(decoded.metadata.capture_timestamp_ms, 100);
                assert_eq!(decoded.metadata.codec, codec);
            }
        }
        fs::remove_dir_all(root).unwrap();
    }
}
