// Current v3 package carries offline proof directly; there is no separate legacy resume endpoint.
mod resume_tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    fn input(f: &Fixture) -> Value {
        let mut value = f.request.clone();
        value["protocolVersion"] = loom_protocol::local_art::package::visibility::PREPARE.into();
        value["knownSourceDigest"] = "0".repeat(64).into();
        value
    }
    fn prepare(f: &Fixture, value: Value) -> (u16, Value) {
        device(
            f.port,
            &f.token,
            "POST",
            "/v3/art/local/prepare-package",
            Some(value),
        )
    }
    #[test]
    fn local_art_resume_http_signs_only_current_approved_device_key_and_exact_source() {
        let mut f = Fixture::new();
        let key = SigningKey::generate(&mut OsRng);
        let (id, token) =
            super::super::wall_http::pair_with_key(f.port, "Resumable Art fixture", &key);
        f.device = id.clone();
        f.token = token;
        f.request["binding"]["deviceId"] = id.into();
        assert_eq!(prepare(&f, input(&f)).0, 403);
        assert_eq!(f.approve(0, true).0, 200);
        let (status, response) = prepare(&f, input(&f));
        assert_eq!(status, 200, "{response}");
        let bytes = serde_json::to_vec(&response["package"]).unwrap();
        let proof = loom_plugin_security::local_art_grant::verify_visibility_package(
            &bytes,
            f.issuer["keyId"].as_str().unwrap(),
            f.issuer["publicKey"].as_str().unwrap(),
        )
        .unwrap();
        let resume = proof.package;
        assert_eq!(
            resume.device_public_key,
            BASE64.encode(key.verifying_key().to_bytes())
        );
        assert_eq!(resume.grant.source.digest, sha256_bytes(SOURCE));
        assert!(resume.grant.offline_until_ms > resume.grant.issued_at_ms);
        assert!(resume.grant.offline_until_ms <= resume.grant.expires_at_ms);
        let grant = loom_plugin_security::local_art_grant::verify(
            &serde_json::to_vec(&response["grant"]).unwrap(),
            f.issuer["keyId"].as_str().unwrap(),
            f.issuer["publicKey"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(resume.grant, grant);
        assert_eq!(
            BASE64.decode(response["source"].as_str().unwrap()).unwrap(),
            SOURCE
        );
        let mut cached = input(&f);
        cached["knownSourceDigest"] = sha256_bytes(SOURCE).into();
        assert!(prepare(&f, cached).1["source"].is_null());
        assert_eq!(f.approve(1, false).0, 200);
        assert_eq!(prepare(&f, input(&f)).0, 403);
    }
    #[test]
    fn local_art_resume_http_requires_offline_approval_and_rejects_old_protocols() {
        let f = Fixture::new();
        assert_eq!(f.approve(0, true).0, 200);
        // The shared `pair` helper registers a keyed device; explicitly zero `offlineTtlMs` rather than mistaking it for an unkeyed legacy identity.
        let (status, mut approvals) = admin(f.port, "GET", "/v1/art/local/approvals", None);
        assert_eq!(status, 200);
        approvals["approvals"][0]["offlineTtlMs"] = 0.into();
        assert_eq!(
            admin(
                f.port,
                "PUT",
                "/v1/art/local/approvals",
                Some(json!({
                    "expectedRevision": approvals["revision"], "approvals": approvals["approvals"]
                }))
            )
            .0,
            200
        );
        let (status, response) = prepare(&f, input(&f));
        assert_eq!(status, 200, "{response}");
        let proof = loom_plugin_security::local_art_grant::verify_visibility_package(
            &serde_json::to_vec(&response["package"]).unwrap(),
            f.issuer["keyId"].as_str().unwrap(), f.issuer["publicKey"].as_str().unwrap()).unwrap();
        assert!(proof.package.device_public_key.is_empty());
        assert_eq!(proof.package.grant.offline_until_ms, 0);
        assert!(response.get("resume").is_none());
        let mut invalid = input(&f);
        invalid["protocolVersion"] = "loom.art.local.prepare.v2".into();
        assert_eq!(prepare(&f, invalid).0, 400);
        assert_eq!(f.prepare(input(&f)).0, 200);
    }
    #[test]
    fn local_art_resume_http_unbound_legacy_device_cannot_obtain_signed_session() {
        let f = Fixture::new();
        let (status, devices) = admin(
            f.port,
            "POST",
            "/v1/devices",
            Some(json!({
                "name": "Legacy Art without public key", "kind": "computer", "address": "127.0.0.1"
            })),
        );
        assert_eq!(status, 200, "{devices}");
        let legacy = devices["devices"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["name"] == "Legacy Art without public key")
            .unwrap();
        assert!(legacy["publicKey"].is_null());
        let (status, response) = public(
            f.port,
            "POST",
            "/v1/device-sessions/challenges",
            Some(json!({"deviceId": legacy["id"]})),
        );
        assert_eq!(status, 409, "{response}");
        assert_eq!(response["error"]["code"], "device_key_missing");
        assert!(response.get("challengeId").is_none());
    }
}
