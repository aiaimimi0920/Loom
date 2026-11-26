//! Explicit confirmation protocol never widens legacy v1 fields; unknown/forged fields, invalid terminal states, and over-budget payloads are rejected.
use super::*;
use serde_json::json;
fn challenge() -> Challenge {
    Challenge {
        nonce: "challenge:one".into(),
        fingerprint: "a".repeat(64),
        plugin_id: "publisher/plugin".into(),
        command_id: "publisher/plugin.run".into(),
        package_digest: "b".repeat(64),
        scope_id: "scope:one".into(),
        expires_at_ms: 1234,
    }
}
#[test]
fn confirmation_challenge_is_typed_bounded_and_not_a_progress_approval() {
    let valid = challenge();
    valid.validate().unwrap();
    let mut bad = valid.clone();
    bad.scope_id = "x\n批准".into();
    assert!(bad.validate().is_err());
    bad = valid.clone();
    bad.fingerprint = "A".repeat(64);
    assert!(bad.validate().is_err());
    bad = valid.clone();
    bad.expires_at_ms = 0;
    assert!(bad.validate().is_err());
    let mut value = serde_json::to_value(valid).unwrap();
    value["approved"] = json!(true);
    assert!(serde_json::from_value::<Challenge>(value).is_err());
    assert!(RequestControl::parse(&vec![b'x'; super::super::MAX_REQUEST + 1], true).is_err());
    assert!(Request::parse(&vec![b'x'; images::MAX_REQUEST + 1]).is_err());
}
