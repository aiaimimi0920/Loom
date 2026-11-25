// Description is an authenticated, approved candidate, never an execution permit or package path disclosure.
mod describe_tests {
    use super::*;
    fn describe(f: &Fixture, body: Value) -> (u16, Value) {
        device(
            f.port,
            &f.token,
            "POST",
            "/v1/art/local/describe",
            Some(body),
        )
    }
    fn input() -> Value {
        json!({"artId":"publisher.test/local-counter"})
    }
    #[test]
    #[ignore = "explicit cross-repository descriptor fixture output"]
    fn local_art_describe_exports_real_http_fixture() {
        let f = Fixture::new();
        assert_eq!(f.approve(0, true).0, 200);
        let (status, response) = describe(&f, input());
        assert_eq!(status, 200, "{response}");
        let path = std::env::var_os("OTTER_LOCAL_ART_DESCRIPTOR_FIXTURE")
            .expect("explicit fixture destination");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        file.write_all(&serde_json::to_vec(&response).unwrap())
            .unwrap();
        file.sync_all().unwrap();
    }
    #[test]
    fn local_art_describe_authenticates_and_feeds_exact_prepare_without_grant() {
        let f = Fixture::new();
        for (status, _) in [
            public(f.port, "POST", "/v1/art/local/describe", Some(input())),
            admin(f.port, "POST", "/v1/art/local/describe", Some(input())),
        ] {
            assert!(status == 401 || status == 403);
        }
        assert_eq!(describe(&f, input()).0, 403);
        assert_eq!(f.approve(0, true).0, 200);
        let (status, result) = describe(&f, input());
        assert_eq!(status, 200, "{result}");
        assert_eq!(result.as_object().unwrap().len(), 2);
        assert_eq!(result["protocolVersion"], "loom.art.local.describe.v1");
        let descriptor = &result["descriptor"];
        assert_eq!(descriptor.as_object().unwrap().len(), 6);
        let mut request = f.request.clone();
        for field in ["artId", "artVersion", "packageDigest", "sdkVersion"] {
            assert_eq!(descriptor[field], request["binding"][field]);
            request["binding"][field] = descriptor[field].clone();
        }
        request["permissions"] = descriptor["permissions"].clone();
        request["services"] = descriptor["services"].clone();
        assert_eq!(f.prepare(request.clone()).0, 200);
        assert_eq!(f.approve(1, false).0, 200);
        assert_eq!(describe(&f, input()).0, 403);
        assert_eq!(f.prepare(request).0, 403); // A saved descriptor cannot bypass revocation.
    }
    #[test]
    fn local_art_describe_returns_approved_subset_and_rejects_scope_above_manifest() {
        let f = Fixture::new();
        assert_eq!(f.approve(0, true).0, 200);
        let (_, policy) = admin(f.port, "GET", "/v1/art/local/approvals", None);
        let mut rows = policy["approvals"].clone();
        rows[0]["permissions"] = json!(["preview"]);
        assert_eq!(
            admin(
                f.port,
                "PUT",
                "/v1/art/local/approvals",
                Some(json!({"expectedRevision":1,"approvals":rows}))
            )
            .0,
            200
        );
        let (status, response) = describe(&f, input());
        assert_eq!(status, 200);
        assert_eq!(response["descriptor"]["permissions"], json!(["preview"]));
        rows[0]["permissions"] = json!(["preview", "loom_service"]);
        rows[0]["services"] = json!(["service:extra"]);
        assert_eq!(
            admin(
                f.port,
                "PUT",
                "/v1/art/local/approvals",
                Some(json!({"expectedRevision":2,"approvals":rows}))
            )
            .0,
            200
        );
        assert_eq!(describe(&f, input()).0, 409);
        assert_eq!(f.approve(3, true).0, 200);
        assert_eq!(describe(&f, input()).0, 200); // Failed source admission released the shared slot.
    }
    #[test]
    fn local_art_describe_rejects_aliases_unknown_fields_and_invalid_sources() {
        let f = Fixture::new();
        assert_eq!(f.approve(0, true).0, 200);
        for art in [
            "local-counter",
            "publisher.test:local-counter",
            "publisher.test/../escape",
            "publisher.test/missing",
        ] {
            assert_ne!(describe(&f, json!({"artId":art})).0, 200);
        }
        assert_eq!(
            describe(
                &f,
                json!({"artId":"publisher.test/local-counter","deviceId":f.device})
            )
            .0,
            400
        );
        assert_ne!(
            device(f.port, &f.token, "GET", "/v1/art/local/describe", None).0,
            200
        );
        drop(f);
        for (entry, digest) in [
            ("../local.js", sha256_bytes(SOURCE)),
            ("local.js", "0".repeat(64)),
        ] {
            let f = Fixture::with_source(entry, &digest);
            assert_eq!(f.approve(0, true).0, 200);
            let (status, response) = describe(&f, input());
            assert_eq!(status, 409, "{response}");
            assert!(response.get("descriptor").is_none());
        }
    }
}
