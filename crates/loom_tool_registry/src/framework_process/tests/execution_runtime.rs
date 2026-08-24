use super::super::*;
use super::execution_support::*;
use std::fs;

#[cfg(unix)]
fn create_directory_link(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).expect("create fixture directory symlink");
}

#[cfg(windows)]
fn create_directory_link(target: &Path, link: &Path) {
    let output = std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .expect("create fixture directory junction");
    assert!(
        output.status.success(),
        "cannot create fixture directory junction: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_manifest_only_fixture(root: &Path, manifest: &[u8]) -> PathBuf {
    let package_root = root.join("publisher.test").join("script");
    let package_dir = package_root.join("versions").join("0.1.0-fixture");
    let art_dir = root.join("arts").join("fixture-art");
    fs::create_dir_all(&package_dir).expect("create fixture package");
    fs::create_dir_all(&art_dir).expect("create fixture Art");
    fs::write(
        package_root.join("active.json"),
        br#"{"active":"versions/0.1.0-fixture"}"#,
    )
    .expect("write fixture activation");
    fs::write(package_dir.join("framework.manifest.json"), manifest)
        .expect("write fixture manifest");
    art_dir
}

#[test]
fn process_error_preserves_code_message_and_detail() {
    let root = temp_root("error");
    let art_dir = write_fixture_package(&root, ERROR_SCRIPT);
    let error = execute_framework_art_in_root_with_timeout(
        &fixture_tool(&art_dir),
        "publisher.test/script",
        json!({}),
        &root,
        Duration::from_secs(10),
        None,
    )
    .expect_err("framework error response");
    let detail = match error {
        ToolRegistryError::FrameworkProcessFailed {
            code,
            message,
            detail,
            ..
        } if code == "quota_exhausted" && message == "quota exhausted" => detail,
        other => panic!("unexpected framework error: {other}"),
    };
    assert!(
        !Path::new(&detail).exists(),
        "framework temp directory leaked after an error: {detail}"
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn unsafe_framework_id_is_rejected_before_package_resolution() {
    let root = temp_root("unsafe-framework-id");
    let art_dir = root.join("arts").join("fixture-art");
    fs::create_dir_all(&art_dir).expect("create fixture art");
    let error = execute_framework_art_in_root_with_timeout(
        &fixture_tool(&art_dir),
        "../outside",
        json!({}),
        &root,
        Duration::from_secs(10),
        None,
    )
    .expect_err("unsafe framework id");
    assert!(matches!(
        error,
        ToolRegistryError::FrameworkProcessProtocol { reason, .. }
            if reason.contains("safe package id")
    ));
    fs::remove_dir_all(root).ok();
}

#[test]
fn oversized_framework_manifest_is_rejected_before_json_parsing() {
    let root = temp_root("oversized-manifest");
    let manifest = vec![b' '; crate::framework::FRAMEWORK_METADATA_MAX_BYTES as usize + 1];
    let art_dir = write_manifest_only_fixture(&root, &manifest);

    let error = execute_framework_art_in_root_with_timeout(
        &fixture_tool(&art_dir),
        "publisher.test/script",
        json!({}),
        &root,
        Duration::from_secs(10),
        None,
    )
    .expect_err("oversized manifest must fail before parsing");
    assert!(matches!(
        error,
        ToolRegistryError::FrameworkPackageNotFound { path, .. }
            if path.ends_with("framework.manifest.json")
    ));
    fs::remove_dir_all(root).ok();
}

#[test]
fn invalid_utf8_framework_manifest_is_a_protocol_error() {
    let root = temp_root("invalid-utf8-manifest");
    let art_dir = write_manifest_only_fixture(&root, &[0xff]);

    let error = execute_framework_art_in_root_with_timeout(
        &fixture_tool(&art_dir),
        "publisher.test/script",
        json!({}),
        &root,
        Duration::from_secs(10),
        None,
    )
    .expect_err("invalid UTF-8 manifest must fail before JSON parsing");
    assert!(matches!(
        error,
        ToolRegistryError::FrameworkProcessProtocol { reason, .. }
            if reason.contains("framework.manifest.json UTF-8")
    ));
    fs::remove_dir_all(root).ok();
}

#[cfg(any(unix, windows))]
#[test]
fn framework_command_link_cannot_escape_the_package_directory() {
    let root = temp_root("command-link-escape");
    let art_dir = write_fixture_package(&root, SUCCESS_SCRIPT);
    let package_dir = root
        .join("publisher.test")
        .join("script")
        .join("versions")
        .join("0.1.0-fixture");
    let outside_dir = root.join("outside-runtime");
    fs::create_dir_all(&outside_dir).expect("create outside runtime directory");
    fs::write(outside_dir.join("escape-command"), b"outside package")
        .expect("write outside command fixture");
    create_directory_link(&outside_dir, &package_dir.join("runtime").join("escape"));

    let manifest_path = package_dir.join("framework.manifest.json");
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("read fixture framework manifest"))
            .expect("parse fixture framework manifest");
    manifest["entry"]["command"] = Value::String("runtime/escape/escape-command".to_owned());
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("serialize fixture framework manifest"),
    )
    .expect("write fixture framework manifest");

    let error = execute_framework_art_in_root_with_timeout(
        &fixture_tool(&art_dir),
        "publisher.test/script",
        json!({}),
        &root,
        Duration::from_secs(10),
        None,
    )
    .expect_err("command link escaping the package must fail closed");
    assert!(matches!(
        error,
        ToolRegistryError::FrameworkProcessProtocol { reason, .. }
            if reason.contains("outside the framework package")
    ));
    fs::remove_dir_all(root).ok();
}

#[test]
fn invalid_process_response_is_a_structured_protocol_error() {
    let root = temp_root("invalid");
    let art_dir = write_fixture_package(&root, INVALID_SCRIPT);
    let error = execute_framework_art_in_root_with_timeout(
        &fixture_tool(&art_dir),
        "publisher.test/script",
        json!({}),
        &root,
        Duration::from_secs(10),
        None,
    )
    .expect_err("invalid framework response");
    assert!(matches!(
        error,
        ToolRegistryError::FrameworkProcessProtocol { reason, .. }
            if reason.contains("invalid JSON response")
    ));
    fs::remove_dir_all(root).ok();
}

#[test]
fn process_timeout_kills_the_framework_process() {
    let root = temp_root("timeout");
    let art_dir = write_fixture_package(&root, TIMEOUT_SCRIPT);
    let error = execute_framework_art_in_root_with_timeout(
        &fixture_tool(&art_dir),
        "publisher.test/script",
        json!({}),
        &root,
        Duration::from_millis(50),
        None,
    )
    .expect_err("framework timeout");
    assert!(matches!(
        error,
        ToolRegistryError::FrameworkProcessTimeout { timeout_ms: 50, .. }
    ));
    fs::remove_dir_all(root).ok();
}

#[test]
fn process_drains_large_stdout_without_deadlocking() {
    let root = temp_root("large-stdout");
    let art_dir = write_fixture_package(&root, LARGE_OUTPUT_SCRIPT);
    let result = execute_framework_art_in_root_with_timeout(
        &fixture_tool(&art_dir),
        "publisher.test/script",
        json!({}),
        &root,
        Duration::from_secs(10),
        None,
    )
    .expect("large framework response");
    assert_eq!(
        result["large"].as_str().map(str::len),
        Some(9 * 1024 * 1024)
    );
    fs::remove_dir_all(root).ok();
}

#[test]
fn process_normalizes_image_paths_before_the_temp_directory_is_removed() {
    let root = temp_root("path-image-output");
    let art_dir = write_fixture_package(&root, PATH_IMAGE_OUTPUT_SCRIPT);
    let result = execute_framework_art_in_root_with_timeout(
        &fixture_image_tool(&art_dir),
        "publisher.test/script",
        json!({}),
        &root,
        Duration::from_secs(10),
        None,
    )
    .expect("path image output");

    assert!(result.get("output_path").is_none());
    assert_eq!(result["width"], 1);
    assert_eq!(result["height"], 1);
    assert!(result["content"][0]["data"]
        .as_str()
        .is_some_and(|value| value.starts_with("data:image/png;base64,")));
    fs::remove_dir_all(root).ok();
}

#[test]
fn process_rejects_image_paths_outside_execution_output_roots() {
    let root = temp_root("outside-path-image-output");
    let art_dir = write_fixture_package(&root, OUTSIDE_PATH_IMAGE_OUTPUT_SCRIPT);
    let error = execute_framework_art_in_root_with_timeout(
        &fixture_image_tool(&art_dir),
        "publisher.test/script",
        json!({}),
        &root,
        Duration::from_secs(10),
        None,
    )
    .expect_err("outside path rejected");

    assert!(matches!(
        error,
        ToolRegistryError::FrameworkProcessProtocol { reason, .. }
            if reason.contains("outside the execution output roots")
    ));
    fs::remove_dir_all(root).ok();
}

#[test]
fn framework_art_requires_installed_package_directory_metadata() {
    let root = temp_root("missing-art-directory");
    let art_dir = write_fixture_package(&root, SUCCESS_SCRIPT);
    let mut tool = fixture_tool(&art_dir);
    tool.metadata = Some(json!({}));
    let error = execute_framework_art_in_root_with_timeout(
        &tool,
        "publisher.test/script",
        json!({}),
        &root,
        Duration::from_secs(10),
        None,
    )
    .expect_err("missing artPackage.dir must fail closed");

    assert!(matches!(
        error,
        ToolRegistryError::FrameworkArtDirectoryNotFound { path, .. }
            if path == "<metadata.artPackage.dir>"
    ));
    fs::remove_dir_all(root).ok();
}

/// The third budget S9-1 asked for: wall time for one whole art execution. This is the number a
/// user feels, and every performance finding in the review that touches the framework path ends
/// up here — resolving the package, building the request, spawning the interpreter, writing
/// stdin, reading the response back and normalising it.
///
/// The art is a fixture that echoes its request rather than one of the shipped sample packages,
/// because a sample package has to be built before it can run and this budget has to hold on
/// every push. What the fixture does keep is everything expensive: a real package on disk, a real
/// interpreter process, and the real supervisor. The framework's own work is the part the fixture
/// leaves out, and no budget here could bound that anyway.
///
/// The measured execution is the second one. A framework package is installed once and executed
/// many times, so the warm case is the representative one; the first execution also pays for the
/// operating system caching the interpreter this test copied a moment earlier, which is an
/// artefact of the fixture rather than a cost a deployment pays per execution.
#[test]
fn one_art_execution_stays_within_its_wall_time_budget() {
    // Measured at 1,562 ms warm on 2026-08-22. The ceiling is far above that because wall time is
    // the one budget that has to survive a shared CI runner competing with other jobs; it is set
    // to catch an execution that has started spawning twice or waiting on a network round trip,
    // not to track interpreter startup drift.
    const BUDGET_MS: u64 = 10_000;

    let root = temp_root("perf-wall-time");
    let art_dir = write_fixture_package(&root, SUCCESS_SCRIPT);
    let tool = fixture_tool(&art_dir);
    let execute = || {
        execute_framework_art_in_root_with_timeout(
            &tool,
            "publisher.test/script",
            json!({ "inputs": { "image": "input.png" } }),
            &root,
            Duration::from_secs(60),
            None,
        )
    };

    execute().expect("warm the fixture package");
    let started = std::time::Instant::now();
    execute().expect("measured art execution");
    let elapsed = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

    loom_perf::assert_within("art_execution_wall_time_ms", "ms", elapsed, BUDGET_MS);
    fs::remove_dir_all(root).ok();
}
