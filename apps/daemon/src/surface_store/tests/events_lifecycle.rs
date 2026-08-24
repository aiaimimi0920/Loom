// Discrete/continuous event retention and attachment lifecycle contract coverage.
#[test]
fn discrete_events_are_validated_deduplicated_and_persisted() {
    let path = temp_path("events");
    let mut store = SurfaceInstanceStore::new(&path).expect("open store");
    let record = create(&mut store);
    let instance_id = record.descriptor.instance_id.clone();
    let attachment = store
        .attach(&instance_id, "hook-node:1", "device:1", None)
        .expect("attach");
    store
        .put_snapshot(
            &instance_id,
            snapshot(&record, &attachment.descriptor.attachment_id),
        )
        .expect("put snapshot");
    let event = SurfaceEvent {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: instance_id.clone(),
        attachment_id: attachment.descriptor.attachment_id,
        event_id: "event:refresh-1".to_owned(),
        node_id: "price".to_owned(),
        event: "click".to_owned(),
        action: Some("refresh".to_owned()),
        class: SurfaceEventClass::Discrete,
        generation: 0,
        base_revision: 1,
        payload: Value::Null,
    };
    let first = store
        .accept_event(&instance_id, event.clone())
        .expect("accept event");
    let duplicate = store
        .accept_event(&instance_id, event)
        .expect("deduplicate event");
    assert_eq!(duplicate, first);
    let running = SurfaceActionAck {
        status: SurfaceActionStatus::Running,
        ..first
    };
    store
        .update_event_ack(running, false)
        .expect("mark action running");
    let succeeded = SurfaceActionAck {
        status: SurfaceActionStatus::Succeeded,
        ..store
            .event_ack(&instance_id, "event:refresh-1")
            .expect("running ack")
    };
    store
        .update_event_ack(succeeded, true)
        .expect("complete action");
    let reloaded = SurfaceInstanceStore::new(&path).expect("reload store");
    let record = reloaded.get(&instance_id).expect("record");
    assert!(record.pending_events.is_empty());
    assert_eq!(record.event_acks.len(), 1);
    assert_eq!(
        record.event_acks["event:refresh-1"].status,
        SurfaceActionStatus::Succeeded
    );
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}

#[test]
fn continuous_events_do_not_enter_queues_or_persisted_state() {
    let path = temp_path("continuous-events");
    let mut store = SurfaceInstanceStore::new(&path).expect("open store");
    let record = create(&mut store);
    let instance_id = record.descriptor.instance_id.clone();
    let attachment = store
        .attach(
            &instance_id,
            "hook-node:continuous",
            "device:continuous",
            None,
        )
        .expect("attach");
    store
        .put_snapshot(
            &instance_id,
            snapshot(&record, &attachment.descriptor.attachment_id),
        )
        .expect("put snapshot");
    let persisted_before = fs::read(&path).expect("read persistent projection");
    let event = SurfaceEvent {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: instance_id.clone(),
        attachment_id: attachment.descriptor.attachment_id,
        event_id: "event:continuous-1".to_owned(),
        node_id: "price".to_owned(),
        event: "click".to_owned(),
        action: Some("refresh".to_owned()),
        class: SurfaceEventClass::Continuous,
        generation: 0,
        base_revision: 1,
        payload: serde_json::json!({"position": 1}),
    };

    let first = store
        .accept_event(&instance_id, event.clone())
        .expect("accept continuous event");
    let duplicate = store
        .accept_event(&instance_id, event.clone())
        .expect("accept repeated continuous event");
    assert!(first.accepted);
    assert_eq!(first.status, SurfaceActionStatus::Queued);
    assert_eq!(duplicate, first);

    // Cross the discrete queue limit with distinct keys to prove continuous traffic is not
    // accidentally retained by either of the persisted event collections.
    for index in 0..=MAX_PENDING_SURFACE_EVENTS {
        let mut next = event.clone();
        next.event_id = format!("event:continuous-burst-{index}");
        next.payload = serde_json::json!({"position": index});
        let ack = store
            .accept_event(&instance_id, next)
            .expect("accept distinct continuous event");
        assert_eq!(ack.status, SurfaceActionStatus::Queued);
    }
    assert!(store
        .event_ack(&instance_id, "event:continuous-1")
        .is_none());
    assert!(store.pending_events().is_empty());
    let live_record = store.get(&instance_id).expect("live record");
    assert!(live_record.event_acks.is_empty());
    assert_eq!(
        fs::read(&path).expect("reread persistent projection"),
        persisted_before
    );

    let reloaded = SurfaceInstanceStore::new(&path).expect("reload store");
    let record = reloaded.get(&instance_id).expect("record");
    assert!(record.pending_events.is_empty());
    assert!(record.event_acks.is_empty());
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}

#[test]
fn lifecycle_is_ordered_idempotent_and_dispose_releases_attachment_state() {
    let path = temp_path("lifecycle");
    let mut store = SurfaceInstanceStore::new(&path).expect("open store");
    let record = create(&mut store);
    let instance_id = record.descriptor.instance_id.clone();
    let attachment = store
        .attach(&instance_id, "hook-node:1", "device:1", None)
        .expect("attach");
    let attachment_id = attachment.descriptor.attachment_id.clone();
    store
        .put_snapshot(&instance_id, snapshot(&record, &attachment_id))
        .expect("mount");
    let active = SurfaceLifecycleEvent {
        protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
        instance_id: instance_id.clone(),
        attachment_id: attachment_id.clone(),
        state: SurfaceLifecycleState::Active,
        revision: 2,
    };
    assert_eq!(
        store
            .transition_lifecycle(&instance_id, active.clone())
            .expect("activate")
            .lifecycle,
        SurfaceLifecycleState::Active
    );
    store
        .transition_lifecycle(&instance_id, active)
        .expect("idempotent replay");
    assert!(store
        .transition_lifecycle(
            &instance_id,
            SurfaceLifecycleEvent {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.clone(),
                attachment_id: attachment_id.clone(),
                state: SurfaceLifecycleState::Suspended,
                revision: 4,
            },
        )
        .is_err());
    let disposed = store
        .transition_lifecycle(
            &instance_id,
            SurfaceLifecycleEvent {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.clone(),
                attachment_id,
                state: SurfaceLifecycleState::Disposed,
                revision: 3,
            },
        )
        .expect("dispose");
    assert_eq!(disposed.lifecycle, SurfaceLifecycleState::Disposed);
    assert!(disposed.snapshot.is_none());
    assert!(disposed.host_capabilities.is_none());
    let _ = fs::remove_dir_all(path.parent().expect("parent"));
}
