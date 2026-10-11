use serde_json::{json, Value};

#[test]
fn pairing_schema_requires_exactly_one_minimal_receipt() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../protocol/schemas/device-session.v1.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let entry = json!({"id":"device-opaque", "publicKey":"a".repeat(44),
        "name":"Otter", "approval":"pending", "enabled":true});
    let pending = json!({"devices":[],"pending":[entry.clone()]});
    assert!(validator.is_valid(&pending));
    let mut approved_entry = entry.clone();
    approved_entry["approval"] = json!("approved");
    approved_entry["enabled"] = json!(false);
    assert!(validator.is_valid(&json!({"devices":[approved_entry],"pending":[]})));
    for invalid in [
        json!({"devices":[],"pending":[]}),
        json!({"pending":[entry.clone()]}),
        json!({"devices":[],"pending":[entry.clone(), entry.clone()]}),
        json!({"devices":[entry.clone()],"pending":[entry.clone()]}),
        json!({"devices":[entry.clone()],"pending":[]}),
    ] {
        assert!(!validator.is_valid(&invalid), "{invalid}");
    }
    for field in [
        "createdAt",
        "address",
        "sessionEpoch",
        "isLocal",
        "lastSeenAt",
    ] {
        let mut invalid = pending.clone();
        invalid["pending"][0][field] = json!("private metadata");
        assert!(!validator.is_valid(&invalid), "{field}");
    }
    let mut invalid = pending.clone();
    invalid["connectedClients"] = json!(3);
    assert!(!validator.is_valid(&invalid));
    for field in ["id", "publicKey", "name", "approval", "enabled"] {
        let mut invalid = pending.clone();
        invalid["pending"][0].as_object_mut().unwrap().remove(field);
        assert!(!validator.is_valid(&invalid), "missing {field}");
    }
}
