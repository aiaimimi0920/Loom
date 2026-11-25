//! 固定测试密钥不用于生产；跨仓库 fixture 由真实 Loom 签发器产生，Otter 独立验签。
use super::*;
use ed25519_dalek::SigningKey;
use loom_protocol::local_art::{Binding, Permission, Source, PROTOCOL, SDK};
use sha2::{Digest, Sha256};
const SOURCE: &[u8] = b"globalThis.art = { preview: 'local' };";
#[path = "local_art_ports_tests.rs"]
mod ports_tests;
#[test]
fn package_proof_cannot_be_used_as_grant_or_legacy_resume_and_binds_resource_bytes() {
    use loom_protocol::local_art::package::{Package, Resource, PROTOCOL};
    let key = key();
    let package = Package {
        protocol_version: PROTOCOL.into(),
        grant: grant(),
        device_public_key: key.public_key.clone(),
        resources: vec![Resource {
            id: "text".into(),
            mime_type: "text/plain".into(),
            size: 3,
            digest: format!("{:x}", Sha256::digest(b"abc")),
            dimensions: [0, 0],
        }],
    };
    let bytes = issue_package(&key, &package).unwrap();
    assert_eq!(
        verify_package(&bytes, &key.key_id, &key.public_key).unwrap(),
        package
    );
    assert!(verify(&bytes, &key.key_id, &key.public_key).is_err());
    assert!(verify_resume(&bytes, &key.key_id, &key.public_key).is_err());
    let mut envelope: Envelope = serde_json::from_slice(&bytes).unwrap();
    let mut payload: serde_json::Value =
        serde_json::from_slice(&BASE64.decode(&envelope.payload).unwrap()).unwrap();
    payload["resources"] = serde_json::json!([]);
    envelope.payload = BASE64.encode(serde_json::to_vec(&payload).unwrap());
    assert!(verify_package(
        &serde_json::to_vec(&envelope).unwrap(),
        &key.key_id,
        &key.public_key
    )
    .is_err());
}
#[test]
fn resumable_authorization_has_an_independent_domain_and_preserves_original_bounds() {
    use loom_protocol::local_art::resume::{Resume, PROTOCOL};
    let key = key();
    let mut resume = Resume {
        protocol_version: PROTOCOL.into(),
        device_public_key: key.public_key.clone(),
        grant: grant(),
    };
    let bytes = issue_resume(&key, &resume).unwrap();
    assert_eq!(
        verify_resume(&bytes, &key.key_id, &key.public_key).unwrap(),
        resume
    );
    assert!(verify(&bytes, &key.key_id, &key.public_key).is_err());
    assert!(verify_resume(
        &issue(&key, &resume.grant).unwrap(),
        &key.key_id,
        &key.public_key
    )
    .is_err());
    resume.grant.offline_until_ms = 0;
    assert!(issue_resume(&key, &resume).is_err());
    resume.grant = grant();
    resume.device_public_key = "not-a-key".into();
    assert!(issue_resume(&key, &resume).is_err());
}

