// 缓存仅消除源码传输，仍通过原真实设备/审批/包验证和签发；由原HTTP测试include。
mod cache_tests {
    use super::*;
    fn input(f: &Fixture, digest: &str) -> Value {
        let mut input = f.request.clone();
        input["protocolVersion"] = loom_protocol::local_art::package::visibility::PREPARE.into();
        input["knownSourceDigest"] = json!(digest);
        input
    }
    fn cached(f: &Fixture, input: Value) -> (u16, Value) {
        device(
            f.port,
            &f.token,
            "POST",
            "/v3/art/local/prepare-package",
            Some(input),
        )
    }
    #[test]
    fn current_package_cache_hit_reissues_exact_grant_and_miss_returns_source() {
        let f = Fixture::new();
        assert_eq!(f.approve(0, true).0, 200);
        let mut input = input(&f, &sha256_bytes(SOURCE));
        input["binding"]["requestId"] = json!("request:cache-new");
        let (status, response) = cached(&f, input.clone());
        assert_eq!(status, 200, "{response}");
        assert_eq!(response["protocolVersion"], loom_protocol::local_art::package::visibility::PREPARE);
        assert!(
            response.as_object().unwrap().contains_key("source") && response["source"].is_null()
        );
        let grant = loom_plugin_security::local_art_grant::verify(
            &serde_json::to_vec(&response["grant"]).unwrap(),
            f.issuer["keyId"].as_str().unwrap(),
            f.issuer["publicKey"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(grant.binding.request_id, "request:cache-new");
        assert_eq!(grant.source.digest, sha256_bytes(SOURCE));
        input["knownSourceDigest"] = json!("0".repeat(64));
        let (status, repaired) = cached(&f, input);
        assert_eq!(status, 200);
        assert_eq!(
            BASE64.decode(repaired["source"].as_str().unwrap()).unwrap(),
            SOURCE
        );
    }
    #[test]
    fn local_art_cached_prepare_never_bypasses_approval_identity_or_schema() {
        let f = Fixture::new();
        let valid = input(&f, &sha256_bytes(SOURCE));
        assert_eq!(cached(&f, valid.clone()).0, 403);
        assert_eq!(f.approve(0, true).0, 200);
        for field in ["protocolVersion", "knownSourceDigest"] {
            let mut invalid = valid.clone();
            invalid[field] = json!("invalid");
            assert_eq!(cached(&f, invalid).0, 400);
        }
        let mut unknown = valid.clone();
        unknown["grant"] = json!({});
        assert_eq!(cached(&f, unknown).0, 400);
        let mut foreign = valid.clone();
        foreign["binding"]["deviceId"] = json!("other");
        assert_eq!(cached(&f, foreign).0, 403);
        assert_eq!(f.prepare(valid.clone()).0, 200);
        assert_eq!(f.approve(1, false).0, 200);
        assert_eq!(cached(&f, valid).0, 403);
    }
}
