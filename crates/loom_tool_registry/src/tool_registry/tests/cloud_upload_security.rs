//! Package paths and defaults must not grant access to host control-plane files.

use super::*;

fn upload_tool(package: &Path) -> ToolDefinition {
    let mut tool = ToolDefinition::new(
        "cloud-upload-security",
        "Cloud upload security",
        "Multipart file authority regression",
        ToolExecution::CloudApi {
            endpoint: "https://api.example.com/upload".to_owned(),
            method: "POST".to_owned(),
            content_type: Some("multipart/form-data".to_owned()),
            headers: None,
            body: None,
        },
    );
    tool.metadata = Some(serde_json::json!({
        "artPackage": { "dir": package }
    }));
    tool
}

#[test]
fn cloud_upload_rejects_private_files_from_arguments_and_package_defaults() {
    // Deliberately use a loom-prefixed temp control plane: the old temp allowlist
    // must not re-authorize secrets after the control-plane root is removed.
    let root = temp_root("cloud-upload-private");
    let package = root.join("arts/publisher/upload/versions/1.0.0");
    fs::create_dir_all(&package).unwrap();
    let template = r#"{"file":"{{inputs.image.path}}"}"#;
    for relative in [
        "daemon-token",
        "plugin-credentials.json",
        "runs.sqlite",
        "arts/other/state/private.json",
        "cache/private.png",
    ] {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"DO-NOT-UPLOAD").unwrap();
        let mut tool = upload_tool(&package);
        for from_default in [false, true] {
            let arguments = if from_default {
                tool.params = vec![serde_json::json!({
                    "id": "image", "default": path
                })];
                prepare_tool_arguments(&tool, serde_json::json!({})).unwrap()
            } else {
                serde_json::json!({ "image": path })
            };
            let result = run_cloud_future(build_cloud_multipart_form(
                &tool,
                Some(template),
                &arguments,
            ))
            .unwrap();
            assert!(
                result.is_err(),
                "uploaded {relative}, default={from_default}"
            );
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cloud_upload_rejects_package_traversal_and_sibling_prefix() {
    let root = temp_root("cloud-upload-traversal");
    let package = root.join("package");
    let sibling = root.join("package-private");
    fs::create_dir_all(&package).unwrap();
    fs::create_dir_all(&sibling).unwrap();
    fs::write(sibling.join("secret"), b"DO-NOT-UPLOAD").unwrap();
    let tool = upload_tool(&package);
    for path in [
        sibling.join("secret"),
        package.join("../package-private/secret"),
    ] {
        assert!(cloud_multipart_upload_path(&tool, "file", path.to_str().unwrap()).is_err());
    }
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn cloud_upload_rejects_package_symlink_escape() {
    let root = temp_root("cloud-upload-symlink");
    let package = root.join("package");
    fs::create_dir_all(&package).unwrap();
    let secret = root.join("secret");
    fs::write(&secret, b"DO-NOT-UPLOAD").unwrap();
    let link = package.join("resource.png");
    std::os::unix::fs::symlink(&secret, &link).unwrap();
    assert!(
        cloud_multipart_upload_path(&upload_tool(&package), "file", link.to_str().unwrap())
            .is_err()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cloud_upload_rejects_other_calls_staging_and_accepts_own_package_resource() {
    let root = temp_root("cloud-upload-staging");
    let package = root.join("arts/publisher/upload/versions/1.0.0");
    fs::create_dir_all(&package).unwrap();
    let tool = upload_tool(&package);
    let resource = package.join("resource.png");
    fs::write(&resource, b"package-resource").unwrap();
    assert_eq!(
        cloud_multipart_upload_path(&tool, "file", resource.to_str().unwrap()).unwrap(),
        fs::canonicalize(&resource).unwrap()
    );
    let other_call = temp_root("cloud-upload-other-call");
    let staged = other_call.join("input.png");
    fs::write(&staged, b"other-call-input").unwrap();
    assert!(cloud_multipart_upload_path(&tool, "file", staged.to_str().unwrap()).is_err());
    fs::remove_dir_all(other_call).unwrap();
    fs::remove_dir_all(root).unwrap();
}
