// Real HTTP + signed pairing; source events are inspected separately from physical input proof.
struct WallInputFixture {
    server: ConcurrencyTestFixture,
    port: u16,
    sessions: SharedLiveSessionStore,
    devices: SharedDeviceRegistryStore,
    device_id: String,
    token: String,
    bindings: [Value; 2],
    layout: Value,
    _root: Root,
}

impl WallInputFixture {
    fn new() -> Self {
        Self::with_identification(false)
    }

    fn with_identification(identify: bool) -> Self {
        let root = Root::new();
        let daemon = LoomDaemon::bind(
            DaemonConfig::localhost(0)
                .with_control_plane_root(&root.0)
                .with_bounded_request_executor(4, 16),
        )
        .unwrap();
        let port = daemon.local_addr().unwrap().port();
        let sessions = Arc::clone(&daemon.runtime.live_sessions);
        let devices = Arc::clone(&daemon.runtime.device_registry);
        let (tx, rx) = mpsc::channel();
        let server = ConcurrencyTestFixture::new(tx, thread::spawn(move || daemon.serve_until(rx)));
        let (device_id, token) = pair(port, "Wall input source and two outputs");
        let mut start = live_start_envelope("live-1", "wall-input-test");
        let LiveControlMessage::SessionStart(message) = &mut start.message else {
            panic!("fixture")
        };
        message.session.source_device_id = device_id.clone();
        message.requested_by_device_id = device_id.clone();
        use loom_protocol::LiveInteractionCapability::*;
        message.session.interaction_capabilities = vec![
            PointerMove,
            PointerButton,
            Wheel,
            Keyboard,
            Cancel,
            DoubleClick,
        ];
        sessions.create(&device_id, start).unwrap();
        sessions
            .set_media_connected("live-1", &device_id, LiveDeviceRole::Source, true)
            .unwrap();
        sessions
            .publish_frame("live-1", &device_id, encoded_live_frame(1))
            .unwrap();
        let mut data: Value = serde_json::from_str(include_str!(
            "../../../../protocol/fixtures/wall-geometry.v1.json"
        ))
        .unwrap();
        for index in 0..2 {
            data["endpoints"][index]["deviceId"] = json!(device_id);
            data["endpoints"][index]["outputId"] = json!(format!("output-{index}"));
            data["endpoints"][index]["renderModes"] = json!(["raw_bgra"]);
            data["endpoints"][index]["inputCapabilities"] = json!(["pointer", "wheel", "keyboard"]);
            if identify {
                data["endpoints"][index]["display"] =
                    json!({"name":format!("Display {}", index + 1),"canIdentify":true});
            }
            assert_eq!(
                device(
                    port,
                    &token,
                    "POST",
                    "/v1/walls/endpoints/register",
                    Some(json!({"baseRevision": index, "endpoint": data["endpoints"][index]}))
                )
                .0,
                200
            );
        }
        data["layout"]["revision"] = json!(3);
        assert_eq!(
            admin(
                port,
                "PUT",
                "/v1/walls/layouts",
                Some(json!({"baseRevision": 2, "layout": data["layout"]}))
            )
            .0,
            200
        );
        let bindings = ["endpoint-left", "endpoint-right"].map(|endpoint| {
            let (status, lease) = device(
                port,
                &token,
                "POST",
                "/v1/walls/connect",
                Some(json!({"endpointId": endpoint})),
            );
            assert_eq!(status, 200);
            assert_eq!(
                device(
                    port,
                    &token,
                    "POST",
                    "/v1/walls/heartbeat",
                    Some(json!({"endpointId": endpoint,
                "leaseId": lease["leaseId"], "sequence": 1, "appliedRevision": 3}))
                )
                .0,
                200
            );
            json!({"endpointId": endpoint, "leaseId": lease["leaseId"], "revision": 3})
        });
        Self {
            server,
            port,
            sessions,
            devices,
            device_id,
            token,
            bindings,
            layout: data["layout"].clone(),
            _root: root,
        }
    }

