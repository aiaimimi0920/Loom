//! Exact ownership, permission, and terminal-state invariants; signature verification and live device revocation are covered separately by server route tests.
use super::*;
use serde_json::json;
fn grant() -> Grant {
    serde_json::from_value(json!({"protocolVersion":super::super::PROTOCOL,"grantId":"grant:1",
        "binding":{"origin":"https://loom.example","requestId":"request:1","deviceId":"device:1",
        "workspaceId":"workspace:1","instanceId":"instance:1","generation":1,"artId":"vendor/demo",
        "artVersion":"1.0.0","packageDigest":"a".repeat(64),"sdkVersion":super::super::SDK},
        "executionOwner":"otter","resultOwner":"otter_workspace","permissions":["local_state","preview","loom_service"],
        "services":["service:ocr"],"issuedAtMs":1000,"expiresAtMs":5000,"offlineUntilMs":0,
        "source":{"resourceId":format!("sha256:{}","b".repeat(64)),"digest":"b".repeat(64),"size":10}})).unwrap()
}
fn owner(g: &Grant) -> Owner {
    Owner {
        binding: g.binding.clone(),
        grant_id: g.grant_id.clone(),
        call_id: "call:1".into(),
        service: "service:ocr".into(),
    }
}
#[test]
fn service_owner_requires_exact_instance_scope_actor_and_live_grant() {
    let g = grant();
    let original = owner(&g);
    assert_eq!(original.validate(&g, "device:1", 1000), Ok(()));
    for time in [999, 5000] {
        assert_eq!(original.validate(&g, "device:1", time), Err(Error::Time));
    }
    assert_eq!(
        original.validate(&g, "device:other", 2000),
        Err(Error::Identity)
    );
    let mut changed = original.clone();
    changed.binding.generation += 1;
    assert_eq!(changed.validate(&g, "device:1", 2000), Err(Error::Identity));
    changed = original.clone();
    changed.grant_id = "grant:other".into();
    assert_eq!(changed.validate(&g, "device:1", 2000), Err(Error::Identity));
    changed = original.clone();
    changed.service = "service:admin".into();
    assert_eq!(
        changed.validate(&g, "device:1", 2000),
        Err(Error::Permission)
    );
    changed = original;
    changed.call_id = "../call".into();
    assert_eq!(changed.validate(&g, "device:1", 2000), Err(Error::Identity));
}
#[test]
fn service_reply_rejects_cross_owner_and_contradictory_final_states() {
    let expected = owner(&grant());
    let mut reply = Reply {
        protocol_version: PROTOCOL.into(),
        owner: expected.clone(),
        sequence: 1,
        status: Status::Accepted,
        output: None,
        error: None,
        progress: None,
    };
    assert_eq!(reply.validate(&expected), Ok(()));
    reply.status = Status::Succeeded;
    assert_eq!(reply.validate(&expected), Err(Error::State));
    reply.output = Some(Value::Null);
    assert_eq!(reply.validate(&expected), Ok(()));
    let roundtrip: Reply = serde_json::from_slice(&serde_json::to_vec(&reply).unwrap()).unwrap();
    assert_eq!(roundtrip.validate(&expected), Ok(()));
    assert_eq!(roundtrip.output, Some(Value::Null));
    reply.error = Some("failed".into());
    assert_eq!(reply.validate(&expected), Err(Error::State));
    reply.output = None;
    reply.status = Status::Failed;
    assert_eq!(reply.validate(&expected), Ok(()));
    let roundtrip: Reply = serde_json::from_slice(&serde_json::to_vec(&reply).unwrap()).unwrap();
    assert_eq!(roundtrip.validate(&expected), Ok(()));
    reply.owner.call_id = "call:other".into();
    assert_eq!(reply.validate(&expected), Err(Error::Identity));
}
#[test]
fn service_input_progress_and_timeout_are_bounded() {
    let g = grant();
    let mut submit = Submit {
        protocol_version: PROTOCOL.into(),
        owner: owner(&g),
        grant: Envelope {
            algorithm: "ed25519".into(),
            key_id: "fixture".into(),
            payload: String::new(),
            signature: String::new(),
        },
        input: json!({}),
        timeout_ms: 1000,
    };
    // Outer signatures are verified at the HTTP route boundary; relational protocol validation is never treated as a trusted grant.
    assert_eq!(submit.validate(&g, "device:1", 1000), Ok(()));
    submit.timeout_ms = 0;
    assert_eq!(submit.validate(&g, "device:1", 1000), Err(Error::Time));
    submit.timeout_ms = 1000;
    submit.input = json!("a".repeat(MAX_INPUT));
    assert_eq!(submit.validate(&g, "device:1", 1000), Err(Error::Limit));
    let reply = Reply {
        protocol_version: PROTOCOL.into(),
        owner: owner(&g),
        sequence: 1,
        status: Status::Running,
        output: None,
        error: None,
        progress: Some(json!("a".repeat(MAX_PROGRESS))),
    };
    assert_eq!(reply.validate(&owner(&g)), Err(Error::Limit));
    let mut wire = serde_json::to_value(submit).unwrap();
    wire["capability"] = json!("admin:any");
    assert!(serde_json::from_value::<Submit>(wire).is_err());
    assert!(matches!(
        Submit::parse(&vec![b' '; MAX_REQUEST + 1]),
        Err(Error::Limit)
    ));
    assert!(matches!(
        Control::parse(&vec![b' '; MAX_REQUEST + 1]),
        Err(Error::Limit)
    ));
}
