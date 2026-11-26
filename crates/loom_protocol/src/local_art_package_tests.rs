//! JSON Schema and strict DTO field contracts; aggregate budgets and device delegation lifetimes are enforced at runtime.
use super::*;
use serde_json::json;
fn value() -> serde_json::Value {
    json!({"protocolVersion":PROTOCOL,"devicePublicKey":"","grant":super::super::tests::value(),"resources":[{"id":"text","mimeType":"text/plain","size":1,"digest":"a".repeat(64),"dimensions":[0,0]}]})
}
#[test]
fn package_schema_and_dto_reject_missing_extra_and_unsafe_fields() {
    let schema: serde_json::Value =
        serde_json::from_str(crate::schemas::LOCAL_ART_PACKAGE_V1).unwrap();
    let grant: serde_json::Value =
        serde_json::from_str(crate::schemas::LOCAL_ART_GRANT_V1).unwrap();
    // Register original public IDs. Rewriting their references would hide broken URI contracts.
    let registry = jsonschema::Registry::new()
        .add(
            schema["$id"].as_str().unwrap(),
            jsonschema::Resource::from_contents(schema.clone()),
        )
        .unwrap()
        .add(
            grant["$id"].as_str().unwrap(),
            jsonschema::Resource::from_contents(grant.clone()),
        )
        .unwrap()
        .prepare()
        .unwrap();
    let validator = jsonschema::options()
        .with_registry(&registry)
        .build(&json!({"$ref": format!("{}#/$defs/package", schema["$id"].as_str().unwrap())}))
        .unwrap();
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

#[test]
fn versioned_package_schemas_resolve_original_public_ids_offline() {
    let sources = [
        crate::schemas::LOCAL_ART_GRANT_V1,
        crate::schemas::LOCAL_ART_PACKAGE_V1,
        include_str!("../../../protocol/schemas/local-art-package.v2.schema.json"),
        include_str!("../../../protocol/schemas/local-art-package.v3.schema.json"),
    ];
    let mut registry = jsonschema::Registry::new();
    for source in sources {
        let schema: serde_json::Value = serde_json::from_str(source).unwrap();
        let id = schema["$id"].as_str().unwrap().to_owned();
        registry = registry
            .add(&id, jsonschema::Resource::from_contents(schema))
            .unwrap();
    }
    let registry = registry.prepare().unwrap();
    for version in [2, 3] {
        let validator = jsonschema::options()
            .with_registry(&registry)
            .build(&json!({"$ref": format!(
                "https://neuro.example/schemas/local-art-package.v{version}.schema.json#/$defs/package"
            )}))
            .unwrap();
        let mut valid = json!({
            "protocolVersion": format!("loom.art.local.package.v{version}"),
            "package": value(),
            "ports": {"inputs": [], "outputs": []}
        });
        if version == 3 {
            valid["visibility"] = json!({"inputs": [], "outputs": []});
        }
        assert!(validator.is_valid(&valid), "v{version} valid package");
        for field in ["ports", "visibility"] {
            if version == 2 && field == "visibility" {
                continue;
            }
            let mut invalid = valid.clone();
            invalid.as_object_mut().unwrap().remove(field);
            assert!(!validator.is_valid(&invalid), "v{version} requires {field}");
        }
        valid["package"]["grant"] = json!({});
        assert!(
            !validator.is_valid(&valid),
            "v{version} must validate nested grant"
        );
    }
}