    fn post(&self, path: &str, body: Value) -> (u16, Value) {
        device(self.port, &self.token, "POST", path, Some(body))
    }
    fn acquire_body(&self, index: usize) -> Value {
        json!({"binding": self.bindings[index], "pixel": {"x": 0, "y": 0}, "pointerId": 1})
    }
    fn acquire(&self, index: usize) -> Value {
        let (status, value) = self.post("/v1/walls/control/acquire", self.acquire_body(index));
        assert_eq!(status, 200, "{value}");
        json!({"binding": self.bindings[index], "controlId": value["controlId"]})
    }
    fn input(&self, control: &Value, sequence: u64, event: Value) -> (u16, Value) {
        self.post(
            "/v1/walls/input",
            json!({"control": control, "sequence": sequence, "event": event}),
        )
    }
    fn button(state: &str) -> Value {
        json!({"kind": "button", "pixel": {"x": 0, "y": 0}, "pointerId": 1, "button": "left", "state": state, "clickCount": 1})
    }
}

#[test]
fn wall_input_endpoint_authority_orders_and_maps_same_device_outputs() {
    let mut f = WallInputFixture::new();
    assert_eq!(
        admin(
            f.port,
            "POST",
            "/v1/walls/control/acquire",
            Some(f.acquire_body(0))
        )
        .0,
        401
    );
    let (_, outsider) = pair(f.port, "Unassigned input device");
    assert_eq!(
        device(
            f.port,
            &outsider,
            "POST",
            "/v1/walls/control/acquire",
            Some(f.acquire_body(0))
        )
        .0,
        403
    );
    let control = f.acquire(0);
    assert_eq!(
        f.post("/v1/walls/control/acquire", f.acquire_body(1)).0,
        409
    );
    let events = [
        WallInputFixture::button("pressed"),
        json!({"kind": "move", "pixel": {"x": 99, "y": 99}, "pointerId": 1}),
        json!({"kind": "wheel", "pixel": {"x": 0, "y": 0}, "deltaX": 0, "deltaY": -120}),
        json!({"kind": "key", "virtualKey": 65, "state": "pressed"}),
        json!({"kind": "key", "virtualKey": 65, "state": "released"}),
        WallInputFixture::button("released"),
    ];
    for (index, event) in events.into_iter().enumerate() {
        assert_eq!(f.input(&control, index as u64 + 1, event).0, 200);
    }
    let (_, events) = f.sessions.events_after("live-1", 0).unwrap();
    let inputs: Vec<_> = events
        .iter()
        .filter_map(|event| match &event.message {
            LiveControlMessage::InputEvent(input) => Some(input),
            _ => None,
        })
        .collect();
    assert_eq!(inputs.len(), 6);
    assert!(inputs
        .iter()
        .all(|input| input.source_device_id == f.device_id));
    let loom_protocol::LiveInputKind::MouseButton(point) = &inputs[0].kind else {
        panic!("button")
    };
    assert!((point.x - 0.102).abs() < 1e-12 && (point.y - 0.203).abs() < 1e-12);
    let record = f.sessions.get("live-1").unwrap();
    assert!(record.session.viewer_devices.is_empty());
    assert_eq!(
        record.session.controller_device.as_deref(),
        Some(f.device_id.as_str())
    );
    assert_eq!(
        f.sessions.state.lock().unwrap()["live-1"].inbound_sequences[&f.device_id].1,
        1
    );
    assert_eq!(f.post("/v1/walls/control/release", control.clone()).0, 200);
    let second = f.acquire(1);
    assert_eq!(
        f.input(&control, 7, WallInputFixture::button("pressed")).0,
        409
    );
    assert_eq!(
        f.input(&second, 1, WallInputFixture::button("pressed")).0,
        200
    );
    let (_, events) = f.sessions.events_after("live-1", 0).unwrap();
    let LiveControlMessage::InputEvent(input) = &events.last().unwrap().message else {
        panic!("input")
    };
    let loom_protocol::LiveInputKind::MouseButton(point) = &input.kind else {
        panic!("button")
    };
    assert!((point.x - 0.899).abs() < 1e-12 && (point.y - 0.203).abs() < 1e-12);
    assert_eq!(
        f.input(&second, 1, WallInputFixture::button("pressed")).0,
        409
    );
    assert!(f
        .sessions
        .get("live-1")
        .unwrap()
        .session
        .controller_device
        .is_none());
    f.server.finish().unwrap();
}

