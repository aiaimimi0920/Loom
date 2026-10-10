//! Runtime registry metadata cannot stand in for current dependency bytes.
use super::*;
use std::io::{Cursor, Read, Write};

fn package_with_runtime_dependency() -> Vec<u8> {
    let source = fake_framework_package_zip_with_version("process", "1.0.0");
    let mut archive = zip::ZipArchive::new(Cursor::new(source)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if entry.name() == FRAMEWORK_MANIFEST_FILE {
            let mut manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            manifest["dependencies"] = serde_json::json!([{
                "kind": "runtime", "id": "fixture.runtime", "version": "=1.0.0"
            }]);
            bytes = serde_json::to_vec(&manifest).unwrap();
        }
        output
            .start_file(entry.name(), zip::write::SimpleFileOptions::default())
            .unwrap();
        output.write_all(&bytes).unwrap();
    }
    output.finish().unwrap().into_inner()
}

#[test]
fn framework_admission_rehashes_registered_runtime_dependencies() {
    let root = temp_root();
    let runtime = root.join("external-runtime");
    fs::create_dir_all(&runtime).unwrap();
    fs::write(runtime.join("runtime.exe"), b"original-runtime").unwrap();
    crate::dependency::RuntimeRegistry::new(&root)
        .register(crate::dependency::PackageCandidate {
            kind: "runtime".to_owned(),
            id: "fixture.runtime".to_owned(),
            version: "1.0.0".to_owned(),
            sha256: canonical_package_digest(&runtime, None).unwrap(),
            path: runtime.clone(),
        })
        .unwrap();
    let registry = FrameworkRegistry::new(&root);
    registry
        .install_framework_package_from_zip(&package_with_runtime_dependency())
        .unwrap();
    let package = registry.runtime_dir("publisher.test/process");
    let text = fs::read_to_string(package.join(FRAMEWORK_MANIFEST_FILE)).unwrap();
    let admission =
        FrameworkExecutionAdmission::capture(&root.join("frameworks"), &package, &text).unwrap();
    fs::write(runtime.join("runtime.exe"), b"tampered-runtime").unwrap();
    let error = admission.revalidate().unwrap_err();
    assert!(error.contains("contents have changed"), "{error}");
    assert!(
        FrameworkExecutionAdmission::capture(&root.join("frameworks"), &package, &text).is_err()
    );
    set_framework_tree_readonly(&root, false).unwrap();
    fs::remove_dir_all(root).unwrap();
}
