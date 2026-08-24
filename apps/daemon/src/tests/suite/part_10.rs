// Loom daemon tests fragment 10; included into the shared crate test module.
#[test]
fn daemon_hook_bridge_streams_workflow_preview_before_formal_result() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("workflow-preview-stream");
    let runtime = test_daemon_runtime(&root, None);
    runtime
        .workflow_store
        .save_workflow(
            "workflow-preview-stream",
            r#"name: Workflow Preview Stream
nodes:
  - id: formal
    uses: neuro.official/missing-formal-tool
  - id: preview
    uses: __sticker__
"#,
        )
        .expect("save preview workflow");
    runtime
        .tool_registry
        .save_tool(ToolDefinition::new(
            "workflow-preview-stream-art",
            "Workflow Preview Stream",
            "Preview phase fixture",
            ToolExecution::Workflow {
                workflow_id: "workflow-preview-stream".to_owned(),
                workflow_bindings: Some(WorkflowExecutionBindings {
                    primary_output: Some(loom_tool_registry::WorkflowOutputBinding {
                        node_id: "formal".to_owned(),
                        output: "result".to_owned(),
                        kind: "node_result".to_owned(),
                    }),
                    preview_output: Some(loom_tool_registry::WorkflowOutputBinding {
                        node_id: "preview".to_owned(),
                        output: "output_image".to_owned(),
                        kind: "node_result".to_owned(),
                    }),
                    preview_required_nodes: vec!["preview".to_owned()],
                    ..WorkflowExecutionBindings::default()
                }),
            },
        ))
        .expect("save preview workflow Art");

    let request = formal_art_execute_request(
        "req-workflow-preview",
        "workflow-preview-node",
        "workflow-preview-stream-art",
        Some(inline_art_input(&test_png_base64())),
        json!({}),
    );
    let (intermediate, final_response) = run_hook_bridge_text_with_intermediate(&runtime, &request);

    assert!(
        intermediate.iter().any(|event| {
            event["method"] == loom_protocol::HOOK_EVENT_ART_PREVIEW
                && event["params"]["requestId"] == "req-workflow-preview"
        }),
        "final response without preview: {final_response}"
    );
    assert!(intermediate.iter().any(|event| {
        event["method"] == loom_protocol::HOOK_EVENT_ART_FAILURE
            && event["params"]["requestId"] == "req-workflow-preview"
    }));
    assert_eq!(final_response["requestId"], "req-workflow-preview");
    assert_eq!(final_response["status"], "failed");
    fs::remove_dir_all(root).expect("cleanup preview workflow root");
}

#[test]
fn daemon_hook_bridge_returns_successful_formal_output_after_workflow_preview() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("workflow-preview-success");
    let runtime = test_daemon_runtime(&root, None);
    runtime
        .workflow_store
        .save_workflow(
            "workflow-preview-success",
            r#"name: Workflow Preview Success
nodes:
  - id: preview
    uses: __sticker__
  - id: formal
    uses: __sticker__
    needs:
      - preview
"#,
        )
        .expect("save successful preview workflow");
    runtime
        .tool_registry
        .save_tool(ToolDefinition::new(
            "workflow-preview-success-art",
            "Workflow Preview Success",
            "Successful preview and formal fixture",
            ToolExecution::Workflow {
                workflow_id: "workflow-preview-success".to_owned(),
                workflow_bindings: Some(WorkflowExecutionBindings {
                    primary_output: Some(loom_tool_registry::WorkflowOutputBinding {
                        node_id: "formal".to_owned(),
                        output: "output_image".to_owned(),
                        kind: "node_result".to_owned(),
                    }),
                    preview_output: Some(loom_tool_registry::WorkflowOutputBinding {
                        node_id: "preview".to_owned(),
                        output: "output_image".to_owned(),
                        kind: "node_result".to_owned(),
                    }),
                    preview_required_nodes: vec!["preview".to_owned()],
                    ..WorkflowExecutionBindings::default()
                }),
            },
        ))
        .expect("save successful preview Art");

    let input_image = test_png_base64();
    let request = formal_art_execute_request(
        "req-workflow-preview-success",
        "workflow-preview-success-node",
        "workflow-preview-success-art",
        Some(inline_art_input(&input_image)),
        json!({}),
    );
    let (intermediate, final_response) = run_hook_bridge_text_with_intermediate(&runtime, &request);

    assert!(
        intermediate.iter().any(|event| {
            event["method"] == loom_protocol::HOOK_EVENT_ART_PREVIEW
                && event["params"]["requestId"] == "req-workflow-preview-success"
        }),
        "final response: {final_response}"
    );
    assert!(intermediate.iter().any(|event| {
        event["method"] == loom_protocol::HOOK_EVENT_ART_RESULT
            && event["params"]["requestId"] == "req-workflow-preview-success"
            && event["params"]["resultRevision"] == 1
    }));
    assert_eq!(final_response["requestId"], "req-workflow-preview-success");
    assert_eq!(final_response["status"], "succeeded");
    assert!(final_response["data"]["outputs"]["output"]["handle"].is_string());

    fs::remove_dir_all(root).expect("cleanup successful preview workflow root");
}

