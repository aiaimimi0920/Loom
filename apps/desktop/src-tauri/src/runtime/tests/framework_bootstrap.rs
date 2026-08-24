//! Framework upgrade and legacy fallback package contracts.

use super::*;

#[test]
fn packaged_art_bootstrap_upgrades_changed_framework_version_after_art_catalog_applied() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let previous_art_catalog = std::env::var(ART_PACKAGE_CATALOG_ENV).ok();
    let previous_framework_catalog = std::env::var(FRAMEWORK_PACKAGE_CATALOG_ENV).ok();
    std::env::remove_var(ART_PACKAGE_CATALOG_ENV);
    std::env::remove_var(FRAMEWORK_PACKAGE_CATALOG_ENV);

    let root = unique_temp_dir("framework-upgrade-bootstrap");
    let desktop_exe = root.join("Loom.exe");
    let art_catalog_root = root.join("packages").join("arts");
    let framework_catalog_root = root.join("packages").join("frameworks");
    let control_plane_root = root.join("control-plane");
    fs::create_dir_all(&art_catalog_root).expect("create Art catalog");
    fs::create_dir_all(&framework_catalog_root).expect("create Framework catalog");
    fs::write(&desktop_exe, b"desktop").expect("write desktop placeholder");

    let framework_package = b"mcp-framework-0.2.2";
    let framework_hash = format!("{:x}", Sha256::digest(framework_package));
    let framework_package_path = framework_catalog_root.join("mcp.zip");
    fs::write(&framework_package_path, framework_package).expect("write Framework package");
    fs::write(
        framework_package_path.with_extension("zip.sha256"),
        format!("{framework_hash}  mcp.zip\n"),
    )
    .expect("write Framework checksum");
    fs::write(
        framework_catalog_root.join("summary.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "configuration": "Release",
            "frameworks": [{
                "id": "mcp",
                "version": "0.2.2",
            }],
        }))
        .expect("serialize Framework catalog"),
    )
    .expect("write Framework catalog");

    let art_package = b"custom-image-search-art";
    let art_hash = format!("{:x}", Sha256::digest(art_package));
    let art_package_path = art_catalog_root.join("custom-image-search.zip");
    fs::write(&art_package_path, art_package).expect("write Art package");
    fs::write(
        art_package_path.with_extension("zip.sha256"),
        format!("{art_hash}  custom-image-search.zip\n"),
    )
    .expect("write Art checksum");
    let art_catalog = serde_json::to_vec_pretty(&serde_json::json!({
        "configuration": "Release",
        "packages": [{
            "id": "custom-image-search",
            "framework": "mcp",
            "zip": "custom-image-search.zip",
            "sha256": art_hash,
        }],
    }))
    .expect("serialize Art catalog");
    fs::write(art_catalog_root.join("summary.json"), &art_catalog).expect("write Art catalog");
    let marker_path = control_plane_root
        .join("migrations")
        .join("packaged-arts.sha256");
    fs::create_dir_all(marker_path.parent().expect("marker parent"))
        .expect("create packaged Art migration directory");
    fs::write(
        &marker_path,
        format!("{:x}\n", Sha256::digest(&art_catalog)),
    )
    .expect("write previously applied Art catalog marker");

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind upgrade fixture");
    let address = listener.local_addr().expect("read upgrade fixture address");
    let (request_tx, request_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        for (status, body) in [
            (
                "200 OK",
                r#"{"frameworks":[{"id":"mcp","qualifiedId":"neuro.official/mcp","installed":true,"enabled":true,"ready":true,"version":"0.2.1"}]}"#,
            ),
            (
                "200 OK",
                r#"{"framework":{"id":"mcp","installed":true,"ready":true,"version":"0.2.2"}}"#,
            ),
            ("200 OK", r#"{"tool":{"id":"custom-image-search"}}"#),
        ] {
            let (mut stream, _) = listener.accept().expect("accept upgrade request");
            request_tx
                .send(read_test_http_request(&mut stream))
                .expect("record upgrade request");
            write_test_json_response(&mut stream, status, body);
        }
    });

    let base_url = format!("http://127.0.0.1:{}", address.port());
    let result = bootstrap_packaged_arts_from_exe(&base_url, &desktop_exe, &control_plane_root)
        .expect("upgrade packaged Framework and reinstall Art");
    server.join().expect("join upgrade fixture");

    assert!(result.available);
    assert!(result.applied);
    assert_eq!(result.framework_ids, vec!["mcp"]);
    assert_eq!(result.art_ids, vec!["custom-image-search"]);
    let requests = [
        request_rx.recv().expect("framework listing request"),
        request_rx.recv().expect("framework upgrade request"),
        request_rx.recv().expect("Art reinstall request"),
    ];
    assert!(requests[0].starts_with("GET /v1/frameworks HTTP/1.1"));
    assert!(requests[1].starts_with("POST /v1/frameworks/mcp/upgrade HTTP/1.1"));
    assert!(requests[1].contains("\"zipBase64\""));
    assert!(requests[2].starts_with("POST /v1/arts/install HTTP/1.1"));
    assert!(requests[2].contains("\"bundledCatalog\":true"));
    assert!(marker_path.is_file());

    fs::remove_dir_all(root).expect("cleanup upgrade fixture");
    restore_env(ART_PACKAGE_CATALOG_ENV, previous_art_catalog);
    restore_env(FRAMEWORK_PACKAGE_CATALOG_ENV, previous_framework_catalog);
}

