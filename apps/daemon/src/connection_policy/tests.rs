use super::*;

fn path() -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("loom-connection-policy-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    root.join("policy.json")
}

fn generated_secret(len: usize) -> String {
    let mut raw = vec![0u8; len];
    if len > 0 {
        SystemRandom::new().fill(&mut raw).unwrap();
    }
    raw.into_iter()
        .map(|byte| char::from(b'a' + (byte % 26)))
        .collect()
}

fn distinct_secret(existing: &str) -> String {
    loop {
        let candidate = generated_secret(existing.len().max(16));
        if candidate != existing {
            return candidate;
        }
    }
}

fn update(approval: Approval, password: Option<&str>) -> Update {
    Update {
        name: "测试 Loom".into(),
        discoverable: false,
        endpoint: None,
        approval,
        password: password.map(str::to_owned),
    }
}

#[test]
fn connection_policy_identity_is_persistent_and_secrets_are_not_public() {
    let path = path();
    let mut store = Store::open(&path).unwrap();
    let id = store.public_info().loom_id;
    let secret = generated_secret(18);
    let wrong = distinct_secret(&secret);
    store.update(update(Approval::Lan, Some(&secret))).unwrap();
    assert!(store.check_password(&secret));
    assert!(!store.check_password(&wrong));
    let stored = std::fs::read_to_string(&path).unwrap();
    assert!(!stored.contains(&secret));
    let public = serde_json::to_string(&store.public_info()).unwrap();
    assert!(!public.contains("digest") && !public.contains("salt"));
    let loaded = Store::open(&path).unwrap();
    assert_eq!(loaded.public_info().loom_id, id);
    assert!(loaded.check_password(&secret));
}

#[test]
fn connection_policy_rejects_invalid_config_without_overwriting() {
    let path = path();
    let mut store = Store::open(&path).unwrap();
    let original = std::fs::read(&path).unwrap();
    let too_short = generated_secret(5);
    assert!(store.update(update(Approval::Password, None)).is_err());
    assert!(store
        .update(update(Approval::Manual, Some(&too_short)))
        .is_err());
    let mut input = update(Approval::Manual, None);
    input.discoverable = true;
    assert!(store.update(input).is_err());
    for url in [
        "http://192.168.1.10:8765",
        "https://a:b@host:8765",
        "https://host/path",
        "https://host/?x=1",
    ] {
        let mut input = update(Approval::Manual, None);
        input.endpoint = Some(url.into());
        assert!(store.update(input).is_err());
    }
    assert_eq!(std::fs::read(&path).unwrap(), original);
}

#[test]
fn connection_policy_corruption_never_resets_identity() {
    let path = path();
    std::fs::write(&path, "not-json").unwrap();
    assert!(Store::open(&path).is_err());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "not-json");
}

#[test]
fn connection_policy_password_preserve_and_clear_are_explicit() {
    let mut store = Store::open(&path()).unwrap();
    let secret = generated_secret(18);
    let empty = generated_secret(0);
    store
        .update(update(Approval::Manual, Some(&secret)))
        .unwrap();
    store.update(update(Approval::Authenticated, None)).unwrap();
    assert!(!store.check_password(&empty));
    store
        .update(update(Approval::Manual, Some(&empty)))
        .unwrap();
    assert!(!store.public_info().password_required);
    assert!(store.check_password(&empty));
}

#[test]
fn connection_policy_reuses_the_existing_loom_signing_identity() {
    let path = path();
    let id = format!("loom-{}", "a".repeat(64));
    let store = Store::open_with_identity(&path, Some(&id)).unwrap();
    assert_eq!(store.public_info().loom_id, id);
    assert!(Store::open_with_identity(&path, Some(&format!("loom-{}", "b".repeat(64)))).is_err());
    assert_eq!(
        Store::open_with_identity(&path, Some(&id))
            .unwrap()
            .public_info()
            .loom_id,
        id
    );
}

#[test]
fn connection_policy_legacy_password_approval_migrates_without_removing_password() {
    let path = path();
    let mut store = Store::open(&path).unwrap();
    let secret = generated_secret(18);
    let empty = generated_secret(0);
    store
        .update(update(Approval::Manual, Some(&secret)))
        .unwrap();
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    document["approval"] = "password".into();
    std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
    let loaded = Store::open(&path).unwrap();
    assert_eq!(loaded.public_info().approval, Approval::Manual);
    assert!(loaded.public_info().password_required);
    assert!(loaded.check_password(&secret));
    assert!(!loaded.check_password(&empty));
}
