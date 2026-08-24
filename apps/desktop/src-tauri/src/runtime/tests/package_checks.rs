//! Bundled package paths, checksum validation, and MCP bootstrap contracts.

use super::*;

#[test]
fn packaged_framework_path_uses_release_catalog() {
    let desktop_exe = Path::new(r"C:\Release\Loom.exe");

    assert_eq!(
        packaged_framework_package_path(desktop_exe, "cloud_api"),
        PathBuf::from(r"C:\Release\packages\frameworks\cloud_api.zip")
    );
}

#[test]
fn packaged_framework_checksum_mismatch_is_rejected() {
    let root = unique_temp_dir("framework-checksum");
    let package_path = root.join("cloud_api.zip");
    fs::write(&package_path, b"framework-package").expect("write package");
    fs::write(
        package_path.with_extension("zip.sha256"),
        format!("{}  cloud_api.zip\n", "0".repeat(64)),
    )
    .expect("write checksum");

    let error = read_verified_framework_package("cloud_api", &package_path)
        .expect_err("mismatched checksum must fail");

    assert!(error.contains("SHA-256 不匹配"), "{error}");
    fs::remove_dir_all(root).expect("cleanup checksum fixture");
}

#[test]
fn packaged_art_checksum_mismatch_is_rejected() {
    let root = unique_temp_dir("art-checksum");
    let package_path = root.join("sample-art.zip");
    fs::write(&package_path, b"art-package").expect("write package");
    fs::write(
        package_path.with_extension("zip.sha256"),
        format!("{}  sample-art.zip\n", "0".repeat(64)),
    )
    .expect("write checksum");

    let error = read_verified_art_package("sample-art", &package_path)
        .expect_err("mismatched checksum must fail");

    assert!(error.contains("SHA-256 不匹配"), "{error}");
    fs::remove_dir_all(root).expect("cleanup checksum fixture");
}

#[test]
fn verified_package_rejects_growth_and_oversized_checksum_inputs() {
    let root = unique_temp_dir("bounded-package");
    let package_path = root.join("bounded.zip");
    let package = fs::File::create(&package_path).expect("create sparse package");
    package.set_len(9).expect("grow sparse package");
    let error = read_verified_package("测试", "bounded", &package_path, 8)
        .expect_err("oversized package must fail before allocation");
    assert!(error.contains("超过 8 字节限制"), "{error}");

    fs::write(&package_path, b"small").expect("replace package");
    fs::write(package_path.with_extension("zip.sha256"), vec![b'a'; 4097])
        .expect("write oversized checksum");
    let error = read_verified_package("测试", "bounded", &package_path, 8)
        .expect_err("oversized checksum must fail");
    assert!(error.contains("超过 4096 字节限制"), "{error}");
    fs::remove_dir_all(root).expect("cleanup bounded package fixture");
}

#[test]
fn packaged_framework_upgrade_rejects_path_traversal_id() {
    let error = upgrade_packaged_framework_from_exe(
        "http://127.0.0.1:1",
        "../escape",
        Path::new(r"C:\Release\Loom.exe"),
    )
    .expect_err("traversal framework id must fail before package lookup");
    assert!(error.contains("框架 ID 无效"), "{error}");
}

#[test]
fn packaged_mcp_server_bootstrap_installs_once_and_respects_uninstall_marker() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let previous_catalog = std::env::var(MCP_SERVER_PACKAGE_CATALOG_ENV).ok();
    std::env::remove_var(MCP_SERVER_PACKAGE_CATALOG_ENV);
    let root = unique_temp_dir("mcp-server-bootstrap");
    let desktop_exe = root.join("Loom.exe");
    let catalog_root = root.join("packages").join("mcp-servers");
    let control_plane_root = root.join("control-plane");
    fs::create_dir_all(&catalog_root).expect("create MCP catalog");
    fs::write(&desktop_exe, b"desktop").expect("write desktop placeholder");
    let image_package = b"independent-image-search-mcp-package";
    let image_hash = format!("{:x}", Sha256::digest(image_package));
    let image_package_path = catalog_root.join("neuro-image-search.zip");
    fs::write(&image_package_path, image_package).expect("write image-search MCP package");
    fs::write(
        image_package_path.with_extension("zip.sha256"),
        format!("{image_hash}  neuro-image-search.zip\n"),
    )
    .expect("write image-search MCP checksum");
    let stock_package = b"independent-stock-api-mcp-package";
    let stock_hash = format!("{:x}", Sha256::digest(stock_package));
    let stock_package_path = catalog_root.join("stock-api.zip");
    fs::write(&stock_package_path, stock_package).expect("write stock-api MCP package");
    fs::write(
        stock_package_path.with_extension("zip.sha256"),
        format!("{stock_hash}  stock-api.zip\n"),
    )
    .expect("write stock-api MCP checksum");
    fs::write(
        catalog_root.join("summary.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "servers": [
                {
                    "id": "neuro-image-search",
                    "qualifiedId": "neuro.official/neuro-image-search",
                    "version": "0.1.0",
                    "zip": "neuro-image-search.zip",
                    "sha256": image_hash,
                },
                {
                    "id": "stock-api",
                    "qualifiedId": "neuro.official/stock-api",
                    "version": "2.9.0",
                    "zip": "stock-api.zip",
                    "sha256": stock_hash,
                }
            ]
        }))
        .expect("serialize MCP catalog"),
    )
    .expect("write MCP catalog");

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind MCP bootstrap fixture");
    let address = listener
        .local_addr()
        .expect("read MCP bootstrap fixture address");
    let (request_tx, request_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        for id in ["neuro-image-search", "stock-api"] {
            let (mut stream, _) = listener.accept().expect("accept MCP install request");
            request_tx
                .send(read_test_http_request(&mut stream))
                .expect("record MCP install request");
            write_test_json_response(
                &mut stream,
                "200 OK",
                &format!(r#"{{"server":{{"id":"{id}"}}}}"#),
            );
        }
    });
    let base_url = format!("http://127.0.0.1:{}", address.port());
    let installed = bootstrap_packaged_mcp_servers(&base_url, &desktop_exe, &control_plane_root)
        .expect("bootstrap packaged MCP server");
    server.join().expect("join MCP bootstrap fixture");
    assert_eq!(installed, vec!["neuro-image-search", "stock-api"]);
    for request in [
        request_rx.recv().expect("image-search MCP install request"),
        request_rx.recv().expect("stock-api MCP install request"),
    ] {
        assert!(request.starts_with("POST /v1/mcp/servers/install HTTP/1.1"));
        assert!(request.contains("\"zipBase64\""));
    }
    assert!(control_plane_root
        .join("migrations/packaged-mcp-servers.sha256")
        .is_file());

    let skipped =
        bootstrap_packaged_mcp_servers("http://127.0.0.1:1", &desktop_exe, &control_plane_root)
            .expect("same MCP catalog must not reinstall after user removal");
    assert!(skipped.is_empty());
    restore_env(MCP_SERVER_PACKAGE_CATALOG_ENV, previous_catalog);
    fs::remove_dir_all(root).expect("cleanup MCP bootstrap fixture");
}