fn expect_binary_route_response(
    response: RouteResponse,
    expected_status: u16,
    expected_content_type: &'static str,
) -> Vec<u8> {
    match response {
        RouteResponse::Binary {
            status,
            content_type,
            body,
        } => {
            assert_eq!(status, expected_status);
            assert_eq!(content_type, expected_content_type);
            body
        }
        RouteResponse::Text { .. } | RouteResponse::TextWithHeaders { .. } => {
            panic!("expected binary response with status {expected_status}")
        }
    }
}

#[test]
fn resolve_hook_session_path_uses_canonical_default_when_no_session_exists() {
    let appdata = unique_temp_dir("hook-session-missing");

    let resolved = resolve_hook_session_path(&appdata);

    assert_eq!(
        resolved,
        appdata.join("com.yamiyu.hook").join("session.json")
    );
}

fn start_daemon_with_store(
    path: &Path,
    brain_planner: BrainPlannerConfig,
) -> (u16, mpsc::Sender<()>, thread::JoinHandle<Result<()>>) {
    let daemon = LoomDaemon::bind(
        DaemonConfig::localhost(0)
            .with_sqlite_run_store(path)
            .with_brain_planner(brain_planner),
    )
    .expect("bind daemon with SQLite store");
    let port = daemon.local_addr().expect("local address").port();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx));
    (port, shutdown_tx, server)
}

#[derive(Debug)]
struct FailingRunEvidenceStore;

struct CountingRunEvidenceStore {
    inner: InMemoryRunEvidenceStore,
    insert_count: Arc<AtomicUsize>,
}

impl CountingRunEvidenceStore {
    fn new(insert_count: Arc<AtomicUsize>) -> Self {
        Self {
            inner: InMemoryRunEvidenceStore::default(),
            insert_count,
        }
    }
}

impl RunEvidenceStore for CountingRunEvidenceStore {
    fn insert_run(
        &mut self,
        run: Value,
        events: Vec<RunEventDraft>,
    ) -> loom_durable::RunStoreResult<()> {
        self.insert_count.fetch_add(1, Ordering::SeqCst);
        self.inner.insert_run(run, events)
    }

    fn transition_run(
        &mut self,
        run: Value,
        event: RunEventDraft,
    ) -> loom_durable::RunStoreResult<()> {
        self.inner.transition_run(run, event)
    }

    fn get_run(&self, run_id: &str) -> loom_durable::RunStoreResult<Option<Value>> {
        self.inner.get_run(run_id)
    }

    fn get_events(&self, run_id: &str) -> loom_durable::RunStoreResult<Option<Vec<Value>>> {
        self.inner.get_events(run_id)
    }

    fn recover_interrupted_runs(&mut self) -> loom_durable::RunStoreResult<usize> {
        self.inner.recover_interrupted_runs()
    }

    fn status(&self) -> RunStoreStatus {
        self.inner.status()
    }
}

impl RunEvidenceStore for FailingRunEvidenceStore {
    fn insert_run(
        &mut self,
        _run: Value,
        _events: Vec<RunEventDraft>,
    ) -> loom_durable::RunStoreResult<()> {
        Err(RunStoreError::Integrity("fixture failure".to_owned()))
    }

    fn transition_run(
        &mut self,
        _run: Value,
        _event: RunEventDraft,
    ) -> loom_durable::RunStoreResult<()> {
        Err(RunStoreError::Integrity("fixture failure".to_owned()))
    }

    fn get_run(&self, _run_id: &str) -> loom_durable::RunStoreResult<Option<Value>> {
        Err(RunStoreError::Integrity("fixture failure".to_owned()))
    }

    fn get_events(&self, _run_id: &str) -> loom_durable::RunStoreResult<Option<Vec<Value>>> {
        Err(RunStoreError::Integrity("fixture failure".to_owned()))
    }

    fn recover_interrupted_runs(&mut self) -> loom_durable::RunStoreResult<usize> {
        Err(RunStoreError::Integrity("fixture failure".to_owned()))
    }

