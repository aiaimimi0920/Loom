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
    let generation = persistent_host_generation();

    let (first, first_host) = request_persistent_mcp_host(
        "fixture-key".to_owned(),
        generation,
        &spec,
        b"{}\n",
        None,
        &tool,
        "mcp",
    )
    .expect("first persistent framework request");
    return_persistent_host(first_host);
    let (second, second_host) = request_persistent_mcp_host(
        "fixture-key".to_owned(),
        generation,
        &spec,
        b"{}\n",
        None,
        &tool,
        "mcp",
    )
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

#[cfg(windows)]
#[test]
fn invalidated_persistent_mcp_host_cannot_return_to_the_pool() {
    let _guard = HOST_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    clear_persistent_host_pool();
    let root = temp_root("persistent-host-invalidation");
    let script_path = write_persistent_host_fixture(&root);
    let spec = persistent_host_spec(&script_path);
    let tool = persistent_host_tool();
    let generation = persistent_host_generation();

    let (_, active_host) = request_persistent_mcp_host(
        "invalidated-key".to_owned(),
        generation,
        &spec,
        b"{}\n",
        None,
        &tool,
        "mcp",
    )
    .expect("active persistent framework request");
    let invalidation = std::thread::spawn(invalidate_persistent_mcp_framework_hosts);
    let invalidation_deadline = Instant::now() + Duration::from_secs(1);
    while persistent_host_generation() == generation && Instant::now() < invalidation_deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_ne!(
        persistent_host_generation(),
        generation,
        "invalidation did not advance the host generation"
    );
    return_persistent_host(active_host);
    assert_eq!(
        invalidation.join().expect("join host invalidation"),
        PersistentMcpHostInvalidationOutcome {
            closed_idle_hosts: 0,
            drained: true,
        }
    );

    assert_eq!(persistent_host_count(), 0);
    fs::remove_dir_all(root).expect("remove invalidated persistent host fixture root");
}

#[cfg(windows)]
#[test]
fn persistent_mcp_host_can_be_invalidated_from_another_thread() {
    let _guard = HOST_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    clear_persistent_host_pool();
    let root = temp_root("persistent-host-cross-thread-invalidation");
    let script_path = write_persistent_host_fixture(&root);

    std::thread::spawn(move || {
        let spec = persistent_host_spec(&script_path);
        let tool = persistent_host_tool();
        let (_, host) = request_persistent_mcp_host(
            "cross-thread-key".to_owned(),
            persistent_host_generation(),
            &spec,
            b"{}\n",
            None,
            &tool,
            "mcp",
        )
        .expect("cross-thread persistent framework request");
        return_persistent_host(host);
    })
    .join()
    .expect("join persistent host worker");

    assert_eq!(persistent_host_count(), 1);
    assert_eq!(
        invalidate_persistent_mcp_framework_hosts(),
        PersistentMcpHostInvalidationOutcome {
            closed_idle_hosts: 1,
            drained: true,
        }
    );
    assert_eq!(persistent_host_count(), 0);
    fs::remove_dir_all(root).expect("remove cross-thread persistent host fixture root");
}

#[cfg(windows)]
#[test]
fn invalidation_cancels_and_drains_an_in_flight_persistent_mcp_host() {
    let _guard = HOST_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    clear_persistent_host_pool();
    let root = temp_root("persistent-host-in-flight-invalidation");
    let marker = root.join("request-entered.txt");
    let script_path = root.join("blocking-host.ps1");
    fs::write(
        &script_path,
        concat!(
            "param([string]$Marker)\n",
            "while ($null -ne ($line = [Console]::In.ReadLine())) {\n",
            "  [IO.File]::WriteAllText($Marker, 'entered')\n",
            "  Start-Sleep -Seconds 30\n",
            "  [Console]::Out.WriteLine('{\"status\":\"late\"}')\n",
            "  [Console]::Out.Flush()\n",
            "}\n"
        ),
    )
    .expect("write blocking persistent host fixture");
    let mut spec = persistent_host_spec(&script_path);
    spec.args.push(marker.display().to_string());
    let tool = persistent_host_tool();
    let generation = persistent_host_generation();

    let worker = std::thread::spawn(move || {
        request_persistent_mcp_host(
            "in-flight-key".to_owned(),
            generation,
            &spec,
            b"{}\n",
            None,
            &tool,
            "mcp",
        )
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !marker.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(marker.exists(), "blocking host did not enter its request");

    let outcome = invalidate_persistent_mcp_framework_hosts();
    assert_eq!(
        outcome,
        PersistentMcpHostInvalidationOutcome {
            closed_idle_hosts: 0,
            drained: true,
        }
    );
    assert!(
        worker
            .join()
            .expect("join blocked persistent host")
            .is_err(),
        "invalidated request must not complete normally"
    );
    assert_eq!(persistent_host_count(), 0);
    fs::remove_dir_all(root).expect("remove in-flight persistent host fixture root");
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
