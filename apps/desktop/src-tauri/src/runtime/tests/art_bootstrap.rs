//! Bundled Art installation, integrity repair, and MCP relock contracts.

use super::*;

#[test]
fn packaged_art_bootstrap_installs_catalog_once() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let previous_catalog = std::env::var(ART_PACKAGE_CATALOG_ENV).ok();
    std::env::remove_var(ART_PACKAGE_CATALOG_ENV);

    let root = unique_temp_dir("art-bootstrap");
    let desktop_exe = root.join("Loom.exe");
    let catalog_root = root.join("packages").join("arts");
    let control_plane_root = root.join("control-plane");
    fs::create_dir_all(&catalog_root).expect("create Art catalog");
    fs::write(&desktop_exe, b"desktop").expect("write desktop placeholder");

    let mut packages = Vec::new();
    let mut expected_hashes = Vec::new();
    for id in ["sample-a", "sample-b"] {
        let package = format!("independent-{id}").into_bytes();
        let hash = format!("{:x}", Sha256::digest(&package));
        expected_hashes.push(hash.clone());
        let package_path = catalog_root.join(format!("{id}.zip"));
        fs::write(&package_path, &package).expect("write Art package");
        fs::write(
            package_path.with_extension("zip.sha256"),
            format!("{hash}  {id}.zip\n"),
        )
        .expect("write Art checksum");
        packages.push(serde_json::json!({
            "id": id,
            "framework": "process",
            "zip": format!("{id}.zip"),
            "sha256": hash,
        }));
    }
    fs::write(
        catalog_root.join("summary.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "configuration": "Release",
            "packages": packages,
        }))
        .expect("serialize Art catalog"),
    )
    .expect("write Art catalog");

    assert_eq!(
        packaged_art_sha256_allowlist(&desktop_exe).expect("read packaged Art allowlist"),
        expected_hashes,
    );

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind Art bootstrap fixture");
    let address = listener
        .local_addr()
        .expect("read Art bootstrap fixture address");
    let (request_tx, request_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        for (status, body) in [
            (
                "200 OK",
                r#"{"frameworks":[{"id":"process","qualifiedId":"neuro.official/process","installed":true,"enabled":true,"ready":true}]}"#,
            ),
            ("200 OK", r#"{"tool":{"id":"sample-a"}}"#),
            ("200 OK", r#"{"tool":{"id":"sample-b"}}"#),
            (
                "200 OK",
                r#"{"frameworks":[{"id":"process","qualifiedId":"neuro.official/process","installed":true,"enabled":true,"ready":true}]}"#,
            ),
            (
                "404 Not Found",
                r#"{"error":{"code":"not_found","message":"doctor unavailable"}}"#,
            ),
        ] {
            let (mut stream, _) = listener.accept().expect("accept Art bootstrap request");
            request_tx
                .send(read_test_http_request(&mut stream))
                .expect("record Art bootstrap request");
            write_test_json_response(&mut stream, status, body);
        }
    });

    let base_url = format!("http://127.0.0.1:{}", address.port());
    let first = bootstrap_packaged_arts_from_exe(&base_url, &desktop_exe, &control_plane_root)
        .expect("bootstrap packaged Arts");

    assert!(first.available);
    assert!(first.applied);
    assert_eq!(first.framework_ids, vec!["process"]);
    assert_eq!(first.art_ids, vec!["sample-a", "sample-b"]);
    let requests = [
        request_rx.recv().expect("framework listing request"),
        request_rx.recv().expect("first Art install request"),
        request_rx.recv().expect("second Art install request"),
    ];
    assert!(requests[0].starts_with("GET /v1/frameworks HTTP/1.1"));
    assert!(requests[1].starts_with("POST /v1/arts/install HTTP/1.1"));
    assert!(requests[2].starts_with("POST /v1/arts/install HTTP/1.1"));
    assert!(requests[1].contains("\"bundledCatalog\":true"));
    assert!(requests[2].contains("\"bundledCatalog\":true"));

    let second = bootstrap_packaged_arts_from_exe(&base_url, &desktop_exe, &control_plane_root)
        .expect("skip previously applied catalog");
    server.join().expect("join Art bootstrap fixture");
    assert!(second.available);
    assert!(!second.applied);
    assert_eq!(second.catalog_hash, first.catalog_hash);
    assert!(request_rx
        .recv()
        .expect("second framework listing request")
        .starts_with("GET /v1/frameworks HTTP/1.1"));
    assert!(request_rx
        .recv()
        .expect("second Art doctor request")
        .starts_with("GET /v1/doctor/arts HTTP/1.1"));

    fs::remove_dir_all(root).expect("cleanup Art bootstrap fixture");
    restore_env(ART_PACKAGE_CATALOG_ENV, previous_catalog);
}