fn key() -> SigningKeyDocument {
    let key = SigningKey::from_bytes(&[7; 32]);
    SigningKeyDocument {
        schema_version: 1,
        key_id: "fixture-issuer".into(),
        private_key: BASE64.encode(key.to_bytes()),
        public_key: BASE64.encode(key.verifying_key().to_bytes()),
    }
}
fn grant() -> Grant {
    let digest = format!("{:x}", Sha256::digest(SOURCE));
    Grant {
        protocol_version: PROTOCOL.into(),
        grant_id: "grant:fixture".into(),
        binding: Binding {
            origin: "https://loom.example:9443".into(),
            request_id: "request:1".into(),
            device_id: "device:1".into(),
            workspace_id: "workspace:1".into(),
            instance_id: "instance:1".into(),
            generation: 1,
            art_id: "vendor/demo".into(),
            art_version: "1.2.3".into(),
            package_digest: "a".repeat(64),
            sdk_version: SDK.into(),
        },
        execution_owner: "otter".into(),
        result_owner: "otter_workspace".into(),
        permissions: vec![
            Permission::LocalState,
            Permission::Preview,
            Permission::LoomService,
        ],
        services: vec!["service:echo".into()],
        issued_at_ms: 1000,
        expires_at_ms: 10000,
        offline_until_ms: 5000,
        source: Source {
            resource_id: format!("sha256:{digest}"),
            digest,
            size: SOURCE.len() as u64,
        },
    }
}
#[test]
fn signed_grant_round_trip_requires_the_pinned_issuer() {
    let key = key();
    let bytes = issue(&key, &grant()).unwrap();
    assert_eq!(
        verify(&bytes, &key.key_id, &key.public_key).unwrap(),
        grant()
    );
    assert!(verify(&bytes, "wrong-issuer", &key.public_key).is_err());
    let other = SigningKey::from_bytes(&[8; 32]);
    assert!(verify(
        &bytes,
        &key.key_id,
        &BASE64.encode(other.verifying_key().to_bytes())
    )
    .is_err());
}
#[test]
fn raw_payload_tampering_and_other_signature_domains_are_rejected() {
    let key = key();
    let bytes = issue(&key, &grant()).unwrap();
    let mut envelope: Envelope = serde_json::from_slice(&bytes).unwrap();
    let mut changed = grant();
    changed.binding.generation = 2;
    envelope.payload = BASE64.encode(serde_json::to_vec(&changed).unwrap());
    assert!(verify(
        &serde_json::to_vec(&envelope).unwrap(),
        &key.key_id,
        &key.public_key
    )
    .is_err());
    envelope.signature = sign_message(&key, &BASE64.decode(&envelope.payload).unwrap()).unwrap();
    assert!(verify(
        &serde_json::to_vec(&envelope).unwrap(),
        &key.key_id,
        &key.public_key
    )
    .is_err());
}
#[test]
fn invalid_scope_time_source_and_legacy_protocol_never_get_signed() {
    let mutations: &[fn(&mut Grant)] = &[
        |g| g.protocol_version = "loom.surface.v1".into(),
        |g| g.binding.sdk_version = "unknown".into(),
        |g| g.execution_owner = "loom".into(),
        |g| g.result_owner = "loom".into(),
        |g| g.binding.generation = 0,
        |g| g.binding.package_digest = "A".repeat(64),
        |g| g.permissions.push(Permission::Preview),
        |g| g.services.push("*".into()),
        |g| g.services.clear(),
        |g| g.expires_at_ms = 1000,
        |g| g.offline_until_ms = 10001,
        |g| g.expires_at_ms = 1_000_000,
        |g| g.source.size = 0,
        |g| g.source.resource_id = "file:///private/source.js".into(),
    ];
    for mutate in mutations {
        let mut value = grant();
        mutate(&mut value);
        assert!(issue(&key(), &value).is_err());
    }
}
#[test]
fn duplicate_or_unknown_signed_fields_are_rejected_not_reinterpreted() {
    let key = key();
    let valid = serde_json::to_string(&grant()).unwrap();
    for extra in [
        "\"publicKey\":\"peer-chosen\",",
        "\"protocolVersion\":\"loom.surface.v1\",",
    ] {
        let payload = format!("{{{extra}{}", &valid[1..]);
        let envelope = Envelope {
            algorithm: "ed25519".into(),
            key_id: key.key_id.clone(),
            payload: BASE64.encode(&payload),
            signature: sign_message(&key, &[DOMAIN, payload.as_bytes()].concat()).unwrap(),
        };
        assert!(verify(
            &serde_json::to_vec(&envelope).unwrap(),
            &key.key_id,
            &key.public_key
        )
        .is_err());
    }
}
#[test]
#[ignore = "explicit cross-repository artifact; OTTER_LOCAL_ART_FIXTURE must name a new linshi file"]
fn emit_cross_repository_signed_grant() {
    use std::io::Write;
    let path = std::env::var_os("OTTER_LOCAL_ART_FIXTURE").expect("fixture output path required");
    let bytes = issue(&key(), &grant()).unwrap();
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    file.write_all(&bytes).unwrap();
    file.sync_all().unwrap();
}
