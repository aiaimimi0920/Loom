//! Signature and publisher trust-policy regressions.

use super::*;

#[test]
fn refuses_an_unsigned_package_when_the_trust_policy_requires_a_signature() {
    // The trust policy used to apply to Arts only, so an operator who required signatures still
    // got unsigned MCP servers installed without a word.
    let root = std::env::temp_dir().join(staging_name());
    write_trust_store(
        &root,
        &TrustStore {
            policy: TrustPolicy::RequireSigned,
            ..TrustStore::default()
        },
    );

    let error = match install_server_package(&root, &stdio_package_bytes()) {
        Ok(_) => panic!("an unsigned package must not install under require-signed"),
        Err(error) => error,
    };
    assert!(
        matches!(&error, McpPackageError::Trust(_)),
        "unexpected error: {error}"
    );
    assert!(!root
        .join("mcp/packages/publisher.test/fixture-search")
        .exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn installs_a_signed_package_when_the_trust_policy_requires_a_signature() {
    let root = std::env::temp_dir().join(staging_name());
    write_trust_store(
        &root,
        &TrustStore {
            policy: TrustPolicy::RequireSigned,
            ..TrustStore::default()
        },
    );
    let key = generate_signing_key("fixture-key");

    let config = install_server_package(&root, &signed_package_bytes(&key, b"Write-Output ready"))
        .expect("a signed package installs");

    assert_eq!(config.id, "fixture-search");
    // The signature document travels with the package and is hashed like any other file.
    let state = config.package.as_ref().expect("package state");
    assert!(state.files.contains_key(SIGNATURE_FILE));
    // A signature nobody has pinned a key for is `Verified`, and that verdict is persisted so a
    // reader does not have to re-verify the signature to report it.
    assert_eq!(state.trust_status, PackageTrustStatus::Verified);
    let active =
        read_active_state(&root, "publisher.test", "fixture-search").expect("read active state");
    assert_eq!(active.trust_status, PackageTrustStatus::Verified);
    verify_installed_entry(&config).expect("entry matches its recorded digest");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn refuses_a_package_whose_files_changed_after_it_was_signed() {
    // Repacking a signed package with a different runtime file leaves the signature document
    // internally consistent; only the digest it covers gives the swap away. No policy is set
    // here, because a signature that does not match its package is a failure at any policy.
    let root = std::env::temp_dir().join(staging_name());
    let key = generate_signing_key("fixture-key");
    let signed = signed_package_bytes(&key, b"Write-Output ready");
    let mut archive = ZipArchive::new(Cursor::new(signed)).expect("open signed package");
    let mut signature = Vec::new();
    archive
        .by_name(SIGNATURE_FILE)
        .expect("signature entry")
        .read_to_end(&mut signature)
        .expect("read signature document");
    let manifest = signed_manifest(&key.key_id);
    let tampered = package_bytes_with_files(&[
        (MCP_SERVER_PACKAGE_MANIFEST, manifest.as_bytes()),
        ("runtime/server.ps1", b"Write-Output tampered"),
        (SIGNATURE_FILE, &signature),
    ]);

    let error = match install_server_package(&root, &tampered) {
        Ok(_) => panic!("a package that no longer matches its signature must not install"),
        Err(error) => error,
    };
    assert!(
        matches!(&error, McpPackageError::Trust(message) if message.contains("digest")),
        "unexpected error: {error}"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn requires_a_trusted_publisher_key_when_the_policy_asks_for_one() {
    let root = std::env::temp_dir().join(staging_name());
    let key = generate_signing_key("fixture-key");
    write_trust_store(
        &root,
        &TrustStore {
            policy: TrustPolicy::RequireTrusted,
            publishers: vec![PublisherTrustRecord {
                publisher_id: "publisher.test".to_owned(),
                key_id: key.key_id.clone(),
                public_key: key.public_key.clone(),
                revoked: false,
            }],
            ..TrustStore::default()
        },
    );

    install_server_package(&root, &signed_package_bytes(&key, b"Write-Output ready"))
        .expect("a package signed by a trusted key installs");

    let active =
        read_active_state(&root, "publisher.test", "fixture-search").expect("read active state");
    assert_eq!(active.trust_status, PackageTrustStatus::Trusted);

    // A signature made with some other key is refused: the publisher named in the manifest has a
    // pinned key here, and this is not it.
    let other = generate_signing_key("other-key");
    let error =
        match install_server_package(&root, &signed_package_bytes(&other, b"Write-Output odd")) {
            Ok(_) => panic!("an untrusted key must not satisfy require-trusted"),
            Err(error) => error,
        };
    assert!(
        matches!(&error, McpPackageError::Trust(_)),
        "unexpected error: {error}"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn refuses_a_signature_from_a_key_the_publisher_does_not_use() {
    // The verifier calls an unknown `(publisher, key)` pair `Verified`, which `require-signed`
    // accepts. Without a binding check, anyone holding any valid key could publish under a
    // publisher name the operator had already pinned.
    let root = std::env::temp_dir().join(staging_name());
    let pinned = generate_signing_key("pinned-key");
    write_trust_store(
        &root,
        &TrustStore {
            policy: TrustPolicy::RequireSigned,
            publishers: vec![PublisherTrustRecord {
                publisher_id: "publisher.test".to_owned(),
                key_id: pinned.key_id.clone(),
                public_key: pinned.public_key.clone(),
                revoked: false,
            }],
            ..TrustStore::default()
        },
    );
    let impostor = generate_signing_key("impostor-key");

    let error = match install_server_package(
        &root,
        &signed_package_bytes(&impostor, b"Write-Output odd"),
    ) {
        Ok(_) => panic!("a key that is not recorded for this publisher must be refused"),
        Err(error) => error,
    };
    assert!(
        matches!(&error, McpPackageError::Trust(message)
                if message.contains("does not record for that publisher")),
        "unexpected error: {error}"
    );
    assert!(!root
        .join("mcp/packages/publisher.test/fixture-search")
        .exists());

    // The publisher's own key still installs, and a publisher nobody has pinned is unaffected:
    // the check only fires when there is a recorded key to contradict.
    install_server_package(&root, &signed_package_bytes(&pinned, b"Write-Output ready"))
        .expect("the recorded key installs");
    let _ = fs::remove_dir_all(root);
}