#[test]
fn packaged_art_bootstrap_repairs_invalid_lock_without_restoring_uninstalled_art() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let previous_art_catalog = std::env::var(ART_PACKAGE_CATALOG_ENV).ok();
    let previous_mcp_catalog = std::env::var(MCP_SERVER_PACKAGE_CATALOG_ENV).ok();
    std::env::remove_var(ART_PACKAGE_CATALOG_ENV);
    std::env::remove_var(MCP_SERVER_PACKAGE_CATALOG_ENV);

    let root = unique_temp_dir("art-integrity-repair-bootstrap");
    let desktop_exe = root.join("Loom.exe");
    let catalog_root = root.join("packages").join("arts");
    let control_plane_root = root.join("control-plane");
    fs::create_dir_all(&catalog_root).expect("create Art catalog");
    fs::write(&desktop_exe, b"desktop").expect("write desktop placeholder");

    let mut packages = Vec::new();
    let mut stock_package = Vec::new();
    for (id, package) in [
        ("custom-stock-monitor", b"stock-monitor-art".as_slice()),
        ("user-uninstalled", b"user-uninstalled-art".as_slice()),
    ] {
        let hash = format!("{:x}", Sha256::digest(package));
        let package_path = catalog_root.join(format!("{id}.zip"));
        fs::write(&package_path, package).expect("write Art package");
        fs::write(
            package_path.with_extension("zip.sha256"),
            format!("{hash}  {id}.zip\n"),
        )
        .expect("write Art checksum");
        packages.push(serde_json::json!({
            "id": id,
            "framework": "mcp",
            "zip": format!("{id}.zip"),
            "sha256": hash,
        }));
        if id == "custom-stock-monitor" {
            stock_package = package.to_vec();
        }
    }
    let catalog = serde_json::to_vec_pretty(&serde_json::json!({
        "configuration": "Release",
        "packages": packages,
    }))
    .expect("serialize Art catalog");
    fs::write(catalog_root.join("summary.json"), &catalog).expect("write Art catalog");
    let marker_path = control_plane_root
        .join("migrations")
        .join("packaged-arts.sha256");
    fs::create_dir_all(marker_path.parent().expect("marker parent"))
        .expect("create packaged Art migration directory");
    fs::write(&marker_path, format!("{:x}\n", Sha256::digest(&catalog)))
        .expect("write previously applied Art catalog marker");

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind repair fixture");
    let address = listener.local_addr().expect("read repair fixture address");
    let (request_tx, request_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        for (status, body) in [
            (
                "200 OK",
                r#"{"frameworks":[{"id":"mcp","qualifiedId":"neuro.official/mcp","installed":true,"enabled":true,"ready":true,"version":"0.2.3"}]}"#,
            ),
            (
                "200 OK",
                r#"{"arts":[{"id":"custom-stock-monitor","qualifiedId":"neuro.official/custom-stock-monitor","lockfileValid":false,"packageDetail":"locked MCP dependency `neuro.official/stock-api` is unavailable or has changed"}]}"#,
            ),
            ("200 OK", r#"{"tool":{"id":"custom-stock-monitor"}}"#),
        ] {
            let (mut stream, _) = listener.accept().expect("accept repair request");
            request_tx
                .send(read_test_http_request(&mut stream))
                .expect("record repair request");
            write_test_json_response(&mut stream, status, body);
        }
    });

    let base_url = format!("http://127.0.0.1:{}", address.port());
    let result = bootstrap_packaged_arts_from_exe(&base_url, &desktop_exe, &control_plane_root)
        .expect("repair stale packaged Art lock");
    server.join().expect("join repair fixture");

    assert!(result.available);
    assert!(result.applied);
    assert_eq!(
        result.art_ids,
        vec!["custom-stock-monitor", "user-uninstalled"]
    );
    let requests = [
        request_rx.recv().expect("framework listing request"),
        request_rx.recv().expect("Art doctor request"),
        request_rx.recv().expect("Art repair request"),
    ];
    assert!(requests[0].starts_with("GET /v1/frameworks HTTP/1.1"));
    assert!(requests[1].starts_with("GET /v1/doctor/arts HTTP/1.1"));
    assert!(requests[2].starts_with("POST /v1/arts/install HTTP/1.1"));
    assert!(requests[2].contains(&base64_encode(&stock_package)));
    assert!(!requests[2].contains(&base64_encode(b"user-uninstalled-art")));

    fs::remove_dir_all(root).expect("cleanup repair fixture");
    restore_env(ART_PACKAGE_CATALOG_ENV, previous_art_catalog);
    restore_env(MCP_SERVER_PACKAGE_CATALOG_ENV, previous_mcp_catalog);
}

