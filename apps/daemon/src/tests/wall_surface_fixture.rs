// Real signed pairing and HTTP routes, with an installed declarative package fixture.
struct WallSurfaceFixture {
    server: ConcurrencyTestFixture,
    _root: Root,
    port: u16,
    owner: String,
    token: String,
    instance: String,
    source_attachment: String,
    layout: Value,
    binding: Value,
}

impl WallSurfaceFixture {
    fn new() -> Self {
        Self::with_inputs(json!(["pointer", "keyboard"]))
    }

    fn with_inputs(inputs: Value) -> Self {
        let root = Root::new();
        let daemon = LoomDaemon::bind(
            DaemonConfig::localhost(0)
                .with_control_plane_root(&root.0)
                .with_bounded_request_executor(4, 16),
        )
        .unwrap();
        daemon
            .runtime
            .framework_registry
            .install_framework_package_from_zip(&framework_package_zip("process", "1.0.0"))
            .unwrap();
        let scene = json!({"protocolVersion": "loom.surface.v1", "scene": {"id":"root", "type":"column", "children":[
            {"id":"refresh", "type":"button", "props":{"label":"Update"}, "events":{"click":"refresh_price"}}
        ]}, "authoritativeState":{"value":7}});
        loom_tool_registry::install::install_art_from_zip(
            &wall_confirmation_package(&scene),
            &root.0,
            &daemon.runtime.framework_registry,
            &daemon.runtime.tool_registry,
        )
        .unwrap();
        let port = daemon.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        let server = ConcurrencyTestFixture::new(tx, thread::spawn(move || daemon.serve_until(rx)));
        let (owner, token) = pair(port, "Art tile");
        let (status, created) = admin(
            port,
            "POST",
            "/v1/surfaces/instances",
            Some(json!({"artId":"wall-art"})),
        );
        assert_eq!(status, 201, "{created}");
        let instance = created["descriptor"]["instanceId"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut source_capabilities = default_declarative_surface_host_capabilities();
        source_capabilities
            .capabilities
            .push("remote_resources".into());
        let (status, attached) = admin(
            port,
            "POST",
            &format!("/v1/surfaces/instances/{instance}/attachments"),
            Some(json!({
                "hookNodeId":"source-art", "deviceId":owner, "capabilities":source_capabilities
            })),
        );
        assert_eq!(status, 201, "{attached}");
        let source_attachment = attached["descriptor"]["attachmentId"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            admin(
                port,
                "POST",
                &format!("/v1/surfaces/instances/{instance}/mount"),
                Some(json!({"attachmentId":source_attachment}))
            )
            .0,
            200
        );
        let endpoint = json!({"protocolVersion":"loom.wall.v1", "endpointId":"art-tile", "deviceId":owner,
            "outputId":"output-1", "pixelSize":{"width":800,"height":600}, "renderModes":["surface_v1","image"],
            "inputCapabilities":inputs});
        assert_eq!(
            device(
                port,
                &token,
                "POST",
                "/v1/walls/endpoints/register",
                Some(json!({"baseRevision":0,"endpoint":endpoint}))
            )
            .0,
            200
        );
        let layout = json!({"protocolVersion":"loom.wall.v1", "wallId":"art-wall", "revision":2,
            "bounds":{"x":0,"y":0,"width":800,"height":600},
            "tiles":[{"tileId":"tile-1","endpointId":"art-tile","rect":{"x":0,"y":0,"width":800,"height":600},"rotation":"deg0"}],
            "placements":[{"placementId":"art","source":{"kind":"surface","id":instance},
                "rect":{"x":0,"y":0,"width":800,"height":600},"sourceCrop":{"x":0,"y":0,"width":1,"height":1},"zIndex":0,"interactive":true}]});
        assert_eq!(
            admin(
                port,
                "PUT",
                "/v1/walls/layouts",
                Some(json!({"baseRevision":1,"layout":layout}))
            )
            .0,
            200
        );
        let (status, lease) = device(
            port,
            &token,
            "POST",
            "/v1/walls/connect",
            Some(json!({"endpointId":"art-tile"})),
        );
        assert_eq!(status, 200, "{lease}");
        Self {
            server,
            _root: root,
            port,
            owner,
            token,
            instance,
            source_attachment,
            layout,
            binding: json!({"endpointId":"art-tile","leaseId":lease["leaseId"],"revision":2}),
        }
    }

    fn call(&self, operation: &str, body: Value) -> (u16, Value) {
        device(
            self.port,
            &self.token,
            "POST",
            &format!("/v1/walls/surfaces/{operation}"),
            Some(body),
        )
    }

    fn open(&self) -> Value {
        let (status, state) = self.call(
            "open",
            json!({"binding":self.binding,"instanceId":self.instance}),
        );
        assert_eq!(status, 200, "{state}");
        state
    }

    fn acknowledge(&self, sequence: u64) {
        assert_eq!(device(self.port, &self.token, "POST", "/v1/walls/heartbeat", Some(json!({
            "endpointId":"art-tile","leaseId":self.binding["leaseId"],"sequence":sequence,"appliedRevision":self.binding["revision"]
        }))).0, 200);
    }

    fn source(&self) -> Value {
        admin(
            self.port,
            "GET",
            &format!("/v1/surfaces/instances/{}", self.instance),
            None,
        )
        .1
    }

    fn event(&self, state: &Value, sequence: u64) -> Value {
        json!({"view":state["view"], "placementId":"art", "pixel":{"x":100,"y":100}, "sequence":sequence,
            "event":{"protocolVersion":"loom.surface.v1","instanceId":self.instance,"attachmentId":state["view"]["attachmentId"],
                "eventId":format!("wall-event-{sequence}"),"nodeId":"refresh","event":"click","action":"refresh_price",
                "class":"discrete","generation":state["generation"],"baseRevision":state["snapshot"]["revision"],"payload":{}}})
    }
}

impl Drop for WallSurfaceFixture {
    fn drop(&mut self) {
        self.server.finish().expect("stop wall Surface daemon");
    }
}

fn wall_confirmation_package(scene: &Value) -> Vec<u8> {
    let bytes = surface_art_package_zip("wall-art", "1.0.0", scene, "independent");
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut output));
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if entry.name() == "manifest.json" {
                let mut manifest: Value = serde_json::from_slice(&bytes).unwrap();
                manifest["metadata"]["capabilities"]["surface"]["actions"][0]["confirmation"] =
                    json!(true);
                manifest["metadata"]["capabilities"]["surface"]["requiredCapabilities"] =
                    json!(["remote_resources"]);
                bytes = serde_json::to_vec(&manifest).unwrap();
            }
            writer
                .start_file(entry.name(), zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer.finish().unwrap();
    }
    output
}
