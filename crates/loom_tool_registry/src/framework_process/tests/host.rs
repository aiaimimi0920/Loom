use super::super::*;
use std::fs;
use std::path::PathBuf;

static HOST_TEST_LOCK: Mutex<()> = Mutex::new(());

fn temp_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("loom-framework-process-{name}-{}", request_id()));
    fs::create_dir_all(&root).expect("create process test root");
    root
}

#[test]
fn request_temp_directory_refuses_a_preexisting_leaf() {
    let root = temp_root("occupied-temp-directory");
    let occupied = root.join("occupied");
    fs::create_dir(&occupied).expect("precreate occupied request directory");

    assert!(
        TempDirectoryGuard::create(occupied).is_err(),
        "a request must not adopt and later remove a preexisting directory"
    );
    fs::remove_dir_all(root).expect("remove occupied request directory fixture");
}

#[cfg(windows)]
fn write_persistent_host_fixture(root: &Path) -> PathBuf {
    let script_path = root.join("persistent-host.ps1");
    fs::write(
        &script_path,
        concat!(
            "$count = 0\n",
            "while ($null -ne ($line = [Console]::In.ReadLine())) {\n",
            "  $count += 1\n",
            "  [Console]::Out.WriteLine(('{{\"count\":{0}}}' -f $count))\n",
            "  [Console]::Out.Flush()\n",
            "}\n"
        ),
    )
    .expect("write persistent host fixture");
    script_path
}

#[cfg(windows)]
fn persistent_host_spec(script_path: &Path) -> ProcessSpec {
    let mut spec = ProcessSpec::new("powershell.exe");
    spec.args = vec![
        "-NoLogo".to_owned(),
        "-NoProfile".to_owned(),
        "-NonInteractive".to_owned(),
        "-ExecutionPolicy".to_owned(),
        "Bypass".to_owned(),
        "-File".to_owned(),
        script_path.display().to_string(),
    ];
    spec.limits.timeout = Duration::from_secs(30);
    spec
}

#[cfg(windows)]
fn persistent_host_tool() -> ToolDefinition {
    ToolDefinition::new(
        "persistent-host-fixture",
        "Persistent Host Fixture",
        "Exercise the bounded framework host pool",
        crate::ToolExecution::FrameworkArt {
            framework: "mcp".to_owned(),
        },
    )
}

#[cfg(windows)]
#[test]
fn persistent_mcp_framework_host_is_reused_between_requests() {
    let _guard = HOST_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    clear_persistent_host_pool();
    let root = temp_root("persistent-host");
    let script_path = write_persistent_host_fixture(&root);
    let spec = persistent_host_spec(&script_path);
    let tool = persistent_host_tool();

    let (first, first_host) =
        request_persistent_mcp_host("fixture-key".to_owned(), &spec, b"{}\n", None, &tool, "mcp")
            .expect("first persistent framework request");
    return_persistent_host(first_host);
    let (second, second_host) =
        request_persistent_mcp_host("fixture-key".to_owned(), &spec, b"{}\n", None, &tool, "mcp")
            .expect("second persistent framework request");
    return_persistent_host(second_host);

    assert_eq!(serde_json::from_slice::<Value>(&first).unwrap()["count"], 1);
    assert_eq!(
        serde_json::from_slice::<Value>(&second).unwrap()["count"],
        2
    );
    clear_persistent_host_pool();
    assert_eq!(persistent_host_count(), 0);
    fs::remove_dir_all(root).expect("remove persistent host fixture root");
}

#[test]
fn persistent_mcp_framework_hosts_have_a_process_wide_limit() {
    let _guard = HOST_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    clear_persistent_host_pool();
    assert_eq!(persistent_host_count(), 0);
    assert_eq!(
        exercise_persistent_host_slot_limit(),
        (MAX_PERSISTENT_MCP_HOSTS, true, 0)
    );
}
