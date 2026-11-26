//! New port proofs do not alter legacy contracts; deterministic test keys verify tampering, versions, normalized results, and proof byte budgets.
use super::*;
use loom_protocol::local_art::package::{
    self,
    ports::{self, Package, Port, Ports},
};
fn proof() -> Package {
    Package {
        protocol_version: ports::PROTOCOL.into(),
        package: package::Package {
            protocol_version: package::PROTOCOL.into(),
            grant: grant(),
            device_public_key: key().public_key,
            resources: vec![],
        },
        ports: Ports {
            inputs: vec![Port {
                name: "原始图片".into(),
                kind: "image".into(),
            }],
            outputs: vec![],
        },
    }
}
#[test]
fn new_port_proof_has_independent_domain_and_cannot_drop_or_change_ports() {
    let key = key();
    let p = proof();
    let bytes = issue_port_package(&key, &p).unwrap();
    assert_eq!(
        verify_port_package(&bytes, &key.key_id, &key.public_key).unwrap(),
        p
    );
    assert!(verify_package(&bytes, &key.key_id, &key.public_key).is_err());
    assert!(verify_resume(&bytes, &key.key_id, &key.public_key).is_err());
    assert!(verify(&bytes, &key.key_id, &key.public_key).is_err());
    let old = issue_package(&key, &p.package).unwrap();
    assert!(verify_port_package(&old, &key.key_id, &key.public_key).is_err());
    assert_eq!(
        verify_package(&old, &key.key_id, &key.public_key).unwrap(),
        p.package
    );
    let mut e: Envelope = serde_json::from_slice(&bytes).unwrap();
    let mut v: serde_json::Value =
        serde_json::from_slice(&BASE64.decode(&e.payload).unwrap()).unwrap();
    v["ports"]["inputs"][0]["name"] = "changed".into();
    e.payload = BASE64.encode(serde_json::to_vec(&v).unwrap());
    assert!(verify_port_package(
        &serde_json::to_vec(&e).unwrap(),
        &key.key_id,
        &key.public_key
    )
    .is_err());
}
#[test]
fn port_proof_rejects_invalid_duplicate_and_oversized_metadata_before_signing() {
    for change in 0..6 {
        let mut p = proof();
        match change {
            0 => p.ports.inputs[0].name = " spaced ".into(),
            1 => p.ports.inputs[0].kind = "bad\nkind".into(),
            2 => p.ports.inputs.push(p.ports.inputs[0].clone()),
            3 => {
                p.ports.inputs = (0..33)
                    .map(|i| Port {
                        name: format!("p{i}"),
                        kind: "image".into(),
                    })
                    .collect()
            }
            4 => p.protocol_version = package::PROTOCOL.into(),
            _ => {
                p.ports.inputs = (0..32)
                    .map(|i| Port {
                        name: format!("{i:03}{}", "a".repeat(125)),
                        kind: "b".repeat(64),
                    })
                    .collect();
                p.ports.outputs = p.ports.inputs.clone();
            }
        }
        assert!(issue_port_package(&key(), &p).is_err(), "change {change}");
    }
}
#[test]
fn visibility_proof_is_independent_strict_and_tamper_evident() {
    use loom_protocol::local_art::package::visibility as v3;
    let p = proof();
    let modern = v3::Package {
        protocol_version: v3::PROTOCOL.into(),
        package: p.package,
        ports: p.ports,
        visibility: v3::Visibility {
            inputs: vec![false],
            outputs: vec![],
        },
    };
    let key = key();
    let bytes = issue_visibility_package(&key, &modern).unwrap();
    assert_eq!(
        verify_visibility_package(&bytes, &key.key_id, &key.public_key).unwrap(),
        modern
    );
    assert!(verify_port_package(&bytes, &key.key_id, &key.public_key).is_err());
    assert!(verify_package(&bytes, &key.key_id, &key.public_key).is_err());
    let legacy = issue_port_package(&key, &proof()).unwrap();
    assert!(verify_visibility_package(&legacy, &key.key_id, &key.public_key).is_err());
    let mut e: Envelope = serde_json::from_slice(&bytes).unwrap();
    let mut v: serde_json::Value =
        serde_json::from_slice(&BASE64.decode(&e.payload).unwrap()).unwrap();
    v["visibility"]["inputs"][0] = true.into();
    e.payload = BASE64.encode(serde_json::to_vec(&v).unwrap());
    assert!(verify_visibility_package(
        &serde_json::to_vec(&e).unwrap(),
        &key.key_id,
        &key.public_key
    )
    .is_err());
    for change in 0..5 {
        let mut p = modern.clone();
        match change {
            0 => p.visibility.inputs.clear(),
            1 => p.visibility.inputs.push(true),
            2 => p.visibility.outputs.push(false),
            3 => p.protocol_version = ports::PROTOCOL.into(),
            _ => p.ports.inputs.clear(),
        }
        assert!(issue_visibility_package(&key, &p).is_err());
    }
}
