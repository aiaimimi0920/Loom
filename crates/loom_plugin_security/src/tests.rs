use std::fs;
use std::path::PathBuf;

use loom_protocol::{
    PackageSignature, PackageTrustStatus, PublisherIdentity, PublisherTrustRecord,
};

use super::*;

fn temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "loom-plugin-security-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("directory");
    path
}

fn signed_fixture(
    name: &str,
) -> (
    PathBuf,
    SigningKeyDocument,
    PublisherIdentity,
    PackageSignature,
    TrustStore,
) {
    let package = temp_dir(name);
    fs::write(package.join("manifest.json"), b"{}\n").unwrap();
    let key = generate_signing_key("test-key");
    sign_package(&package, "signature.json", &key).expect("sign");
    let publisher = PublisherIdentity {
        id: "example.vendor".to_owned(),
        key_id: Some(key.key_id.clone()),
        ..PublisherIdentity::default()
    };
    let signature = PackageSignature {
        algorithm: "ed25519".to_owned(),
        key_id: key.key_id.clone(),
        file: "signature.json".to_owned(),
    };
    let mut trust = TrustStore::default();
    trust.trust(PublisherTrustRecord {
        publisher_id: publisher.id.clone(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: false,
    });
    (package, key, publisher, signature, trust)
}

#[test]
fn package_signature_roundtrip_and_trust_status() {
    let (package, _key, publisher, signature, trust) = signed_fixture("roundtrip");
    assert_eq!(
        verify_package_signature(&package, Some(&publisher), Some(&signature), &trust)
            .expect("verify"),
        PackageTrustStatus::Trusted
    );
    fs::write(package.join("manifest.json"), b"tampered\n").unwrap();
    assert!(
        verify_package_signature(&package, Some(&publisher), Some(&signature), &trust).is_err()
    );
    let _ = fs::remove_dir_all(package);
}

#[test]
fn trust_policy_is_explicit() {
    assert!(TrustPolicy::AllowUnsigned
        .enforce(PackageTrustStatus::Unsigned)
        .is_ok());
    assert!(TrustPolicy::RequireSigned
        .enforce(PackageTrustStatus::Unsigned)
        .is_err());
    assert!(TrustPolicy::RequireTrusted
        .enforce(PackageTrustStatus::Verified)
        .is_err());
}

#[test]
fn trust_policy_matrix_rejects_invalid_and_revoked_statuses() {
    for status in [PackageTrustStatus::Invalid, PackageTrustStatus::Revoked] {
        assert!(TrustPolicy::AllowUnsigned.enforce(status.clone()).is_err());
        assert!(TrustPolicy::RequireSigned.enforce(status.clone()).is_err());
        assert!(TrustPolicy::RequireTrusted.enforce(status).is_err());
    }
    for status in [PackageTrustStatus::Verified, PackageTrustStatus::Trusted] {
        assert!(TrustPolicy::RequireSigned.enforce(status).is_ok());
    }
    assert!(TrustPolicy::RequireTrusted
        .enforce(PackageTrustStatus::Trusted)
        .is_ok());
}

#[test]
fn trust_store_atomic_write_replaces_existing_document() {
    let root = temp_dir("trust-store-atomic");
    let path = root.join("plugin-trust.json");
    let mut trust = TrustStore::default();
    trust
        .write_atomic(&path)
        .expect("write initial trust store");
    trust.trust(PublisherTrustRecord {
        publisher_id: "publisher.atomic".to_owned(),
        key_id: "key-1".to_owned(),
        public_key: "public-key".to_owned(),
        revoked: false,
    });
    trust.set_policy(TrustPolicy::RequireTrusted);
    trust.trust_publisher_id("publisher.atomic");
    trust.write_atomic(&path).expect("replace trust store");

    let loaded = TrustStore::load(&path).expect("load replaced store");
    assert_eq!(loaded, trust);
    assert_eq!(loaded.policy, TrustPolicy::RequireTrusted);
    assert_eq!(loaded.trusted_publishers, ["publisher.atomic"]);
    assert_eq!(
        fs::read_dir(&root)
            .expect("list trust root")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count(),
        0
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&root)
                .expect("trust root metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path)
                .expect("trust metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn publisher_key_id_is_bound_to_the_signature() {
    let (package, _key, mut publisher, signature, trust) = signed_fixture("publisher-key-id");
    publisher.key_id = Some("different-key".to_owned());
    assert!(matches!(
        verify_package_signature(&package, Some(&publisher), Some(&signature), &trust),
        Err(PluginSecurityError::SignatureMetadataMismatch)
    ));
    let _ = fs::remove_dir_all(package);
}

#[test]
fn trust_status_distinguishes_unknown_revoked_and_mismatched_keys() {
    let (package, key, publisher, signature, mut trust) = signed_fixture("trust-statuses");
    let unknown = PublisherIdentity {
        id: "unknown.vendor".to_owned(),
        key_id: Some(key.key_id.clone()),
        ..PublisherIdentity::default()
    };
    assert_eq!(
        verify_package_signature(&package, Some(&unknown), Some(&signature), &trust).unwrap(),
        PackageTrustStatus::Verified
    );
    assert!(trust.revoke(&publisher.id, &key.key_id));
    assert_eq!(
        verify_package_signature(&package, Some(&publisher), Some(&signature), &trust).unwrap(),
        PackageTrustStatus::Revoked
    );
    trust.publishers[0].revoked = false;
    trust.publishers[0].public_key = "different-public-key".to_owned();
    assert!(matches!(
        verify_package_signature(&package, Some(&publisher), Some(&signature), &trust),
        Err(PluginSecurityError::VerificationFailed)
    ));
    let _ = fs::remove_dir_all(package);
}

#[test]
fn trust_store_rejects_unknown_schema_and_duplicate_keys() {
    let root = temp_dir("trust-schema");
    let path = root.join("plugin-trust.json");
    let mut trust = TrustStore {
        schema_version: 2,
        ..TrustStore::default()
    };
    fs::write(&path, serde_json::to_vec(&trust).unwrap()).unwrap();
    assert!(TrustStore::load(&path)
        .unwrap_err()
        .to_string()
        .contains("unsupported trust-store schema"));

    trust.schema_version = TRUST_STORE_SCHEMA_VERSION;
    let record = PublisherTrustRecord {
        publisher_id: "duplicate.publisher".to_owned(),
        key_id: "duplicate-key".to_owned(),
        public_key: "public-key".to_owned(),
        revoked: false,
    };
    trust.publishers = vec![record.clone(), record];
    fs::write(&path, serde_json::to_vec(&trust).unwrap()).unwrap();
    assert!(TrustStore::load(&path)
        .unwrap_err()
        .to_string()
        .contains("duplicate trust record"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn signing_key_io_is_atomic_private_bounded_and_versioned() {
    let root = temp_dir("key-io");
    let path = root.join("keys").join("publisher.json");
    let key = generate_signing_key("publisher-key");
    write_signing_key(&path, &key).expect("write key");
    assert_eq!(read_signing_key(&path).expect("read key"), key);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    let mut unknown = key.clone();
    unknown.schema_version += 1;
    fs::write(&path, serde_json::to_vec(&unknown).unwrap()).unwrap();
    assert!(read_signing_key(&path)
        .unwrap_err()
        .to_string()
        .contains("unsupported signing-key schema"));
    fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&path)
        .unwrap()
        .set_len(MAX_SIGNING_KEY_BYTES + 1)
        .unwrap();
    assert!(read_signing_key(&path)
        .unwrap_err()
        .to_string()
        .contains("exceeds the"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn signature_document_read_is_bounded() {
    let package = temp_dir("signature-bound");
    fs::write(package.join("manifest.json"), b"{}\n").unwrap();
    let signature_path = package.join("signature.json");
    fs::File::create(&signature_path)
        .unwrap()
        .set_len(MAX_SIGNATURE_DOCUMENT_BYTES + 1)
        .unwrap();
    let signature = PackageSignature {
        algorithm: "ed25519".to_owned(),
        key_id: "key".to_owned(),
        file: "signature.json".to_owned(),
    };
    assert!(
        verify_package_signature(&package, None, Some(&signature), &TrustStore::default())
            .unwrap_err()
            .to_string()
            .contains("exceeds the")
    );
    let _ = fs::remove_dir_all(package);
}

#[test]
fn canonical_digest_is_deterministic_and_excludes_the_signature() {
    let package = temp_dir("digest");
    fs::create_dir_all(package.join("nested")).unwrap();
    fs::write(package.join("manifest.json"), b"manifest\n").unwrap();
    fs::write(
        package.join("nested").join("payload.bin"),
        vec![7u8; 256 * 1024],
    )
    .unwrap();
    let before = canonical_package_digest(&package, Some("signature.json")).unwrap();
    fs::write(package.join("signature.json"), b"ignored\n").unwrap();
    let after = canonical_package_digest(&package, Some("signature.json")).unwrap();
    assert_eq!(before, after);
    let _ = fs::remove_dir_all(package);
}

#[cfg(unix)]
#[test]
fn signature_output_rejects_a_preexisting_symlink() {
    use std::os::unix::fs::symlink;

    let package = temp_dir("signature-symlink");
    fs::write(package.join("manifest.json"), b"{}\n").unwrap();
    let outside = package.with_extension("outside");
    fs::write(&outside, b"keep\n").unwrap();
    symlink(&outside, package.join("signature.json")).unwrap();
    let error = sign_package(&package, "signature.json", &generate_signing_key("key"))
        .expect_err("signature symlink must fail");
    assert!(matches!(error, PluginSecurityError::SymbolicLink(_)));
    assert_eq!(fs::read(&outside).unwrap(), b"keep\n");
    let _ = fs::remove_dir_all(package);
    let _ = fs::remove_file(outside);
}

#[test]
fn signature_path_rejects_parent_traversal() {
    let package = temp_dir("signature-traversal");
    fs::write(package.join("manifest.json"), b"{}\n").unwrap();
    assert!(matches!(
        sign_package(&package, "../outside.json", &generate_signing_key("key")),
        Err(PluginSecurityError::UnsafePath(_))
    ));
    let _ = fs::remove_dir_all(package);
}
