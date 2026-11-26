// Full HTTP admission for package resources; uses independent fixed PNG/pixel oracles rather than deriving expectations from rendered output under test.
mod package_http {
    use super::*;
    pub(super) const PNG:&str="iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAFElEQVR4nGP4z8DwHwyBNBAwNAAAR0sIeRPxYNAAAAAASUVORK5CYII=";
    pub(super) const SOURCE:&[u8]=br#"NeuroSurface.define({mount({root,snapshot}){const p=snapshot.state.package;if(NeuroSurface.resource(p.config.resourceId)!=='{"answer":42}')throw Error('config');if(NeuroSurface.resource(p.data.resourceId)!=='data:application/octet-stream;base64,AP8=')throw Error('bytes');const im=NeuroSurface.image(p.pixel.resourceId),c=document.createElement('canvas');c.id='package-image';c.width=im.width;c.height=im.height;root.append(c);c.getContext('2d').drawImage(im,0,0);NeuroSurface.commitResult({answer:{kind:'value',value:packageAnswer()},image:{kind:'canvas',nodeId:'package-image'}})}});"#;
    pub(super) fn zip(change: impl FnOnce(&mut Value)) -> Vec<u8> {
        let assembled = [
            b"(() => {\n\"use strict\";\n".as_slice(),
            b"function packageAnswer(){return 42;}\n;\n",
            SOURCE,
            b"\n;\n\n})();\n",
        ]
        .concat();
        let original = package_scoped("local.js", &sha256_bytes(&assembled), &[], SOURCE, true);
        let mut archive = zip::ZipArchive::new(Cursor::new(original)).unwrap();
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let resources = [
            (
                "config",
                "assets/config.json",
                "application/json",
                br#"{"answer":42}"#.to_vec(),
                [0, 0],
            ),
            (
                "data",
                "assets/data.bin",
                "application/octet-stream",
                vec![0, 255],
                [0, 0],
            ),
            (
                "pixel",
                "assets/pixel.png",
                "image/png",
                BASE64.decode(PNG).unwrap(),
                [2, 2],
            ),
        ];
        let mut change = Some(change);
        for i in 0..archive.len() {
            let mut e = archive.by_index(i).unwrap();
            let mut bytes = Vec::new();
            e.read_to_end(&mut bytes).unwrap();
            if e.name().unwrap().as_ref() == "manifest.json" {
                let mut m: Value = serde_json::from_slice(&bytes).unwrap();
                m["metadata"]["otterExecution"]["resources"]=json!(resources.iter().map(|(id,path,mime,bytes,dim)|json!({"entry":path,"resource":{"id":id,"mimeType":mime,"size":bytes.len(),"digest":sha256_bytes(bytes),"dimensions":dim}})).collect::<Vec<_>>());
                change.take().unwrap()(&mut m);
                bytes = serde_json::to_vec(&m).unwrap();
            }
            writer
                .start_file(e.name().unwrap(), zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        for (path, bytes) in [
            (
                "helper.js",
                b"function packageAnswer(){return 42;}".to_vec(),
            ),
            (
                "local.js.sources.json",
                br#"{"schemaVersion":1,"sourceFiles":["helper.js"]}"#.to_vec(),
            ),
        ]
        .into_iter()
        .chain(
            resources
                .into_iter()
                .map(|(_, path, _, bytes, _)| (path, bytes)),
        ) {
            writer
                .start_file(path, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }
    pub(super) fn fixture() -> Fixture {
        let mut f = Fixture::build_package(&zip(|_| {}), false);
        f.request["permissions"]
            .as_array_mut()
            .unwrap()
            .push(json!("workspace_result"));
        f
    }
    pub(super) fn prepare(f: &Fixture) -> (u16, Value) {
        let mut r = f.request.clone();
        r["protocolVersion"] = loom_protocol::local_art::package::visibility::PREPARE.into();
        r["knownSourceDigest"] = "0".repeat(64).into();
        device(
            f.port,
            &f.token,
            "POST",
            "/v3/art/local/prepare-package",
            Some(r),
        )
    }
    fn resource(f: &Fixture, grant: Value, id: &str) -> (u16, Value) {
        device(
            f.port,
            &f.token,
            "POST",
            "/v1/art/local/resource",
            Some(
                json!({"protocolVersion":loom_protocol::local_art::package::RESOURCE,"grant":grant,"resourceId":id}),
            ),
        )
    }
    #[test]
    fn current_package_http_requires_approval_and_never_omits_resources() {
        let f = fixture();
        assert_eq!(prepare(&f).0, 403);
        assert_eq!(f.approve(0, true).0, 200);
        assert_eq!(f.prepare(f.request.clone()).0, 200);
        let (status, r) = prepare(&f);
        assert_eq!(status, 200, "{r}");
        let proof = loom_plugin_security::local_art_grant::verify_visibility_package(
            &serde_json::to_vec(&r["package"]).unwrap(),
            f.issuer["keyId"].as_str().unwrap(),
            f.issuer["publicKey"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(proof.package.resources.len(), 3);
        let (status, asset) = resource(&f, r["grant"].clone(), "pixel");
        assert_eq!(status, 200, "{asset}");
        assert_eq!(asset["bytes"], PNG);
        assert_eq!(
            resource(&f, r["grant"].clone(), "../assets/pixel.png").0,
            404
        );
        assert_eq!(f.approve(1, false).0, 200);
        assert_eq!(resource(&f, r["grant"].clone(), "pixel").0, 403);
    }
    #[test]
    fn local_art_package_http_rejects_path_digest_size_duplicate_and_budget_manifest() {
        for change in 0..5 {
            let zip = zip(|m| {
                let r = &mut m["metadata"]["otterExecution"]["resources"];
                match change {
                    0 => r[0]["entry"] = "../escape".into(),
                    1 => r[0]["resource"]["digest"] = "0".repeat(64).into(),
                    2 => r[0]["resource"]["size"] = 999.into(),
                    3 => r[1]["resource"]["id"] = "config".into(),
                    _ => r[2]["resource"]["dimensions"] = json!([u32::MAX, u32::MAX]),
                }
            });
            let mut f = Fixture::build_package(&zip, false);
            f.request["permissions"]
                .as_array_mut()
                .unwrap()
                .push(json!("workspace_result"));
            assert_eq!(f.approve(0, true).0, 200);
            assert_eq!(prepare(&f).0, 409, "change {change}");
        }
    }
    #[test]
    fn local_art_package_http_resource_rejects_modified_signature_and_unpaired_device() {
        let f = fixture();
        assert_eq!(f.approve(0, true).0, 200);
        let (status, r) = prepare(&f);
        assert_eq!(status, 200);
        let mut grant = r["grant"].clone();
        grant["signature"] = "A".repeat(88).into();
        assert_eq!(resource(&f, grant, "config").0, 403);
        let (_, other) = pair(f.port, "not package owner");
        assert_eq!(device(f.port,&other,"POST","/v1/art/local/resource",Some(json!({"protocolVersion":loom_protocol::local_art::package::RESOURCE,"grant":r["grant"],"resourceId":"config"}))).0,403);
    }
}
