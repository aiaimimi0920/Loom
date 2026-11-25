//! 同一公开 Schema 与 Rust wire 的正负例；时间与权限的关系约束由运行时补充。
use super::*;
use serde_json::{json, Value};

pub(super) fn value() -> Value {
    json!({"protocolVersion":PROTOCOL,"grantId":"grant:1",
        "binding":{"origin":"https://loom.example","requestId":"request:1","deviceId":"device:1",
            "workspaceId":"workspace:1","instanceId":"instance:1","generation":1,
            "artId":"vendor/demo","artVersion":"1.2.3","packageDigest":"a".repeat(64),"sdkVersion":SDK},
        "executionOwner":"otter","resultOwner":"otter_workspace","permissions":["local_state","preview"],"services":[],
        "issuedAtMs":1000,"expiresAtMs":10000,"offlineUntilMs":5000,
        "source":{"resourceId":format!("sha256:{}","b".repeat(64)),"digest":"b".repeat(64),"size":32}})
}
#[test]
fn schema_and_rust_agree_on_explicit_contract_and_reject_legacy_aliases() {
    let schema: Value = serde_json::from_str(crate::schemas::LOCAL_ART_GRANT_V1).unwrap();
    let body = json!({"$schema":"https://json-schema.org/draft/2020-12/schema", "$defs":schema["$defs"],"$ref":"#/$defs/grant"});
    let validator = jsonschema::validator_for(&body).unwrap();
    let original = value();
    assert!(validator.is_valid(&original));
    serde_json::from_value::<Grant>(original.clone())
        .unwrap()
        .validate()
        .unwrap();
    for (field, invalid) in [
        ("protocolVersion", json!("loom.surface.v1")),
        ("executionOwner", json!("loom")),
        ("resultOwner", json!("loom")),
        ("permissions", json!(["filesystem"])),
        ("source", json!({"path":"private/file.js"})),
    ] {
        let mut item = original.clone();
        item[field] = invalid;
        assert!(!validator.is_valid(&item));
        assert!(serde_json::from_value::<Grant>(item).map_or(true, |g| g.validate().is_err()));
    }
    let mut extra = original;
    extra["javascript"] = json!("inline source");
    assert!(!validator.is_valid(&extra));
    assert!(serde_json::from_value::<Grant>(extra).is_err());
}
#[test]
fn unsigned_and_self_trusting_envelopes_are_not_the_wire_contract() {
    let schema: Value = serde_json::from_str(crate::schemas::LOCAL_ART_GRANT_V1).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let mut envelope = json!({"algorithm":"ed25519","keyId":"issuer:1","payload":"e30=","signature":"A".repeat(88)});
    assert!(validator.is_valid(&envelope)); // 格式不是签名有效性证明。
    envelope["publicKey"] = json!("peer selected key");
    assert!(!validator.is_valid(&envelope));
    assert!(serde_json::from_value::<Envelope>(envelope).is_err());
    assert!(!validator.is_valid(&json!({"token":"opaque device token","expiresAtMs":10000})));
}
#[test]
fn runtime_relational_guards_reject_unbounded_time_and_scope() {
    let mut g: Grant = serde_json::from_value(value()).unwrap();
    g.offline_until_ms = g.expires_at_ms + 1;
    assert_eq!(g.validate(), Err(Error::Time));
    g.offline_until_ms = 0;
    g.expires_at_ms = g.issued_at_ms + MAX_VALIDITY_MS + 1;
    assert_eq!(g.validate(), Err(Error::Time));
    g.expires_at_ms = 10000;
    g.services.push("service:echo".into());
    assert_eq!(g.validate(), Err(Error::Permission));
    g.permissions.push(Permission::LoomService);
    assert_eq!(g.validate(), Ok(()));
    g.source.resource_id = format!("sha256:{}", "c".repeat(64));
    assert_eq!(g.validate(), Err(Error::Source));
}
#[test]
fn workspace_result_scope_is_explicit_and_schema_matches_all_four_permissions() {
    let mut item = value();
    let initial: Grant = serde_json::from_value(item.clone()).unwrap();
    assert!(!initial.permissions.contains(&Permission::WorkspaceResult));
    item["permissions"] = json!(["local_state", "preview", "loom_service", "workspace_result"]);
    item["services"] = json!(["service:echo"]);
    let schema: Value = serde_json::from_str(crate::schemas::LOCAL_ART_GRANT_V1).unwrap();
    let body = json!({"$schema":"https://json-schema.org/draft/2020-12/schema", "$defs":schema["$defs"],"$ref":"#/$defs/grant"});
    assert!(jsonschema::validator_for(&body).unwrap().is_valid(&item));
    let mut grant: Grant = serde_json::from_value(item).unwrap();
    grant.validate().unwrap();
    grant.permissions.push(Permission::WorkspaceResult);
    assert_eq!(grant.validate(), Err(Error::Permission));
}
