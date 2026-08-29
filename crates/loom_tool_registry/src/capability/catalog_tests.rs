use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use chrono::{Duration, Utc};
use loom_plugin_security::{generate_signing_key, sign_message, TrustStore};
use loom_protocol::{
    CapabilityApiRequirement, CapabilityHostCompatibility, CapabilityPublisher,
    PublisherTrustRecord,
};
use loom_security::network::OutboundPolicy;
use sha2::{Digest as _, Sha256};

use super::*;

#[test]
fn signed_catalog_rejects_tampering_and_expired_metadata() {
    let key = generate_signing_key("catalog-1");
    let mut trust = TrustStore::default();
    trust.trust(PublisherTrustRecord {
        publisher_id: "neuro.official".to_owned(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: false,
    });
    let now = Utc::now();
    let document = signed_catalog(&key, now, now + Duration::hours(1));
    let bytes = serde_json::to_vec(&document).expect("catalog JSON");

    let verified = parse_and_verify_capability_catalog(&bytes, &trust, now).expect("catalog");
    assert_eq!(
        verified.signed.packages[0].qualified_id,
        "neuro.official/text-tools"
    );

    let mut tampered = document.clone();
    tampered.signed.packages[0].name = "Tampered".to_owned();
    assert!(parse_and_verify_capability_catalog(
        &serde_json::to_vec(&tampered).unwrap(),
        &trust,
        now
    )
    .is_err());

    let expired = signed_catalog(&key, now - Duration::hours(2), now - Duration::hours(1));
    assert!(parse_and_verify_capability_catalog(
        &serde_json::to_vec(&expired).unwrap(),
        &trust,
        now
    )
    .is_err());
}

#[test]
fn catalog_requires_trusted_non_revoked_signer() {
    let key = generate_signing_key("catalog-1");
    let now = Utc::now();
    let document = signed_catalog(&key, now, now + Duration::hours(1));
    let bytes = serde_json::to_vec(&document).unwrap();

    assert!(parse_and_verify_capability_catalog(&bytes, &TrustStore::default(), now).is_err());

    let mut revoked = TrustStore::default();
    revoked.trust(PublisherTrustRecord {
        publisher_id: "neuro.official".to_owned(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: true,
    });
    assert!(parse_and_verify_capability_catalog(&bytes, &revoked, now).is_err());
}

#[test]
fn catalog_rejects_a_trusted_non_official_publisher() {
    let key = generate_signing_key("catalog-1");
    let now = Utc::now();
    let mut document = signed_catalog(&key, now, now + Duration::hours(1));
    document.signed.publisher.id = "publisher.example".to_owned();
    document.signature.value = sign_message(
        &key,
        &serde_json::to_vec(&document.signed).expect("catalog payload"),
    )
    .expect("catalog signature");
    let mut trust = TrustStore::default();
    trust.trust(PublisherTrustRecord {
        publisher_id: "publisher.example".to_owned(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: false,
    });

    assert!(parse_and_verify_capability_catalog(
        &serde_json::to_vec(&document).unwrap(),
        &trust,
        now,
    )
    .is_err());
}

#[test]
fn host_compatibility_requires_extension_hook_and_declared_features() {
    let key = generate_signing_key("catalog-1");
    let now = Utc::now();
    let entry = signed_catalog(&key, now, now + Duration::hours(1))
        .signed
        .packages
        .remove(0);
    let mut support = compatible_host(false);
    assert_eq!(
        capability_host_compatibility_error(&entry, &support).as_deref(),
        Some("需要连接支持能力扩展的 Hook")
    );
    support.hook_connected = true;
    assert!(capability_host_compatibility_error(&entry, &support).is_none());
    support.hook_features.clear();
    assert!(capability_host_compatibility_error(&entry, &support)
        .is_some_and(|message| message.contains("Hook")));
}

#[test]
fn catalog_client_downloads_digest_pinned_supply_chain_artifacts() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("catalog fixture listener");
    let base = format!("http://{}", listener.local_addr().unwrap());
    let key = generate_signing_key("catalog-1");
    let now = Utc::now();
    let package = b"signed-package-fixture".to_vec();
    let sbom = br#"{"bomFormat":"CycloneDX"}"#.to_vec();
    let provenance = br#"{"predicateType":"https://slsa.dev/provenance/v1"}"#.to_vec();
    let mut document = signed_catalog(&key, now, now + Duration::hours(1));
    let entry = &mut document.signed.packages[0];
    set_artifact(
        &mut entry.package.url,
        &mut entry.package.sha256,
        &mut entry.package.bytes,
        &base,
        "package.zip",
        &package,
    );
    set_artifact(
        &mut entry.sbom.url,
        &mut entry.sbom.sha256,
        &mut entry.sbom.bytes,
        &base,
        "package.cdx.json",
        &sbom,
    );
    set_artifact(
        &mut entry.provenance.url,
        &mut entry.provenance.sha256,
        &mut entry.provenance.bytes,
        &base,
        "provenance.json",
        &provenance,
    );
    document.signature.value =
        sign_message(&key, &serde_json::to_vec(&document.signed).unwrap()).unwrap();
    let catalog = serde_json::to_vec(&document).unwrap();
    let bodies = BTreeMap::from([
        ("/catalog.json".to_owned(), catalog),
        ("/package.zip".to_owned(), package.clone()),
        ("/package.cdx.json".to_owned(), sbom.clone()),
        ("/provenance.json".to_owned(), provenance.clone()),
    ]);
    let server = thread::spawn(move || serve_fixture(listener, bodies));
    let mut trust = TrustStore::default();
    trust.trust(PublisherTrustRecord {
        publisher_id: "neuro.official".to_owned(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: false,
    });
    let client = CapabilityCatalogClient::new(
        format!("{base}/catalog.json"),
        OutboundPolicy {
            allow_http_loopback: true,
            ..OutboundPolicy::default()
        },
    )
    .expect("catalog client");

    let verified = client.fetch(&trust).expect("verified catalog");
    let downloaded = client
        .download(&verified.signed.packages[0])
        .expect("downloaded package");
    assert_eq!(downloaded.package_bytes, package);
    assert!(downloaded.supply_chain_verified);
    server.join().expect("catalog fixture server");
}

fn set_artifact(
    url: &mut String,
    digest: &mut String,
    bytes: &mut u64,
    base: &str,
    name: &str,
    body: &[u8],
) {
    *url = format!("{base}/{name}");
    *digest = format!("{:x}", Sha256::digest(body));
    *bytes = body.len() as u64;
}

fn serve_fixture(listener: TcpListener, bodies: BTreeMap<String, Vec<u8>>) {
    for _ in 0..bodies.len() {
        let (mut stream, _) = listener.accept().expect("catalog fixture connection");
        let mut request = [0_u8; 4096];
        let size = stream.read(&mut request).expect("catalog fixture request");
        let first_line = String::from_utf8_lossy(&request[..size]);
        let path = first_line.split_whitespace().nth(1).expect("request path");
        let body = bodies.get(path).expect("fixture response");
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .expect("fixture response headers");
        stream.write_all(body).expect("fixture response body");
    }
}

fn signed_catalog(
    key: &loom_plugin_security::SigningKeyDocument,
    generated_at: chrono::DateTime<Utc>,
    expires_at: chrono::DateTime<Utc>,
) -> CapabilityCatalogDocument {
    let publisher = CapabilityPublisher {
        id: "neuro.official".to_owned(),
        key_id: key.key_id.clone(),
    };
    let payload = CapabilityCatalogPayload {
        schema_version: CAPABILITY_CATALOG_SCHEMA_VERSION,
        publisher: publisher.clone(),
        generated_at: generated_at.to_rfc3339(),
        expires_at: expires_at.to_rfc3339(),
        packages: vec![CapabilityCatalogEntry {
            qualified_id: "neuro.official/text-tools".to_owned(),
            name: "Text Tools".to_owned(),
            description: "Signed fixture".to_owned(),
            version: "1.0.0".to_owned(),
            publisher,
            package: CapabilityCatalogPackage {
                url: "https://plugins.example.test/text-tools.zip".to_owned(),
                sha256: "1".repeat(64),
                bytes: 1024,
                signature: CapabilityCatalogPackageSignature {
                    algorithm: "ed25519".to_owned(),
                    key_id: key.key_id.clone(),
                },
            },
            sbom: artifact("text-tools.cdx.json"),
            provenance: artifact("text-tools.provenance.json"),
            host_compatibility: host_compatibility(),
            permissions: vec!["hook.notice.show".to_owned()],
            disk_bytes: 4096,
        }],
    };
    let signature = sign_message(key, &serde_json::to_vec(&payload).unwrap()).unwrap();
    CapabilityCatalogDocument {
        signed: payload,
        signature: CapabilityCatalogSignature {
            algorithm: "ed25519".to_owned(),
            key_id: key.key_id.clone(),
            value: signature,
        },
    }
}

fn artifact(name: &str) -> CapabilityCatalogArtifact {
    CapabilityCatalogArtifact {
        url: format!("https://plugins.example.test/{name}"),
        sha256: "2".repeat(64),
        bytes: 128,
    }
}

fn host_compatibility() -> CapabilityHostCompatibility {
    CapabilityHostCompatibility {
        loom_capability_api: requirement(&["commands.v1", "attachments.v1"]),
        hook_extension_api: requirement(&["commands.v1", "unit-overlays.v1"]),
        surface_api: Some(requirement(&["loom_resource"])),
    }
}

fn requirement(features: &[&str]) -> CapabilityApiRequirement {
    CapabilityApiRequirement {
        minimum: "1.0".to_owned(),
        maximum: Some("1.0".to_owned()),
        required_features: features
            .iter()
            .map(|feature| (*feature).to_owned())
            .collect(),
        optional_features: Vec::new(),
    }
}

fn compatible_host(hook_connected: bool) -> CapabilityHostSupport {
    CapabilityHostSupport {
        loom_api_version: "1.0".to_owned(),
        loom_features: vec!["commands.v1".to_owned(), "attachments.v1".to_owned()],
        hook_connected,
        hook_api_version: "1.0".to_owned(),
        hook_features: vec!["commands.v1".to_owned(), "unit-overlays.v1".to_owned()],
        surface_api_version: "1.0".to_owned(),
        surface_features: vec!["loom_resource".to_owned()],
    }
}
