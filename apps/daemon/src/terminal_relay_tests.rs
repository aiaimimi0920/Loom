use super::*;

fn insert_slot(root: Option<&Path>) -> PendingRelayGuard {
    let id = Uuid::new_v4().to_string();
    global_relay_hub().state.lock().unwrap().slots.insert(
        id.clone(),
        RelaySlot {
            http_scope: root.map(Path::to_path_buf),
            descriptor: PendingTerminalRelayDescriptor {
                relay_id: id.clone(),
                workflow_id: "workflow".into(),
                node_id: "node".into(),
                uses: "tool".into(),
                device_id: Some("owner".into()),
                arguments: json!({"private": "input"}),
                created_at_ms: unix_time_millis(),
            },
            outcome: None,
        },
    );
    PendingRelayGuard { relay_id: id }
}

fn result(id: &str) -> TerminalRelayResultSubmission {
    TerminalRelayResultSubmission {
        relay_id: id.into(),
        status: Some("succeeded".into()),
        output: Some(json!({"ok": true})),
        error: None,
    }
}

#[test]
fn results_require_scope_transport_and_device_and_are_first_wins() {
    let a = Path::new("daemon-a");
    let b = Path::new("daemon-b");
    let slot = insert_slot(Some(a));
    assert!(complete_relay_slot(result(&slot.relay_id), Some(b), Some("owner")).is_err());
    assert!(complete_relay_slot(result(&slot.relay_id), None, None).is_err());
    assert!(complete_relay_slot(result(&slot.relay_id), Some(a), Some("other")).is_err());
    complete_relay_slot(result(&slot.relay_id), Some(a), Some("owner")).unwrap();
    assert_eq!(
        complete_relay_slot(result(&slot.relay_id), Some(a), Some("owner")),
        Err("relay_already_completed")
    );
}

#[test]
fn socket_results_are_limited_to_ids_issued_on_that_socket() {
    let slot = insert_slot(None);
    let raw = json!({"method": HOOK_METHOD_WORKFLOW_NODE_RELAY_RESULT,
        "params": {"relayId": slot.relay_id, "output": "done"}})
    .to_string();
    let mut issued = std::collections::HashSet::new();
    assert!(try_handle_websocket_relay_result(&raw, &issued));
    assert!(
        global_relay_hub().state.lock().unwrap().slots[&slot.relay_id]
            .outcome
            .is_none()
    );
    issued.insert(slot.relay_id.clone());
    assert!(try_handle_websocket_relay_result(&raw, &issued));
    assert!(
        global_relay_hub().state.lock().unwrap().slots[&slot.relay_id]
            .outcome
            .is_some()
    );
}

#[test]
fn worker_presence_is_scoped_to_the_daemon_and_requested_device() {
    let root = PathBuf::from(Uuid::new_v4().to_string());
    record_terminal_worker_heartbeat(&root, "owner");
    assert!(has_active_http_relay_worker(&root, Some("owner")));
    assert!(!has_active_http_relay_worker(&root, Some("other")));
    assert!(!has_active_http_relay_worker(
        Path::new("other-daemon"),
        None
    ));
}

#[test]
fn shutdown_cancels_waiting_and_new_http_relays() {
    use std::sync::atomic::AtomicBool;
    let root = PathBuf::from(Uuid::new_v4().to_string());
    let shutdown = AtomicBool::new(false);
    record_terminal_worker_heartbeat(&root, "owner");
    std::thread::scope(|scope| {
        let waiting = scope.spawn(|| {
            DaemonOtterWorkflowRelay::for_http(&root, &shutdown).execute_on_terminal(
                TerminalNodeRelayRequest {
                    workflow_id: "wf",
                    node_id: "node",
                    uses: "__sticker__",
                    tool: None,
                    device_id: Some("owner"),
                    arguments: json!({}),
                    timeout: Some(Duration::from_secs(30)),
                    cancellation: None,
                },
            )
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        while !global_relay_hub()
            .state
            .lock()
            .unwrap()
            .slots
            .values()
            .any(|slot| slot.http_scope.as_deref() == Some(root.as_path()))
        {
            assert!(Instant::now() < deadline, "relay did not enter wait");
            std::thread::yield_now();
        }
        let started = Instant::now();
        shutdown.store(true, Ordering::Release);
        assert!(matches!(
            waiting.join().unwrap(),
            Err(WorkflowRuntimeError::Cancelled)
        ));
        assert!(started.elapsed() < Duration::from_secs(2));
    });
    let result = DaemonOtterWorkflowRelay::for_http(&root, &shutdown).execute_on_terminal(
        TerminalNodeRelayRequest {
            workflow_id: "wf",
            node_id: "node",
            uses: "__sticker__",
            tool: None,
            device_id: Some("owner"),
            arguments: json!({}),
            timeout: None,
            cancellation: None,
        },
    );
    assert!(matches!(result, Err(WorkflowRuntimeError::Cancelled)));
}
