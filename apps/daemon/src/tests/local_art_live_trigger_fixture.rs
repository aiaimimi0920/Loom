// Uses only fresh packages, devices, policies, and HTTP server created by this test without executing Arts or touching user input.
const ART_TRIGGER_ROUTE: &str = "/v1/art/local/live-trigger";
const ART_TRIGGER_SESSION: &str = "live:art-trigger";
struct TriggerFixture {
    f: Fixture,
    grant: Value,
    scope: Value,
    source: String,
    source_token: String,
}
impl TriggerFixture {
    fn new() -> Self {
        let f = Fixture::new();
        assert_eq!(f.approve(0, true).0, 200);
        let (status, prepared) = f.prepare(f.request.clone());
        assert_eq!(status, 200, "{prepared}");
        let grant = prepared["grant"].clone();
        let (source, source_token) = pair(f.port, "Local trigger source");
        let mut envelope = live_start_envelope(ART_TRIGGER_SESSION);
        if let LiveControlMessage::SessionStart(start) = &mut envelope.message {
            start.session.source_device_id = source.clone();
            start.requested_by_device_id = source.clone();
            start.session.observation_capabilities = vec![LiveObservationCapability::UiaTree];
        }
        let response = device(
            f.port,
            &source_token,
            "POST",
            "/v1/live/sessions",
            Some(json!({"envelope":envelope})),
        );
        assert_eq!(response.0, 201, "{response:?}");
        assert_eq!(
            attach_viewer(&f, viewer_request(&f, grant.clone(), ART_TRIGGER_SESSION)).0,
            200
        );
        // This gate verifies HTTP, conditions, and ownership without claiming that directly seeded media presence is a real WSS joint run.
        f.live_sessions
            .set_media_connected(ART_TRIGGER_SESSION, &f.device, LiveDeviceRole::Viewer, true)
            .unwrap();
        Self {
            f,
            grant,
            scope: json!({"sessionId":ART_TRIGGER_SESSION,"epoch":1,
            "bindingId":"local-art:test","authorizationId":"authorization:1"}),
            source,
            source_token,
        }
    }
    fn body(&self, operation: Value) -> Value {
        json!({"protocolVersion":"loom.art.local.live-trigger.v1","binding":self.f.request["binding"],
            "grant":self.grant,"operation":operation})
    }
    fn send(&self, operation: Value) -> (u16, Value) {
        device(
            self.f.port,
            &self.f.token,
            "POST",
            ART_TRIGGER_ROUTE,
            Some(self.body(operation)),
        )
    }
    fn op(&self, kind: &str) -> Value {
        json!({"kind":kind,"scope":self.scope})
    }
    fn configuration(&self, revision: u64, sequence: u64) -> Value {
        json!({"kind":"configure","configuration":{"scope":self.scope,
            "consent":"automatic_local_callback","sequence":sequence,
            "condition":{"conditionId":"condition:complete","revision":revision,
                "observationId":"uia:progress_bar:buildProgress","operator":"greater_or_equal",
                "operand":{"path":"/value","value":100},"stableForMs":500,
                "risingEdge":true,"rearm":true,"minimumConfidence":"exact"}}})
    }
    fn configure(&self) -> Value {
        let (status, value) = self.send(self.configuration(1, 2));
        assert_eq!(status, 200, "{value}");
        assert_eq!(value["status"], "idle");
        assert_eq!(value["delivery"], Value::Null);
        value
    }
    fn observe_value(
        &self,
        index: u64,
        value: f64,
        confidence: LiveObservationConfidence,
    ) -> Value {
        let mut envelope = phase_seven_observation(ART_TRIGGER_SESSION, index, value, 1000);
        if let LiveControlMessage::Observation(observation) = &mut envelope.message {
            observation.confidence = confidence;
        }
        serde_json::to_value(envelope).unwrap()
    }
    fn publish(&self, envelope: Value) -> (u16, Value) {
        device(
            self.f.port,
            &self.source_token,
            "POST",
            &format!("/v1/live/sessions/{ART_TRIGGER_SESSION}/observations"),
            Some(json!({"envelope":envelope})),
        )
    }
    fn observe(&self, index: u64, value: f64) {
        let reply =
            self.publish(self.observe_value(index, value, LiveObservationConfidence::Exact));
        assert_eq!(reply.0, 202, "{reply:?}");
    }
    fn ready(&self) {
        self.configure();
        self.observe(1, 0.);
        self.observe(2, 100.);
    }
    fn inspect(&self) -> Value {
        let (status, reply) = self.send(self.op("inspect"));
        assert_eq!(status, 200, "{reply}");
        assert_eq!(reply["delivery"], Value::Null);
        reply
    }
    fn ack(&self, id: &Value, kind: &str) -> (u16, Value) {
        self.send(json!({"kind":"ack","scope":self.scope,"deliveryId":id,"receipt":{"kind":kind}}))
    }
}
