use super::*;
use serde_json::json;
fn fixture() -> (Grant, serde_json::Value) {
    let grant: Grant = serde_json::from_value(super::super::tests::value()).unwrap();
    let value = json!({"protocolVersion":PROTOCOL,"binding":grant.binding,
        "grant":{"algorithm":"ed25519","keyId":"fixture","payload":"e30=","signature":"fixture"},
        "operation":{"kind":"configure","configuration":{
            "scope":{"sessionId":"live:1","epoch":1,"bindingId":"local-art:1","authorizationId":"auth:1"},
            "consent":"automatic_local_callback","sequence":2,
            "condition":{"conditionId":"condition:1","revision":1,"observationId":"observation:1",
                "operator":"equals","operand":100,"stableForMs":0,"risingEdge":false,"rearm":true,"minimumConfidence":"exact"}}}});
    (grant, value)
}
#[test]
fn local_trigger_requires_explicit_consent_version_and_exact_signed_host() {
    let (grant, value) = fixture();
    let request = Request::parse(&serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(request.validate(&grant, "device:1", 1000), Ok(()));
    assert_eq!(
        request.validate(&grant, "other", 1000),
        Err(Error::Identity)
    );
    assert_eq!(
        request.validate(&grant, "device:1", 10000),
        Err(Error::Time)
    );
    for (path, bad) in [
        ("/protocolVersion", json!("loom.art.local.grant.v1")),
        ("/operation/configuration/consent", json!("preview")),
        ("/operation/kind", json!("click")),
    ] {
        let mut changed = value.clone();
        *changed.pointer_mut(path).unwrap() = bad;
        assert!(Request::parse(&serde_json::to_vec(&changed).unwrap()).is_err());
    }
    let mut changed = request.clone();
    changed.binding.package_digest = "c".repeat(64);
    assert_eq!(
        changed.validate(&grant, "device:1", 1000),
        Err(Error::Identity)
    );
    let mut changed = value;
    changed["operation"]["configuration"]
        .as_object_mut()
        .unwrap()
        .remove("consent");
    assert!(Request::parse(&serde_json::to_vec(&changed).unwrap()).is_err());
}
#[test]
fn local_trigger_reduction_never_claims_or_acks_with_an_expired_grant() {
    let (grant, mut value) = fixture();
    let scope = value["operation"]["configuration"]["scope"].clone();
    for kind in ["inspect", "stop", "claim", "ack"] {
        value["operation"] = json!({"kind":kind,"scope":scope});
        if kind == "ack" {
            value["operation"]["deliveryId"] = json!("trigger:1");
            value["operation"]["receipt"] = json!({"kind":"applied","formalRevision":1});
        }
        let request = Request::parse(&serde_json::to_vec(&value).unwrap()).unwrap();
        let expected = if matches!(kind, "inspect" | "stop") {
            Ok(())
        } else {
            Err(Error::Time)
        };
        assert_eq!(request.validate(&grant, "device:1", 10000), expected);
        assert_eq!(
            request.validate(&grant, "other", 10000),
            Err(Error::Identity)
        );
    }
}
#[test]
fn local_trigger_rejects_unbounded_scope_invalid_conditions_and_forged_receipts() {
    let (grant, value) = fixture();
    assert_eq!(
        Request::parse(&vec![b' '; MAX_REQUEST + 1]).err(),
        Some(Error::Limit)
    );
    for (path, bad) in [
        ("/operation/configuration/scope/epoch", json!(0)),
        (
            "/operation/configuration/scope/bindingId",
            json!("binding:surface"),
        ),
        ("/operation/configuration/condition/revision", json!(0)),
        ("/operation/configuration/sequence", json!(0)),
    ] {
        let mut changed = value.clone();
        *changed.pointer_mut(path).unwrap() = bad;
        let request = Request::parse(&serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(
            request.validate(&grant, "device:1", 1000).is_err(),
            "{path}"
        );
    }
    let scope = value["operation"]["configuration"]["scope"].clone();
    for revision in [0, 9_007_199_254_740_992u64] {
        let mut changed = value.clone();
        changed["operation"] = json!({"kind":"ack","scope":scope,"deliveryId":"trigger:1",
            "receipt":{"kind":"applied","formalRevision":revision}});
        let request = Request::parse(&serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(request.validate(&grant, "device:1", 1000).is_err());
    }
}
