use super::*;

#[test]
fn idle_maintenance_stops_and_lazily_restarts_a_persistent_runtime() {
    let root = temp_root("idle-maintenance");
    let executable = compile_fixture(&root);
    let host = CapabilityRuntimeHost::new(RuntimeHostLimits {
        idle_timeout: Duration::from_millis(20),
        ..RuntimeHostLimits::default()
    });
    host.activate(package(&root, &executable, &[]))
        .expect("activate persistent runtime");
    assert_eq!(host.process_ids().len(), 1);

    thread::sleep(Duration::from_millis(40));
    assert_eq!(host.prune_idle().expect("prune idle runtime"), 1);
    assert!(host.process_ids().is_empty());

    invoke_fixture(&host, None, None).expect("lazy restart after idle shutdown");
    assert_eq!(host.process_ids().len(), 1);
    host.deactivate_all();
    cleanup(&root);
}
