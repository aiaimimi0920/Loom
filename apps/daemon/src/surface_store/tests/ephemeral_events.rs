struct EphemeralEventFixture {
    path: PathBuf,
    store: SurfaceInstanceStore,
    instance: String,
    source: String,
    wall: String,
}

impl EphemeralEventFixture {
    fn new() -> Self {
        let path = temp_path("ephemeral-events");
        let mut store = SurfaceInstanceStore::new(&path).unwrap();
        let record = create(&mut store);
        let instance = record.descriptor.instance_id.clone();
        let source = store.attach(&instance, "source", "device", None).unwrap();
        let source = source.descriptor.attachment_id;
        store
            .put_snapshot(&instance, snapshot(&record, &source))
            .unwrap();
        let wall = store
            .attach_ephemeral(&instance, "tile", "device", ephemeral_host())
            .unwrap()
            .descriptor
            .attachment_id;
        store
            .put_snapshot(&instance, snapshot(&record, &wall))
            .unwrap();
        Self {
            path,
            store,
            instance,
            source,
            wall,
        }
    }

    fn event(&self, id: &str, class: SurfaceEventClass) -> SurfaceEvent {
        SurfaceEvent {
            protocol_version: SURFACE_PROTOCOL_VERSION.into(),
            instance_id: self.instance.clone(),
            attachment_id: self.wall.clone(),
            event_id: id.into(),
            node_id: "price".into(),
            event: "click".into(),
            action: Some("refresh".into()),
            class,
            generation: 0,
            base_revision: 1,
            payload: serde_json::json!({"value": "transient input"}),
        }
    }

    fn finish(&mut self, ack: SurfaceActionAck) {
        self.store
            .update_event_ack(
                SurfaceActionAck {
                    status: SurfaceActionStatus::Succeeded,
                    ..ack
                },
                true,
            )
            .unwrap();
    }
}

impl Drop for EphemeralEventFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.path.parent().unwrap());
    }
}

#[test]
fn ephemeral_surface_continuous_admission_is_visible_without_retaining_payloads() {
    let mut fixture = EphemeralEventFixture::new();
    let event = fixture.event("wall-continuous", SurfaceEventClass::Continuous);
    let ack = fixture
        .store
        .accept_event(&fixture.instance, event)
        .unwrap();
    assert_eq!(
        fixture.store.event_ack(&fixture.instance, &ack.event_id),
        Some(ack.clone())
    );
    assert!(fixture
        .store
        .get(&fixture.instance)
        .unwrap()
        .pending_events
        .is_empty());
    assert!(fixture
        .store
        .remove_ephemeral_attachment(&fixture.instance, &fixture.wall)
        .is_err());
    let reloaded = SurfaceInstanceStore::new(&fixture.path).unwrap();
    assert!(reloaded
        .get(&fixture.instance)
        .unwrap()
        .event_acks
        .is_empty());
    fixture.finish(ack);
    fixture
        .store
        .remove_ephemeral_attachment(&fixture.instance, &fixture.wall)
        .unwrap();
    assert!(fixture
        .store
        .get(&fixture.instance)
        .unwrap()
        .event_acks
        .is_empty());
}