#[test]
fn packaged_art_bootstrap_relocks_arts_after_mcp_catalog_change() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let previous_art_catalog = std::env::var(ART_PACKAGE_CATALOG_ENV).ok();
    let previous_mcp_catalog = std::env::var(MCP_SERVER_PACKAGE_CATALOG_ENV).ok();
    std::env::remove_var(ART_PACKAGE_CATALOG_ENV);
    std::env::remove_var(MCP_SERVER_PACKAGE_CATALOG_ENV);

    let root = unique_temp_dir("mcp-change-art-relock-bootstrap");
    let desktop_exe = root.join("Loom.exe");
    let art_catalog_root = root.join("packages").join("arts");
    let mcp_catalog_root = root.join("packages").join("mcp-servers");
    let control_plane_root = root.join("control-plane");
    fs::create_dir_all(&art_catalog_root).expect("create Art catalog");
    fs::create_dir_all(&mcp_catalog_root).expect("create MCP catalog");
    fs::write(&desktop_exe, b"desktop").expect("write desktop placeholder");

    let mcp_package = b"changed-stock-api-package";
    let mcp_hash = format!("{:x}", Sha256::digest(mcp_package));
    let mcp_package_path = mcp_catalog_root.join("stock-api.zip");
    fs::write(&mcp_package_path, mcp_package).expect("write MCP package");
    fs::write(
        mcp_package_path.with_extension("zip.sha256"),
        format!("{mcp_hash}  stock-api.zip\n"),
    )
    .expect("write MCP checksum");
    fs::write(
        mcp_catalog_root.join("summary.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "servers": [{
                "id": "stock-api",
                "qualifiedId": "neuro.official/stock-api",
                "version": "2.9.0",
                "zip": "stock-api.zip",
                "sha256": mcp_hash,
            }],
        }))
        .expect("serialize MCP catalog"),
    )
    .expect("write MCP catalog");

    let art_package = b"custom-stock-monitor-art";
    let art_hash = format!("{:x}", Sha256::digest(art_package));
    let art_package_path = art_catalog_root.join("custom-stock-monitor.zip");
    fs::write(&art_package_path, art_package).expect("write Art package");
    fs::write(
        art_package_path.with_extension("zip.sha256"),
        format!("{art_hash}  custom-stock-monitor.zip\n"),
    )
    .expect("write Art checksum");
    let uninstalled_art_package = b"user-uninstalled-art";
    let uninstalled_art_hash = format!("{:x}", Sha256::digest(uninstalled_art_package));
    let uninstalled_art_path = art_catalog_root.join("user-uninstalled.zip");
    fs::write(&uninstalled_art_path, uninstalled_art_package)
        .expect("write uninstalled Art package");
    fs::write(
        uninstalled_art_path.with_extension("zip.sha256"),
        format!("{uninstalled_art_hash}  user-uninstalled.zip\n"),
    )
    .expect("write uninstalled Art checksum");
    let art_catalog = serde_json::to_vec_pretty(&serde_json::json!({
        "configuration": "Release",
        "packages": [
            {
                "id": "custom-stock-monitor",
                "framework": "mcp",
                "zip": "custom-stock-monitor.zip",
                "sha256": art_hash,
            },
            {
                "id": "user-uninstalled",
                "framework": "mcp",
                "zip": "user-uninstalled.zip",
                "sha256": uninstalled_art_hash,
            },
        ],
    }))
    .expect("serialize Art catalog");
    fs::write(art_catalog_root.join("summary.json"), &art_catalog).expect("write Art catalog");
    let art_marker_path = control_plane_root
        .join("migrations")
        .join("packaged-arts.sha256");
    fs::create_dir_all(art_marker_path.parent().expect("marker parent"))
        .expect("create packaged Art migration directory");
    fs::write(
        &art_marker_path,
        format!("{:x}\n", Sha256::digest(&art_catalog)),
    )
    .expect("write previously applied Art catalog marker");

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind MCP relock fixture");
    let address = listener
        .local_addr()
        .expect("read MCP relock fixture address");
    let (request_tx, request_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        for (status, body) in [
            ("200 OK", r#"{"server":{"id":"stock-api"}}"#),
            (
                "200 OK",
                r#"{"frameworks":[{"id":"mcp","qualifiedId":"neuro.official/mcp","installed":true,"enabled":true,"ready":true,"version":"0.2.3"}]}"#,
            ),
            (
                "200 OK",
                r#"{"arts":[{"id":"custom-stock-monitor","qualifiedId":"neuro.official/custom-stock-monitor","lockfileValid":false}]}"#,
            ),
            ("200 OK", r#"{"tool":{"id":"custom-stock-monitor"}}"#),
        ] {
            let (mut stream, _) = listener.accept().expect("accept MCP relock request");
            request_tx
                .send(read_test_http_request(&mut stream))
                .expect("record MCP relock request");
            write_test_json_response(&mut stream, status, body);
        }
    });

    let base_url = format!("http://127.0.0.1:{}", address.port());
    let result = bootstrap_packaged_arts_from_exe(&base_url, &desktop_exe, &control_plane_root)
        .expect("relock Art after MCP catalog change");
    server.join().expect("join MCP relock fixture");

    assert!(result.applied);
    let requests = [
        request_rx.recv().expect("MCP install request"),
        request_rx.recv().expect("framework listing request"),
        request_rx.recv().expect("Art doctor request"),
        request_rx.recv().expect("Art reinstall request"),
    ];
    assert!(requests[0].starts_with("POST /v1/mcp/servers/install HTTP/1.1"));
    assert!(requests[1].starts_with("GET /v1/frameworks HTTP/1.1"));
    assert!(requests[2].starts_with("GET /v1/doctor/arts HTTP/1.1"));
    assert!(requests[3].starts_with("POST /v1/arts/install HTTP/1.1"));
    assert!(requests[3].contains(&base64_encode(art_package)));
    assert!(!requests[3].contains(&base64_encode(uninstalled_art_package)));
    assert!(control_plane_root
        .join("migrations/packaged-mcp-servers.sha256")
        .is_file());

    fs::remove_dir_all(root).expect("cleanup MCP relock fixture");
    restore_env(ART_PACKAGE_CATALOG_ENV, previous_art_catalog);
    restore_env(MCP_SERVER_PACKAGE_CATALOG_ENV, previous_mcp_catalog);
}
