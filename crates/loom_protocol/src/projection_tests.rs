use super::*;
use serde_json::{json, Value};

fn invitation() -> ProjectionEnvelope {
    serde_json::from_value(json!({
        "protocol": QR_PROJECTION_PROTOCOL,
        "projectionId": format!("projection:{}", "1".repeat(32)),
        "serverOrigin": "https://loom.example.test:8765",
        "source": {"deviceId": "device-a", "sessionId": "session-a", "unitId": "unit-a", "revision": 1},
        "content": {"kind": "art", "digest": "a".repeat(64)},
        "expiresAtMs": 300001,
        "nonce": "2".repeat(32),
        "signature": {"algorithm": "ed25519", "keyId": "device-a", "value": "A".repeat(86)}
    })).unwrap()
}

#[test]
fn projection_signature_binds_the_server_and_every_content_identity_field() {
    let envelope = invitation();
    assert!(envelope.validate().is_ok());
    let expected = format!(
        "neuro.qr-projection.v1\nprojection:{}\nhttps://loom.example.test:8765\ndevice-a\nsession-a\nunit-a\n1\nart\n{}\n300001\n{}",
        "1".repeat(32), "a".repeat(64), "2".repeat(32),
    );
    assert_eq!(envelope.signature_message(), expected);
    let mut redirected = envelope.clone();
    redirected.server_origin = "https://other.example.test".to_owned();
    assert_ne!(redirected.signature_message(), envelope.signature_message());
}

#[test]
fn projection_schema_and_rust_reject_unknown_fields_and_missing_origin() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../protocol/schemas/qr-projection.v1.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let value = serde_json::to_value(invitation()).unwrap();
    assert!(validator.is_valid(&value));
    for field in ["serverOrigin", "signature", "source", "nonce"] {
        let mut changed = value.clone();
        changed.as_object_mut().unwrap().remove(field);
        assert!(!validator.is_valid(&changed));
        assert!(serde_json::from_value::<ProjectionEnvelope>(changed).is_err());
    }
    let mut changed = value;
    changed["authToken"] = json!("must-never-be-transferred");
    assert!(!validator.is_valid(&changed));
    assert!(serde_json::from_value::<ProjectionEnvelope>(changed).is_err());
}

#[test]
fn projection_origins_are_only_bounded_https_or_literal_loopback_origins() {
    for value in [
        "https://loom.example.test",
        "https://192.168.1.2:4430",
        "https://[2001:db8::1]:8765",
        "http://127.0.0.1:8765",
        "http://[::1]:8765",
        "http://localhost:8765",
    ] {
        assert!(projection_origin_valid(value), "{value}");
    }
    for value in [
        "http://192.168.1.2:8765",
        "http://127.0.0.1.evil.test",
        "http://127.1",
        "https://user:pass@host",
        "https://host/path",
        "https://host?q=1",
        "https://host#x",
        "https://host\\evil",
        "https://host\n",
        "https://",
        "https://host:0",
        "https://host:65536",
        "https://[invalid]",
        "https://-host",
        "https://a..b",
        "file:///tmp/test",
    ] {
        assert!(!projection_origin_valid(value), "{value}");
    }
    assert!(!projection_origin_valid(&format!(
        "https://{}",
        "a".repeat(257)
    )));
}

#[test]
fn projection_revisions_timestamps_and_signature_identifiers_are_bounded() {
    for revision in [0, MAX_PROJECTION_REVISION + 1] {
        let mut envelope = invitation();
        envelope.source.revision = revision;
        assert!(envelope.validate().is_err());
    }
    let mut envelope = invitation();
    envelope.expires_at_ms = MAX_PROJECTION_REVISION + 1;
    assert!(envelope.validate().is_err());
    envelope = invitation();
    envelope.signature.key_id = "other-device".to_owned();
    assert!(envelope.validate().is_err());
    envelope = invitation();
    envelope.source.unit_id = "unit\nforged-field".to_owned();
    assert!(envelope.validate().is_err());
}