#[test]
fn wall_input_layout_revocation_and_device_revoke_release_without_terminal_cooperation() {
    let mut f = WallInputFixture::new();
    let control = f.acquire(0);
    assert_eq!(
        f.input(&control, 1, WallInputFixture::button("pressed")).0,
        200
    );
    f.layout["revision"] = json!(4);
    assert_eq!(
        admin(
            f.port,
            "PUT",
            "/v1/walls/layouts",
            Some(json!({"baseRevision": 3, "layout": f.layout}))
        )
        .0,
        200
    );
    assert!(f
        .sessions
        .get("live-1")
        .unwrap()
        .session
        .controller_device
        .is_none());
    assert_eq!(
        f.input(&control, 2, WallInputFixture::button("released")).0,
        409
    );
    f.bindings[0]["revision"] = json!(4);
    assert_eq!(
        f.post("/v1/walls/control/acquire", f.acquire_body(0)).0,
        409
    );
    assert_eq!(
        f.post(
            "/v1/walls/heartbeat",
            json!({"endpointId": "endpoint-left", "leaseId": f.bindings[0]["leaseId"],
        "sequence": 2, "appliedRevision": 4})
        )
        .0,
        200
    );
    let current = f.acquire(0);
    assert_eq!(
        f.input(
            &current,
            1,
            json!({"kind": "key", "virtualKey": 17, "state": "pressed"})
        )
        .0,
        200
    );
    f.devices
        .lock()
        .unwrap()
        .revoke_device_sessions(&f.device_id);
    let deadline = Instant::now() + Duration::from_secs(2);
    while f
        .sessions
        .get("live-1")
        .unwrap()
        .session
        .controller_device
        .is_some()
        && Instant::now() < deadline
    {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(f
        .sessions
        .get("live-1")
        .unwrap()
        .session
        .controller_device
        .is_none());
    f.server.finish().unwrap();
}

#[test]
fn wall_input_invalid_edges_disconnect_and_expiry_fail_closed() {
    let mut f = WallInputFixture::new();
    let mut invalid = f.acquire_body(0);
    invalid["extra"] = json!(true);
    assert_eq!(f.post("/v1/walls/control/acquire", invalid).0, 400);
    let mut invalid = f.acquire_body(0);
    invalid["pixel"]["x"] = json!(0.5);
    assert_eq!(f.post("/v1/walls/control/acquire", invalid).0, 400);
    for event in [
        json!({"kind": "move", "pixel": {"x": 100, "y": 0}, "pointerId": 1}),
        json!({"kind": "move", "pixel": {"x": 0, "y": 0}, "pointerId": 2}),
        json!({"kind": "wheel", "pixel": {"x": 0, "y": 0}, "deltaX": 120, "deltaY": 120}),
        json!({"kind": "key", "virtualKey": 65, "state": "released"}),
    ] {
        let control = f.acquire(0);
        assert_eq!(f.input(&control, 1, event).0, 400);
        assert!(f
            .sessions
            .get("live-1")
            .unwrap()
            .session
            .controller_device
            .is_none());
    }
    let control = f.acquire(0);
    f.sessions
        .state
        .lock()
        .unwrap()
        .get_mut("live-1")
        .unwrap()
        .wall_controller
        .as_mut()
        .unwrap()
        .deadline = Instant::now();
    assert_eq!(f.post("/v1/walls/control/renew", control).0, 409);
    let control = f.acquire(0);
    f.sessions
        .set_media_connected("live-1", &f.device_id, LiveDeviceRole::Source, false)
        .unwrap();
    assert!(f
        .sessions
        .get("live-1")
        .unwrap()
        .session
        .controller_device
        .is_none());
    assert_eq!(f.post("/v1/walls/control/renew", control).0, 409);
    f.server.finish().unwrap();
}

#[test]
fn wall_input_event_poll_is_woken_by_control_instead_of_blocking_it() {
    let mut f = WallInputFixture::new();
    let (_, events) = f.sessions.events_after("live-1", 0).unwrap();
    let after = events.last().unwrap().sequence;
    let port = f.port;
    let token = f.token.clone();
    let poll = thread::spawn(move || {
        device(
            port,
            &token,
            "GET",
            &format!("/v1/live/sessions/live-1/events?after={after}&timeoutMs=1000"),
            None,
        )
    });
    // Let the real request enter its long-poll before sending the event that must wake it.
    thread::sleep(Duration::from_millis(150));
    let started = Instant::now();
    let control = f.acquire(0);
    let waited = started.elapsed();
    let (status, response) = poll.join().unwrap();
    assert_eq!(f.post("/v1/walls/control/release", control).0, 200);
    f.server.finish().unwrap();
    assert_eq!(status, 200);
    assert!(response["events"].as_array().unwrap().iter().any(|event| {
        event.pointer("/payload/reason").and_then(Value::as_str) == Some("wall_controller_acquired")
    }), "control waited {waited:?}; the poll timed out before it could observe the input authority event");
}