#[test]
fn packaged_framework_install_falls_back_for_an_old_daemon() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let previous_catalog = std::env::var(FRAMEWORK_PACKAGE_CATALOG_ENV).ok();
    std::env::remove_var(FRAMEWORK_PACKAGE_CATALOG_ENV);

    let root = unique_temp_dir("framework-old-daemon");
    let desktop_exe = root.join("Loom.exe");
    let catalog = root.join("packages").join("frameworks");
    fs::create_dir_all(&catalog).expect("create package catalog");
    fs::write(&desktop_exe, b"desktop").expect("write desktop placeholder");
    let package_path = catalog.join("cloud_api.zip");
    let package = b"independent-cloud-api-framework";
    fs::write(&package_path, package).expect("write framework package");
    let hash = format!("{:x}", Sha256::digest(package));
    fs::write(
        package_path.with_extension("zip.sha256"),
        format!("{hash}  cloud_api.zip\n"),
    )
    .expect("write framework checksum");

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind old daemon fixture");
    let address = listener.local_addr().expect("read old daemon address");
    let (request_tx, request_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        for (status, body) in [
            (
                "500 Internal Server Error",
                r#"{"error":{"code":"framework_install_failed","message":"framework `cloud_api` has no configured runtime download source (set LOOM_ART_STORE_URL or LOOM_FRAMEWORK_RUNTIME_URL)"}}"#,
            ),
            (
                "200 OK",
                r#"{"framework":{"id":"cloud_api","installed":true,"ready":true}}"#,
            ),
        ] {
            let (mut stream, _) = listener.accept().expect("accept install request");
            request_tx
                .send(read_test_http_request(&mut stream))
                .expect("record install request");
            write_test_json_response(&mut stream, status, body);
        }
    });

    let response = install_packaged_framework_from_exe(
        &format!("http://127.0.0.1:{}", address.port()),
        "cloud_api",
        &desktop_exe,
    )
    .expect("packaged fallback install");
    server.join().expect("join old daemon fixture");
    let first_request = request_rx.recv().expect("first install request");
    let second_request = request_rx.recv().expect("fallback install request");

    assert!(first_request.starts_with("POST /v1/frameworks/cloud_api/install HTTP/1.1"));
    assert!(second_request.starts_with("POST /v1/frameworks/install HTTP/1.1"));
    let fallback_body: Value = serde_json::from_str(
        second_request
            .split_once("\r\n\r\n")
            .expect("fallback request body")
            .1,
    )
    .expect("fallback request json");
    assert_eq!(fallback_body["zipBase64"], base64_encode(package));
    assert_eq!(response["framework"]["installed"], true);
    assert_eq!(response["framework"]["ready"], true);

    fs::remove_dir_all(root).expect("cleanup old daemon fixture");
    restore_env(FRAMEWORK_PACKAGE_CATALOG_ENV, previous_catalog);
}

#[test]
fn packaged_framework_install_does_not_mask_other_daemon_errors() {
    let root = unique_temp_dir("framework-daemon-error");
    let desktop_exe = root.join("Loom.exe");
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind daemon error fixture");
    let address = listener.local_addr().expect("read daemon error address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept failed install request");
        let _ = read_test_http_request(&mut stream);
        write_test_json_response(
            &mut stream,
            "500 Internal Server Error",
            r#"{"error":{"code":"framework_install_failed","message":"framework runtime self-test failed"}}"#,
        );
    });

    let error = install_packaged_framework_from_exe(
        &format!("http://127.0.0.1:{}", address.port()),
        "cloud_api",
        &desktop_exe,
    )
    .expect_err("non-source error must remain visible");
    server.join().expect("join daemon error fixture");

    assert!(error.contains("runtime self-test failed"), "{error}");
    fs::remove_dir_all(root).expect("cleanup daemon error fixture");
}
