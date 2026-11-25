//! Schema与严格DTO字段合同；聚合预算和设备委托期限仍由运行时检查。
use super::*;
use serde_json::json;
fn value() -> serde_json::Value {
    json!({"protocolVersion":PROTOCOL,"devicePublicKey":"","grant":super::super::tests::value(),"resources":[{"id":"text","mimeType":"text/plain","size":1,"digest":"a".repeat(64),"dimensions":[0,0]}]})
}
#[test]
fn package_schema_and_dto_reject_missing_extra_and_unsafe_fields() {
    let mut schema: serde_json::Value =
        serde_json::from_str(crate::schemas::LOCAL_ART_PACKAGE_V1).unwrap();
    let grant: serde_json::Value =
        serde_json::from_str(crate::schemas::LOCAL_ART_GRANT_V1).unwrap();
    // grant内的本地引用还依赖binding/source等定义；整体装入，保持公开Schema原义。
    schema["$defs"]
        .as_object_mut()
        .unwrap()
        .extend(grant["$defs"].as_object().unwrap().clone());
    schema["$defs"]["package"]["properties"]["grant"] = json!({"$ref":"#/$defs/grant"});
    schema["$ref"] = json!("#/$defs/package");
    for field in ["type", "properties", "required", "additionalProperties"] {
        schema.as_object_mut().unwrap().remove(field);
    }
    let validator = jsonschema::validator_for(&schema).unwrap();
    assert!(validator.is_valid(&value()));
    serde_json::from_value::<Package>(value())
        .unwrap()
        .validate()
        .unwrap();
    for change in 0..5 {
        let mut v = value();
        match change {
            0 => {
                v.as_object_mut().unwrap().remove("resources");
            }
            1 => v["resources"][0]["id"] = "../escape".into(),
            2 => v["resources"][0]["path"] = "private".into(),
            3 => v["devicePublicKey"] = "bad".into(),
            _ => v["resources"][0]["size"] = 0.into(),
        };
        assert!(!validator.is_valid(&v));
        assert!(serde_json::from_value::<Package>(v).map_or(true, |p| p.validate().is_err()));
    }
}
#[test]
fn resource_contract_checks_duplicates_and_arithmetic_before_allocation() {
    for key in ["é".repeat(22), format!("{}B=", "A".repeat(42)), "".into()] {
        assert!(!super::super::resume::valid_device_key(&key));
    }
    let p: Package = serde_json::from_value(value()).unwrap();
    let mut r = p.resources[0].clone();
    assert!(validate_resources(&[r.clone(), r.clone()]).is_err());
    r.mime_type = "image/png".into();
    r.dimensions = [u32::MAX, u32::MAX];
    assert!(validate_resources(&[r]).is_err());
    let list = (0..17)
        .map(|i| {
            let mut r = p.resources[0].clone();
            r.id = format!("r{i}");
            r
        })
        .collect::<Vec<_>>();
    assert!(validate_resources(&list).is_err());
}
