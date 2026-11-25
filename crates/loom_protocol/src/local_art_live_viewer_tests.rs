use super::*;
use serde_json::json;
fn fixture() -> (Grant, Attach) {
    let grant: Grant = serde_json::from_value(super::super::tests::value()).unwrap();
    let request = Attach {
        protocol_version: PROTOCOL.into(), binding: grant.binding.clone(),
        grant: Envelope { algorithm: "ed25519".into(), key_id: "fixture".into(), payload: "e30=".into(), signature: "fixture".into() },
        envelope: serde_json::from_value(json!({"protocolVersion":"loom.live.v1","sessionId":"live:1","epoch":1,"sequence":1,
            "messageType":"session_ack","payload":{"accepted":true,"responderDeviceId":"device:1"}})).unwrap(),
        require_existing_membership: false,
    };
    (grant, request)
}
#[test]
fn local_art_viewer_validates_exact_host_actor_time_and_read_only_message() {
    let (grant, request) = fixture();
    assert_eq!(request.validate(&grant, "device:1", 1000), Ok(()));
    assert_eq!(
        request.validate(&grant, "other", 1000),
        Err(Error::Identity)
    );
    for now in [999, 10000] {
        assert_eq!(request.validate(&grant, "device:1", now), Err(Error::Time));
    }
    let mut changed = request.clone();
    changed.binding.generation += 1;
    assert_eq!(
        changed.validate(&grant, "device:1", 1000),
        Err(Error::Identity)
    );
    let mut without_preview = grant.clone();
    without_preview.permissions = vec![Permission::LocalState];
    assert_eq!(
        request.validate(&without_preview, "device:1", 1000),
        Err(Error::Permission)
    );
    let mut changed = request.clone();
    changed.envelope.sequence = 0;
    assert!(changed.validate(&grant, "device:1", 1000).is_err());
    let mut rejected = request;
    if let LiveControlMessage::SessionAck(ack) = &mut rejected.envelope.message {
        ack.accepted = false;
    }
    assert_eq!(
        rejected.validate(&grant, "device:1", 1000),
        Err(Error::Identity)
    );
}
#[test]
fn local_art_viewer_wire_is_bounded_strict_and_separate_from_surface() {
    let (_, request) = fixture();
    let mut value = serde_json::to_value(request).unwrap();
    assert!(Attach::parse(&serde_json::to_vec(&value).unwrap()).is_ok());
    value["surfaceInstanceId"] = json!("not-a-native-proof");
    assert!(Attach::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    assert_eq!(
        Attach::parse(&vec![b' '; MAX_REQUEST + 1]).err(),
        Some(Error::Limit)
    );
    assert!(Attach::parse(
        br#"{"protocolVersion":"loom.art.local.live-viewer.v1","protocolVersion":"other"}"#
    )
    .is_err());
    value.as_object_mut().unwrap().remove("surfaceInstanceId");
    value["grant"]["payload"] = json!("x".repeat(MAX_ENVELOPE));
    assert_eq!(
        Attach::parse(&serde_json::to_vec(&value).unwrap()).err(),
        Some(Error::Limit)
    );
}
