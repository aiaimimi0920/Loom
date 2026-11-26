// Exact installed package -> Device/approval -> current signed port proof; retired prepare routes are explicitly rejected.
mod ports_http {
    use super::*;
    use loom_protocol::local_art::package::visibility as ports;
    fn prepare(f: &Fixture) -> (u16, Value) {
        let mut request = f.request.clone();
        request["protocolVersion"] = ports::PREPARE.into();
        request["knownSourceDigest"] = "0".repeat(64).into();
        device(f.port, &f.token, "POST", ports::ROUTE, Some(request))
    }
    #[test]
    fn current_port_proof_uses_immutable_manifest_and_rejects_retired_routes() {
        let zip = package_http::zip(|m| {
            // Intentional UTF-8/CJK custom port name literal to verify non-ASCII port proof signing.
            m["inputs"] = json!([{"name":"caption","type":"text"},{"name":"原始图片","data_type":"Image"}]);
            m["outputs"] = json!([{"name":"processed","execution_type":"image"}]);
        });
        let f = Fixture::build_package(&zip, false);
        assert_eq!(prepare(&f).0, 403);
        assert_eq!(f.approve(0, true).0, 200);
        let (status, reply) = prepare(&f);
        assert_eq!(status, 200, "{reply}");
        assert_eq!(reply["protocolVersion"], ports::PREPARE);
        let bytes = serde_json::to_vec(&reply["package"]).unwrap();
        let proof = loom_plugin_security::local_art_grant::verify_visibility_package(&bytes,
            f.issuer["keyId"].as_str().unwrap(), f.issuer["publicKey"].as_str().unwrap()).unwrap();
        assert_eq!(proof.ports.inputs[0].name, "caption");
        assert_eq!(proof.ports.inputs[1].name, "原始图片");
        assert_eq!(proof.ports.inputs[1].kind, "image");
        assert_eq!(proof.ports.outputs[0].name, "processed");
        assert_eq!(proof.package.resources.len(), 3);
        assert_eq!(proof.package.grant.binding.package_digest, f.request["binding"]["packageDigest"]);
        assert!(loom_plugin_security::local_art_grant::verify_package(&bytes,
            f.issuer["keyId"].as_str().unwrap(), f.issuer["publicKey"].as_str().unwrap()).is_err());
        for route in ["/v1/art/local/prepare", "/v1/art/local/prepare-cached", "/v1/art/local/prepare-resumable", "/v1/art/local/prepare-package", "/v2/art/local/prepare-package"] {
            // All retired prepare paths are denied Device admission and are never restored.
            assert_eq!(device(f.port, &f.token, "POST", route, Some(f.request.clone())).0, 403, "{route}");
        }
        assert_eq!(f.approve(1, false).0, 200);
        assert_eq!(prepare(&f).0, 403);
    }
    #[test]
    fn invalid_port_declarations_fail_current_protocol_without_alternate_route() {
        for inputs in [
            json!([{"name":"same","type":"image"},{"name":"same","type":"image"}]),
            json!([{"name":" padded ","type":"image"}]),
            json!([{"name":"bad\nname","type":"image"}]),
            json!([{"type":"image"}]),
            json!((0..33).map(|i| json!({"name":format!("p{i}"),"type":"image"})).collect::<Vec<_>>()),
        ] {
            let zip = package_http::zip(|m| m["inputs"] = inputs);
            let f = Fixture::build_package(&zip, false);
            assert_eq!(f.approve(0, true).0, 200);
            let (status, reply) = prepare(&f);
            assert_eq!(status, 409, "{reply}");
            assert_eq!(reply["error"]["code"], "local_art_ports_invalid");
            assert_eq!(package_http::prepare(&f).0, 409);
        }
    }
    #[test]
    fn signed_visibility_uses_immutable_defaults_per_direction() {
        use loom_protocol::local_art::package::visibility as v3;
        let zip = package_http::zip(|m| {
            m["inputs"] = json!([
                {"name":"same","type":"image","defaultVisible":false},
                {"name":"fallback","type":"text"},
                {"name":"explicit","type":"text","defaultVisible":true},
                {"name":"malformed","type":"text","defaultVisible":"no"},
                {"name":"unspecified","type":"text"}
            ]);
            m["outputs"] = json!([{"name":"same","type":"image","defaultVisible":true}]);
            m["metadata"]["defaultVisibility"] = json!({"same":true,"fallback":false,"explicit":false,"malformed":false});
        });
        let f = Fixture::build_package(&zip, false);
        let mut input = f.request.clone();
        input["protocolVersion"] = v3::PREPARE.into();
        input["knownSourceDigest"] = "0".repeat(64).into();
        assert_eq!(device(f.port, &f.token, "POST", v3::ROUTE, Some(input.clone())).0, 403);
        assert_eq!(f.approve(0, true).0, 200);
        let (status, response) = device(f.port, &f.token, "POST", v3::ROUTE, Some(input.clone()));
        assert_eq!(status, 200, "{response}");
        assert_eq!(response["protocolVersion"], v3::PREPARE);
        let bytes = serde_json::to_vec(&response["package"]).unwrap();
        let proof = loom_plugin_security::local_art_grant::verify_visibility_package(&bytes,
            f.issuer["keyId"].as_str().unwrap(), f.issuer["publicKey"].as_str().unwrap()).unwrap();
        assert_eq!(proof.visibility.inputs, vec![false, true, true, true, true]);
        assert_eq!(proof.visibility.outputs, vec![true]);
        assert_eq!(proof.package.resources.len(), 3);
        assert_eq!(proof.package.grant.binding.package_digest, f.request["binding"]["packageDigest"]);
        assert_eq!(serde_json::to_value(&proof.ports.inputs[0]).unwrap(), json!({"name":"same","type":"image"}));
        assert_eq!(f.approve(1, false).0, 200);
        assert_eq!(device(f.port, &f.token, "POST", v3::ROUTE, Some(input)).0, 403);
    }
    #[test]
    fn retired_metadata_is_not_consulted_for_current_visibility() {
        use loom_protocol::local_art::package::visibility as v3;
        let zip = package_http::zip(|m| {
            m["inputs"] = json!([{"name":"explicit","type":"image","defaultVisible":false},
                {"name":"fallback","type":"text"}]);
            m["outputs"] = json!([]);
            m["metadata"]["defaultVisibility"] = json!({"explicit":true,"fallback":false});
        });
        let f = Fixture::build_package(&zip, false);
        assert_eq!(f.approve(0, true).0, 200);
        let mut input = f.request.clone();
        input["protocolVersion"] = v3::PREPARE.into();
        input["knownSourceDigest"] = "0".repeat(64).into();
        let (status, response) = device(f.port, &f.token, "POST", v3::ROUTE, Some(input));
        assert_eq!(status, 200, "{response}");
        let proof = loom_plugin_security::local_art_grant::verify_visibility_package(&serde_json::to_vec(&response["package"]).unwrap(),
            f.issuer["keyId"].as_str().unwrap(), f.issuer["publicKey"].as_str().unwrap()).unwrap();
        assert_eq!(proof.visibility.inputs, vec![false, true]);
        assert!(proof.visibility.outputs.is_empty());
    }
}
