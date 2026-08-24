//! Archive path and name-collision regressions.

use super::*;

#[test]
fn rejects_package_path_traversal() {
    let root = std::env::temp_dir().join(staging_name());
    let mut bytes = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut bytes);
        zip.start_file("../outside.txt", SimpleFileOptions::default())
            .expect("unsafe entry");
        zip.write_all(b"unsafe").expect("unsafe bytes");
        zip.finish().expect("finish zip");
    }
    let error =
        install_server_package(&root, &bytes.into_inner()).expect_err("path traversal must fail");
    assert!(matches!(error, McpPackageError::UnsafePath(_)));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rejects_a_package_with_a_case_colliding_manifest() {
    // On Windows two entries differing only in case land on one file, so the last copy used to
    // win while a reviewer reading the archive by name saw the first: what was reviewed and
    // what was installed could differ.
    let root = std::env::temp_dir().join(staging_name());
    let mut bytes = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut bytes);
        let options = SimpleFileOptions::default();
        zip.start_file(MCP_SERVER_PACKAGE_MANIFEST, options)
            .expect("reviewed manifest entry");
        zip.write_all(br#"{"schemaVersion":1,"id":"reviewed"}"#)
            .expect("reviewed manifest bytes");
        zip.start_file(MCP_SERVER_PACKAGE_MANIFEST.to_ascii_uppercase(), options)
            .expect("installed manifest entry");
        zip.write_all(br#"{"schemaVersion":1,"id":"installed"}"#)
            .expect("installed manifest bytes");
        zip.finish().expect("finish zip");
    }
    let error = install_server_package(&root, &bytes.into_inner())
        .expect_err("case-colliding manifest must fail");
    assert!(matches!(error, McpPackageError::UnsafePath(_)));
    let _ = fs::remove_dir_all(root);
}