#[test]
fn ephemeral_surface_completed_ack_history_is_bounded_and_excluded_from_persistence() {
    for class in [SurfaceEventClass::Continuous, SurfaceEventClass::Discrete] {
        let mut fixture = EphemeralEventFixture::new();
        let mut ordinary = fixture.event("ordinary-source", SurfaceEventClass::Discrete);
        ordinary.attachment_id = fixture.source.clone();
        let ack = fixture
            .store
            .accept_event(&fixture.instance, ordinary)
            .unwrap();
        fixture.finish(ack);
        let source_ack = fixture
            .store
            .event_ack(&fixture.instance, "ordinary-source")
            .unwrap();
        for index in 0..192 {
            let event = fixture.event(&format!("wall-{index}"), class.clone());
            let ack = fixture
                .store
                .accept_event(&fixture.instance, event)
                .unwrap();
            fixture.finish(ack);
            assert!(
                fixture
                    .store
                    .get(&fixture.instance)
                    .unwrap()
                    .event_acks
                    .len()
                    <= 65
            );
        }
        let reloaded = SurfaceInstanceStore::new(&fixture.path).unwrap();
        assert_eq!(
            reloaded.get(&fixture.instance).unwrap().event_acks,
            BTreeMap::from([("ordinary-source".into(), source_ack.clone())])
        );
        fixture
            .store
            .remove_ephemeral_attachment(&fixture.instance, &fixture.wall)
            .unwrap();
        let current = fixture.store.get(&fixture.instance).unwrap();
        assert_eq!(
            current.event_acks,
            BTreeMap::from([("ordinary-source".into(), source_ack)])
        );
        assert!(current.attachments.contains_key(&fixture.source));
        assert_eq!(
            current.authoritative_state,
            serde_json::json!({"price":100})
        );
    }
}

#[test]
fn ephemeral_surface_ack_capacity_never_retires_unfinished_work() {
    let mut fixture = EphemeralEventFixture::new();
    for index in 0..64 {
        let event = fixture.event(&format!("pending-{index}"), SurfaceEventClass::Continuous);
        fixture
            .store
            .accept_event(&fixture.instance, event)
            .unwrap();
    }
    let overflow = fixture.event("overflow", SurfaceEventClass::Continuous);
    assert!(fixture
        .store
        .accept_event(&fixture.instance, overflow)
        .is_err());
    let current = fixture.store.get(&fixture.instance).unwrap();
    assert_eq!(current.event_acks.len(), 64);
    assert!(current
        .event_acks
        .values()
        .all(|ack| ack.status == SurfaceActionStatus::Queued));
    let other = fixture
        .store
        .attach_ephemeral(&fixture.instance, "other-tile", "device", ephemeral_host())
        .unwrap();
    let other = other.descriptor.attachment_id;
    fixture
        .store
        .put_snapshot(&fixture.instance, snapshot(&current, &other))
        .unwrap();
    let mut event = fixture.event("other-view", SurfaceEventClass::Continuous);
    event.attachment_id = other;
    fixture
        .store
        .accept_event(&fixture.instance, event)
        .unwrap();
    assert_eq!(
        fixture
            .store
            .get(&fixture.instance)
            .unwrap()
            .event_acks
            .len(),
        65
    );
}

#[test]
fn ephemeral_surface_confirmation_acks_are_transient_before_and_after_rejection() {
    let mut fixture = EphemeralEventFixture::new();
    let event = fixture.event("confirmation", SurfaceEventClass::Discrete);
    let (_, confirmation) = fixture
        .store
        .await_confirmation(&fixture.instance, event, SurfaceActionRisk::High)
        .unwrap();
    assert!(SurfaceInstanceStore::new(&fixture.path)
        .unwrap()
        .get(&fixture.instance)
        .unwrap()
        .event_acks
        .is_empty());
    fixture
        .store
        .resolve_confirmation(SurfaceConfirmationDecision {
            protocol_version: SURFACE_PROTOCOL_VERSION.into(),
            confirmation_id: confirmation.confirmation_id,
            instance_id: fixture.instance.clone(),
            attachment_id: fixture.wall.clone(),
            device_id: "device".into(),
            approved: false,
        })
        .unwrap();
    assert!(SurfaceInstanceStore::new(&fixture.path)
        .unwrap()
        .get(&fixture.instance)
        .unwrap()
        .event_acks
        .is_empty());
    fixture
        .store
        .remove_ephemeral_attachment(&fixture.instance, &fixture.wall)
        .unwrap();
    assert!(fixture
        .store
        .get(&fixture.instance)
        .unwrap()
        .event_acks
        .is_empty());
}
