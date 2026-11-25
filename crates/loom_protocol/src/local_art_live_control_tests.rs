use super::*;
use serde_json::json;
fn fixture(kind: &str) -> (Grant, Request) {
    let grant: Grant = serde_json::from_value(super::super::tests::value()).unwrap();
    let request =
        serde_json::from_value(json!({"protocolVersion":PROTOCOL,"binding":grant.binding,
        "grant":{"algorithm":"ed25519","keyId":"fixture","payload":"e30=","signature":"fixture"},
        "operation":{"kind":kind,"sessionId":"live:1","epoch":1,"sequence":2}}))
        .unwrap();
    (grant, request)
}
#[test]
fn native_control_is_explicit_exact_and_release_only_reduces_after_expiry() {
    let (grant, acquire) = fixture("acquire");
    assert_eq!(acquire.validate(&grant, "device:1", 1000), Ok(()));
    assert_eq!(
        acquire.validate(&grant, "other", 1000),
        Err(Error::Identity)
    );
    for time in [999, 10000] {
        assert_eq!(acquire.validate(&grant, "device:1", time), Err(Error::Time));
    }
    let (_, release) = fixture("release");
    assert!(release.reducing());
    assert_eq!(release.validate(&grant, "device:1", 10000), Ok(()));
    let mut changed = acquire.clone();
    changed.binding.generation += 1;
    assert_eq!(
        changed.validate(&grant, "device:1", 1000),
        Err(Error::Identity)
    );
    let mut limited = grant;
    limited.permissions = vec![super::super::Permission::LocalState];
    assert_eq!(
        acquire.validate(&limited, "device:1", 1000),
        Err(Error::Permission)
    );
}
#[test]
fn native_control_input_requires_original_input_identity_and_is_never_reduction() {
    let (grant, mut input) = fixture("acquire");
    input.operation = Operation::Input {envelope:serde_json::from_value(json!({
        "protocolVersion":"loom.live.v1","sessionId":"live:1","epoch":1,"sequence":3,"messageType":"input_event",
        "payload":{"inputSequence":1,"issuedAtMs":1000,"sourceDeviceId":"device:1","kind":{"kind":"key","data":{"code":"vk:65","state":"pressed","modifiers":0}}}
    })).unwrap()};
    assert!(!input.reducing());
    assert_eq!(input.validate(&grant, "device:1", 1000), Ok(()));
    assert_eq!(input.validate(&grant, "device:1", 10000), Err(Error::Time));
    if let Operation::Input { envelope } = &mut input.operation {
        if let LiveControlMessage::InputEvent(event) = &mut envelope.message {
            event.source_device_id = "other".into();
        }
    }
    assert_eq!(
        input.validate(&grant, "device:1", 1000),
        Err(Error::Identity)
    );
}
#[test]
fn native_control_wire_rejects_surface_alias_revoke_unknown_and_oversize() {
    let (_, request) = fixture("acquire");
    let value = serde_json::to_value(request).unwrap();
    assert!(Request::parse(&serde_json::to_vec(&value).unwrap()).is_ok());
    for (pointer, bad) in [
        ("/operation/kind", json!("revoke")),
        ("/protocolVersion", json!("loom.art.local.live-viewer.v1")),
    ] {
        let mut changed = value.clone();
        *changed.pointer_mut(pointer).unwrap() = bad;
        assert!(Request::parse(&serde_json::to_vec(&changed).unwrap()).is_err());
    }
    let mut changed = value;
    changed["surfaceInstanceId"] = json!("not-a-proof");
    assert!(Request::parse(&serde_json::to_vec(&changed).unwrap()).is_err());
    assert_eq!(
        Request::parse(&vec![b' '; MAX_REQUEST + 1]).err(),
        Some(Error::Limit)
    );
}
