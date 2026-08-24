// Loom daemon tests fragment 30; included into the shared crate test module.
#[test]
fn installed_javascript_surface_mounts_with_a_verified_entry_resource_and_fallback() {
    let root = unique_temp_dir("javascript-surface-package-mount");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    runtime
        .framework_registry
        .install_framework_package_from_zip(&framework_package_zip("process", "1.0.0"))
        .expect("install process framework");
    let source = br#"NeuroSurface.define({ mount({ root }) { root.textContent = 'price'; } });"#;
    let fallback = json!({
        "protocolVersion": "loom.surface.v1",
        "scene": {
            "id": "root",
            "type": "column",
            "children": [{
                "id": "refresh",
                "type": "button",
                "props": { "label": "刷新" },
                "events": { "click": "refresh_price" }
            }]
        },
        "authoritativeState": { "price": 101.2 }
    });
    loom_tool_registry::install::install_art_from_zip(
        &javascript_surface_art_package_zip("surface-javascript", "1.0.0", source, &fallback),
        &root,
        &runtime.framework_registry,
        &runtime.tool_registry,
    )
    .expect("install JavaScript Surface Art");

    let (status, created) = route_with_runtime(
        &runtime,
        &parsed_request(
            "POST",
            "/v1/surfaces/instances",
            &[],
            Some(&json!({ "artId": "surface-javascript" }).to_string()),
        ),
    )
    .expect("create JavaScript Surface instance");
    assert_eq!(status, 201, "{created}");
    let created: Value = serde_json::from_str(&created).expect("created JSON");
    let instance_id = created["descriptor"]["instanceId"]
        .as_str()
        .expect("instance id");
    let mut host = default_declarative_surface_host_capabilities();
    host.runtimes.push(SurfaceRuntimeKind::Javascript);
    host.transports.push("loom_resource".to_owned());
    host.capabilities.push("surface.javascript.v1".to_owned());
    let attach_path = format!("/v1/surfaces/instances/{instance_id}/attachments");
    let (status, attached) = route_with_runtime(
        &runtime,
        &parsed_request(
            "POST",
            &attach_path,
            &[],
            Some(
                &json!({
                    "hookNodeId": "hook-node:surface-javascript",
                    "deviceId": "device-000-local",
                    "capabilities": host,
                })
                .to_string(),
            ),
        ),
    )
    .expect("attach JavaScript Surface instance");
    assert_eq!(status, 201, "{attached}");
    let attached: Value = serde_json::from_str(&attached).expect("attached JSON");
    let attachment_id = attached["descriptor"]["attachmentId"]
        .as_str()
        .expect("attachment id");

    let mount_path = format!("/v1/surfaces/instances/{instance_id}/mount");
    let (status, mounted) = route_with_runtime(
        &runtime,
        &parsed_request(
            "POST",
            &mount_path,
            &[],
            Some(&json!({ "attachmentId": attachment_id }).to_string()),
        ),
    )
    .expect("mount JavaScript Surface instance");
    assert_eq!(status, 200, "{mounted}");
    let mounted: Value = serde_json::from_str(&mounted).expect("mounted JSON");
    assert_eq!(mounted["runtime"], "javascript");
    assert_eq!(mounted["entry"], "surface/main.js");
    let snapshot = &mounted["instance"]["attachments"][attachment_id]["snapshot"];
    assert_eq!(snapshot["runtime"], "javascript");
    assert_eq!(snapshot["scene"]["children"][0]["id"], "refresh");
    let resource_id = snapshot["entryResourceId"]
        .as_str()
        .expect("entry resource id")
        .to_owned();
    assert_eq!(
        snapshot["resourceLeases"][0]["resource"]["resourceId"],
        resource_id
    );
    assert_eq!(
        snapshot["resourceLeases"][0]["transport"]["kind"],
        "loom_resource"
    );
    let original_lease_id = snapshot["resourceLeases"][0]["leaseId"]
        .as_str()
        .expect("entry lease id")
        .to_owned();
    let digest = resource_id.trim_start_matches("sha256:");
    match route_request(
        &runtime,
        &parsed_request(
            "GET",
            &format!("/v1/surfaces/resources/{digest}"),
            &[("X-Loom-Surface-Lease", original_lease_id.as_str())],
            None,
        ),
    ) {
        RouteResponse::Binary {
            status,
            content_type,
            body,
        } => {
            assert_eq!(status, 200);
            assert_eq!(content_type, "application/javascript");
            assert_eq!(body, source);
        }
        RouteResponse::Text { status, body }
        | RouteResponse::TextWithHeaders { status, body, .. } => {
            panic!("expected JavaScript entry bytes, got {status}: {body}")
        }
    }
    let (status, remounted) = route_with_runtime(
        &runtime,
        &parsed_request(
            "POST",
            &mount_path,
            &[],
            Some(&json!({ "attachmentId": attachment_id }).to_string()),
        ),
    )
    .expect("remount JavaScript Surface instance");
    assert_eq!(status, 200, "{remounted}");
    let remounted: Value = serde_json::from_str(&remounted).expect("remounted JSON");
    let recovered = &remounted["instance"]["attachments"][attachment_id]["snapshot"];
    assert_eq!(recovered["revision"], 2);
    assert_eq!(recovered["entryResourceId"], resource_id);
    assert_eq!(recovered["scene"], snapshot["scene"]);
    let lease_id = recovered["resourceLeases"][0]["leaseId"]
        .as_str()
        .expect("recovered entry lease id")
        .to_owned();
    assert_ne!(lease_id, original_lease_id);
    match route_request(
        &runtime,
        &parsed_request(
            "GET",
            &format!("/v1/surfaces/resources/{digest}"),
            &[("X-Loom-Surface-Lease", lease_id.as_str())],
            None,
        ),
    ) {
        RouteResponse::Binary {
            status,
            content_type,
            body,
        } => {
            assert_eq!(status, 200);
            assert_eq!(content_type, "application/javascript");
            assert_eq!(body, source);
        }
        RouteResponse::Text { status, body }
        | RouteResponse::TextWithHeaders { status, body, .. } => {
            panic!("expected remounted JavaScript entry bytes, got {status}: {body}")
        }
    }
    let lifecycle_path = format!("/v1/surfaces/instances/{instance_id}/lifecycle");
    let (status, disposed) = route_with_runtime(
        &runtime,
        &parsed_request(
            "POST",
            &lifecycle_path,
            &[],
            Some(
                &json!({
                    "protocolVersion": loom_protocol::SURFACE_PROTOCOL_VERSION,
                    "instanceId": instance_id,
                    "attachmentId": attachment_id,
                    "state": "disposed",
                    "revision": 2,
                })
                .to_string(),
            ),
        ),
    )
    .expect("dispose JavaScript Surface attachment");
    assert_eq!(status, 200, "{disposed}");
    match route_request(
        &runtime,
        &parsed_request(
            "GET",
            &format!("/v1/surfaces/resources/{digest}"),
            &[("X-Loom-Surface-Lease", lease_id.as_str())],
            None,
        ),
    ) {
        RouteResponse::Text { status, body }
        | RouteResponse::TextWithHeaders { status, body, .. } => {
            assert_eq!(status, 403, "{body}");
            let body: Value = serde_json::from_str(&body).expect("lease rejection JSON");
            assert_eq!(body["error"]["code"], "surface_resource_lease_rejected");
        }
        RouteResponse::Binary { .. } => panic!("disposed lease still returned bytes"),
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn modular_javascript_surface_sources_are_bounded_and_assembled_in_manifest_order() {
    let root = unique_temp_dir("modular-javascript-surface");
    let art_dir = root.join("arts").join("modular-surface");
    fs::create_dir_all(art_dir.join("surface").join("modules"))
        .expect("create modular Surface directory");
    fs::write(
        art_dir.join("surface/modules/constants.js"),
        b"const answer = 42;",
    )
    .expect("write constants module");
    fs::write(
        art_dir.join("surface/modules/render.js"),
        b"const render = () => answer;",
    )
    .expect("write render module");
    fs::write(
        art_dir.join("surface/main.js"),
        b"NeuroSurface.define({ render });",
    )
    .expect("write JavaScript entry");
    let descriptor_path = art_dir.join("surface/main.js.sources.json");
    let descriptor = json!({
        "schemaVersion": 1,
        "sourceFiles": [
            "surface/modules/constants.js",
            "surface/modules/render.js"
        ]
    });
    fs::write(
        &descriptor_path,
        serde_json::to_vec(&descriptor).expect("serialize source descriptor"),
    )
    .expect("write JavaScript source descriptor");
    let variant: loom_protocol::SurfaceVariant = serde_json::from_value(json!({
        "runtime": "javascript",
        "entry": "surface/main.js"
    }))
    .expect("parse modular JavaScript variant");

    fs::remove_file(&descriptor_path).expect("temporarily remove source descriptor");
    assert_eq!(
        load_surface_javascript_source(&root, &art_dir, &variant)
            .expect("load legacy JavaScript Surface"),
        b"NeuroSurface.define({ render });",
        "a package without a descriptor must preserve the entry bytes exactly"
    );
    fs::write(
        &descriptor_path,
        serde_json::to_vec(&descriptor).expect("restore source descriptor JSON"),
    )
    .expect("restore source descriptor");

    let source = load_surface_javascript_source(&root, &art_dir, &variant)
        .expect("assemble modular JavaScript Surface");
    let source = std::str::from_utf8(&source).expect("assembled source is UTF-8");
    assert!(source.starts_with("(() => {\n\"use strict\";\n"));
    let constants = source.find("const answer = 42;").expect("constants source");
    let render = source
        .find("const render = () => answer;")
        .expect("render source");
    let entry = source
        .find("NeuroSurface.define({ render });")
        .expect("entry source");
    assert!(constants < render && render < entry, "{source}");
    assert!(source.ends_with("\n})();\n"));

    fs::write(
        &descriptor_path,
        serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "sourceFiles": ["surface/main.js"]
        }))
        .expect("serialize repeated-entry descriptor"),
    )
    .expect("write repeated-entry descriptor");
    let error = load_surface_javascript_source(&root, &art_dir, &variant)
        .expect_err("reject entry repeated as a source file");
    assert!(error.to_string().contains("must not repeat entry"), "{error:#}");
    fs::write(root.join("arts/escape.js"), b"const escaped = true;")
        .expect("write out-of-package source");
    fs::write(
        &descriptor_path,
        serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "sourceFiles": ["../escape.js"]
        }))
        .expect("serialize escaping descriptor"),
    )
    .expect("write escaping descriptor");
    let error = load_surface_javascript_source(&root, &art_dir, &variant)
        .expect_err("reject source outside the immutable package");
    assert!(error.to_string().contains("escapes its immutable package"), "{error:#}");
    fs::write(
        &descriptor_path,
        serde_json::to_vec(&descriptor).expect("restore source descriptor JSON"),
    )
    .expect("restore source descriptor");

    fs::write(
        art_dir.join("surface/modules/constants.js"),
        vec![b'x'; MAX_SURFACE_JAVASCRIPT_BYTES as usize],
    )
    .expect("write oversized aggregate source");
    let error = load_surface_javascript_source(&root, &art_dir, &variant)
        .expect_err("reject oversized modular JavaScript Surface");
    assert!(
        error
            .to_string()
            .contains("assembled JavaScript Surface exceeds"),
        "the per-file maximum source must be rejected only after wrapper and entry bytes exceed the aggregate cap: {error:#}"
    );
    let _ = fs::remove_dir_all(root);
}
