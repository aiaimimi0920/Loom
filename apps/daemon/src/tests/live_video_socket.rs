mod live_video_socket_tests {
    use super::live_jpeg_tests::{read_frame, TestDaemon};
    use super::*;
    use loom_protocol::LiveVideoSourceControl;
    const OFFER: &str = "loom.live.h264.v1, loom.live.jpeg.v1, loom.live.v1";
    type Socket = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>;

    fn policy(socket: &mut Socket, allowed: bool) {
        let start = Instant::now();
        loop {
            assert!(start.elapsed() < Duration::from_secs(4));
            match socket.read().unwrap() {
                tungstenite::Message::Text(text) => {
                    let LiveVideoSourceControl::VideoPolicy {
                        epoch,
                        h264_allowed,
                        ..
                    } = serde_json::from_str(&text).unwrap();
                    assert_eq!(epoch, 1);
                    if h264_allowed == allowed {
                        return;
                    }
                }
                tungstenite::Message::Ping(bytes) => {
                    socket.send(tungstenite::Message::Pong(bytes)).unwrap()
                }
                other => panic!("unexpected policy message: {other:?}"),
            }
        }
    }

    #[test]
    fn live_video_real_socket_negotiates_ordered_aus_and_old_viewer_fallback() {
        let daemon = TestDaemon::new();
        let (mut source, selected) = daemon.connect("source", "device-source", OFFER);
        assert_eq!(selected, loom_protocol::LIVE_H264_PROTOCOL_VERSION);
        policy(&mut source, false);
        let (mut viewer, selected) = daemon.connect("viewer", "viewer:new", OFFER);
        assert_eq!(selected, loom_protocol::LIVE_H264_PROTOCOL_VERSION);
        policy(&mut source, true);
        for id in 1..=2 {
            let frame = super::live_h264_continuity_tests::frame(id, id == 1);
            source
                .send(tungstenite::Message::Binary(frame.clone()))
                .unwrap();
            assert_eq!(read_frame(&mut viewer), frame);
        }
        let (mut old, _) = daemon.connect("viewer", "viewer:old", "loom.live.v1");
        policy(&mut source, false);
        source
            .send(tungstenite::Message::Binary(
                super::live_h264_continuity_tests::frame(3, false),
            ))
            .unwrap();
        // JPEG remains available on the video profile, and old viewers still receive BGRA.
        let jpeg = include_bytes!("../../../../protocol/fixtures/live-jpeg-v1.nllv");
        source
            .send(tungstenite::Message::Binary(jpeg.to_vec()))
            .unwrap();
        assert_eq!(read_frame(&mut viewer), jpeg);
        assert_eq!(
            LiveBinaryFrame::decode(&read_frame(&mut old))
                .unwrap()
                .metadata
                .codec,
            loom_protocol::LiveCodec::RawBgra
        );
        old.close(None).unwrap();
        let _ = old.read();
        policy(&mut source, true);
        assert_eq!(daemon.store.get("live:jpeg").unwrap().published_frames, 3);
    }

    #[test]
    fn live_video_real_socket_fallback_is_sticky_and_stale_control_disconnects() {
        let daemon = TestDaemon::new();
        let (mut source, _) = daemon.connect("source", "device-source", OFFER);
        let (mut viewer, _) = daemon.connect("viewer", "viewer:new", OFFER);
        policy(&mut source, true);
        viewer
            .send(tungstenite::Message::Text(
                r#"{"type":"video_fallback","epoch":1}"#.into(),
            ))
            .unwrap();
        policy(&mut source, false);
        viewer
            .send(tungstenite::Message::Text(
                r#"{"type":"keyframe_request","epoch":2}"#.into(),
            ))
            .unwrap();
        assert!(matches!(
            viewer.read(),
            Ok(tungstenite::Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed)
        ));
        // A new connection is a fresh decoder capability, not a stale socket resurrecting state.
        let (_rejoined, _) = daemon.connect("viewer", "viewer:new", OFFER);
        policy(&mut source, true);
    }
}
