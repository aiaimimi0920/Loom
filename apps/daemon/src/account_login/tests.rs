use super::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};

fn session(account: &Account) -> Value {
    json!({"protocol":model::PROTOCOL, "deviceId":uuid::Uuid::new_v4(), "accountId":"fixture-account",
        "username":"Fixture", "deviceName":account.device_name, "publicKey":account.public_key().unwrap(),
        "expiresAtMs":now() + model::SESSION_MS})
}

#[test]
fn public_views_never_contain_key_or_verifier_and_proofs_are_action_bound() {
    let mut account = Account::new(
        "https://platform.example".into(),
        "Test device".into(),
        now(),
    );
    let public = account.view().unwrap().to_string();
    assert!(!public.contains(&account.seed));
    assert!(!public.contains(&account.verifier));
    let exchange = account.exchange().unwrap();
    let key = VerifyingKey::from_bytes(
        &STANDARD
            .decode(account.public_key().unwrap())
            .unwrap()
            .try_into()
            .unwrap(),
    )
    .unwrap();
    let signature = Signature::from_slice(
        &STANDARD
            .decode(exchange["signature"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    let message = format!(
        "{}\nexchange\n{}\n{}",
        model::PROTOCOL,
        account.request_id,
        account.challenge()
    );
    assert!(key.verify(message.as_bytes(), &signature).is_ok());
    account.accept(session(&account), now()).unwrap();
    assert!(account.verifier.is_empty());
    let proof = account.proof("status", now()).unwrap();
    let signed = format!(
        "{}\nstatus\n{}\n{}\n{}",
        model::PROTOCOL,
        proof["deviceId"].as_str().unwrap(),
        proof["timestampMs"],
        proof["nonce"].as_str().unwrap()
    );
    let signature = Signature::from_slice(
        &STANDARD
            .decode(proof["signature"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    assert!(key.verify(signed.as_bytes(), &signature).is_ok());
    assert!(key
        .verify(
            signed.replace("\nstatus\n", "\nrevoke\n").as_bytes(),
            &signature
        )
        .is_err());
}

#[test]
fn account_origin_and_remote_identity_are_checked() {
    for invalid in [
        "http://platform.example",
        "https://u:p@platform.example",
        "https://platform.example/path",
        "https://platform.example/?token=x",
    ] {
        assert!(transport::origin(invalid).is_err());
    }
    assert_eq!(
        transport::origin("https://PLATFORM.example/").unwrap(),
        "https://platform.example"
    );
    assert!(transport::origin("http://127.0.0.1:8765").is_ok());
    let mut account = Account::new("https://platform.example".into(), "Device".into(), now());
    let mut wrong = session(&account);
    wrong["publicKey"] = json!(STANDARD.encode([1; 32]));
    assert!(account.accept(wrong, now()).is_err());
    let valid = session(&account);
    account.accept(valid.clone(), now()).unwrap();
    let mut changed = valid;
    changed["accountId"] = json!("different-account");
    assert_eq!(
        account.accept(changed, now()).unwrap_err().code,
        "account_identity_changed"
    );
}

#[cfg(windows)]
#[test]
fn private_store_survives_restart_and_cancel_rejects_stale_poll() {
    let root = std::env::temp_dir().join(format!("loom-account-{}", uuid::Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let created = handle(
            &root,
            "start",
            r#"{"origin":"https://platform.example","deviceName":"Fixture"}"#,
        )?;
        let store = Store::new(&root);
        let account = store.read()?.unwrap();
        let bytes =
            std::fs::read_to_string(root.join("account-login/plugin-credentials.json")).unwrap();
        assert!(bytes.contains("windows-dpapi-current-user"));
        assert!(!bytes.contains(&account.seed));
        assert!(!bytes.contains(&account.verifier));
        assert_eq!(handle(&root, "status", "{}")?, created);
        assert_eq!(
            handle(&root, "start", "{}").unwrap_err().code,
            "account_logout_required"
        );
        handle(&root, "logout", "{}")?;
        assert!(store.read()?.is_none());
        let next = handle(
            &root,
            "start",
            r#"{"origin":"https://platform.example","deviceName":"Fixture"}"#,
        )?;
        assert_ne!(next["requestId"], created["requestId"]);
        assert_eq!(
            handle(
                &root,
                "poll",
                &json!({"requestId":created["requestId"]}).to_string()
            )
            .unwrap_err()
            .code,
            "account_request_stale"
        );
        Ok(())
    })();
    std::fs::remove_dir_all(&root).unwrap();
    result.unwrap();
}