    fn status(&self) -> RunStoreStatus {
        RunStoreStatus {
            mode: "memory",
            persistent: false,
        }
    }
}

#[test]
fn daemon_help_and_version_are_available_without_binding_a_port() {
    let help = daemon_help_text();
    assert!(help.contains("Usage: loom-daemon"));
    assert!(help.contains("LOOM_DAEMON_HOST"));
    assert!(help.contains("LOOM_DAEMON_PORT"));
    assert!(help.contains("--manifest-dir"));
    assert!(help.contains("LOOM_CAPABILITY_MANIFEST_DIR"));
    assert!(help.contains("LOOM_RUN_STORE_PATH"));
    assert!(help.contains("LOOM_DAEMON_WORKERS"));
    assert!(help.contains("worker threads [default: 4]"));
    assert!(help.contains("LOOM_DAEMON_QUEUE_CAPACITY"));
    assert!(help.contains("Queued requests [default: 32]"));
    assert!(help.contains("/v1/invoke"));

    assert_eq!(
        daemon_version_text(),
        format!("loom-daemon {}", loom_core::LOOM_VERSION)
    );
}

#[test]
fn json_http_responses_declare_utf8_charset() {
    let mut response = Vec::new();

    write_response(&mut response, 200, r#"{"name":"Hook 实时工作流"}"#).expect("write response");
    let response = String::from_utf8(response).expect("utf8 response");

    assert!(response.contains("Content-Type: application/json; charset=utf-8"));
    assert!(response.contains(r#""name":"Hook 实时工作流""#));
}

#[test]
fn daemon_serves_health_and_module_status_on_configured_isolated_port() {
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    let address = daemon.local_addr().expect("local address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve daemon"));

    let health = http_get(address.port(), "/health");
    assert!(health.contains("200 OK"));
    assert!(health.contains("\"status\":\"ok\""));

    let status = http_get(address.port(), "/status");
    assert!(status.contains("200 OK"));
    assert!(status.contains("\"status\":\"ready\""));
    assert!(status.contains("\"name\":\"core\""));
    assert!(status.contains("\"name\":\"gateway\""));
    assert!(status.contains("\"name\":\"hooks\""));

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server thread");
}

#[test]
fn daemon_reports_brain_planner_status_by_default() {
    let root = unique_temp_dir("status-brain-planner-default");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let status = expect_json_text_route_response(
        route_request(&runtime, &parsed_request("GET", "/status", &[], None)),
        200,
    );

    assert_eq!(status["brain_planner"]["mode"], "local_template");
    assert_eq!(status["brain_planner"]["configured"], false);
    assert!(status["brain_planner"].get("model").is_none());
    assert!(status["brain_planner"].get("timeout_seconds").is_none());
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn daemon_reports_inline_request_executor_by_default() {
    let root = unique_temp_dir("status-inline-request-executor");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    let status = expect_json_text_route_response(
        route_request(&runtime, &parsed_request("GET", "/status", &[], None)),
        200,
    );
    assert_eq!(status["requestExecutor"]["mode"], "inline");
    assert_eq!(status["requestExecutor"]["workers"], 1);
    assert_eq!(status["requestExecutor"]["queueCapacity"], 0);
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn daemon_reports_explicit_bounded_request_executor() {
    let root = unique_temp_dir("status-bounded-request-executor");
    let runtime = test_daemon_runtime_from_config(
        &root,
        DaemonConfig::localhost(0).with_bounded_request_executor(2, 3),
    );
    let status = expect_json_text_route_response(
        route_request(&runtime, &parsed_request("GET", "/status", &[], None)),
        200,
    );
    assert_eq!(status["requestExecutor"]["mode"], "bounded_workers");
    assert_eq!(status["requestExecutor"]["workers"], 2);
    assert_eq!(status["requestExecutor"]["queueCapacity"], 3);
    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn daemon_runtime_remains_available_across_sequential_routes() {
    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0).with_bounded_request_executor(2, 4))
        .expect("bind daemon");
    let port = daemon.local_addr().expect("address").port();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx));

    let health = http_json_get(port, "/health");
    assert_eq!(health["status"], "ok");
    assert!(health.get("pid").is_none());
    assert!(health.get("executablePath").is_none());
    let status = http_json_get(port, "/status");
    assert_eq!(status["status"], "ready");
    assert_eq!(status["pid"], std::process::id());
    assert!(status["executablePath"]
        .as_str()
        .is_some_and(|path| !path.is_empty()));
    assert!(http_json_get(port, "/v1/capabilities")["capabilities"].is_array());

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("server").expect("serve");
}
