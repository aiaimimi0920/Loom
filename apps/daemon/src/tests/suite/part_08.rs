// Loom daemon tests fragment 8; included into the shared crate test module.
#[test]
fn hook_art_rejects_tampered_art_packages_before_execution() {
    let root = unique_temp_dir("hook-art-package-integrity");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    runtime
        .framework_registry
        .install_framework_package_from_zip(&framework_package_zip("process", "1.0.0"))
        .expect("install framework");
    let install = loom_tool_registry::install::install_art_from_zip(
        &art_package_zip("integrity-art", "1.0.0", b"never-executed"),
        &root,
        &runtime.framework_registry,
        &runtime.tool_registry,
    )
    .expect("install Art");
    let payload_path = install.art_dir.join("bin/tool.exe");
    let mut permissions = fs::metadata(&payload_path)
        .expect("payload metadata")
        .permissions();
    permissions.set_readonly(false);
    fs::set_permissions(&payload_path, permissions).expect("unlock installed payload");
    fs::write(&payload_path, b"tampered").expect("tamper installed payload");

    let hook_response = run_hook_bridge_text(
        &runtime,
        &formal_art_execute_request(
            "integrity-request",
            "integrity-node",
            "integrity-art",
            None,
            json!({}),
        ),
    );
    assert!(
        hook_response
            .to_string()
            .contains("integrity verification failed"),
        "unexpected Hook response: {hook_response}"
    );
    let hook_run_id = hook_response["data"]["executionId"]
        .as_str()
        .expect("Hook run id");
    {
        let store = runtime.run_store.lock().expect("run store");
        assert_eq!(
            store.get_run(hook_run_id).unwrap().unwrap()["status"],
            "failed"
        );
        let events = store.get_events(hook_run_id).unwrap().unwrap();
        assert_eq!(events.last().unwrap()["kind"], "external_tool_failed");
    }

    drop(runtime);
    fs::remove_dir_all(&root).ok();
}

#[test]
fn installed_art_resolution_preserves_mutable_registry_state() {
    let root = unique_temp_dir("art-package-enabled-state");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    runtime
        .framework_registry
        .install_framework_package_from_zip(&framework_package_zip("process", "1.0.0"))
        .expect("install framework");
    loom_tool_registry::install::install_art_from_zip(
        &art_package_zip("disabled-art", "1.0.0", b"never-executed"),
        &root,
        &runtime.framework_registry,
        &runtime.tool_registry,
    )
    .expect("install Art");

    let mut registered = runtime
        .tool_registry
        .get_tool("disabled-art")
        .expect("read registered Art")
        .expect("registered Art");
    registered.enabled = false;
    registered
        .metadata
        .as_mut()
        .and_then(Value::as_object_mut)
        .expect("registered metadata")
        .insert(
            "artUserSettings".to_owned(),
            json!({ "credentialBindings": { "api_key": "stored-secret" } }),
        );
    let resolved = resolve_registered_tool_package(
        &registered,
        &runtime.tool_registry,
        &runtime.framework_registry,
        &root,
    )
    .expect("resolve immutable Art package");

    assert!(!resolved.enabled);
    assert_eq!(
        resolved
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.pointer("/artUserSettings/credentialBindings/api_key")),
        Some(&Value::String("stored-secret".to_owned()))
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn workflow_art_install_registers_its_packaged_definition() {
    let root = unique_temp_dir("workflow-art-package-install");
    let runtime = test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0));
    runtime
        .framework_registry
        .install_framework_package_from_zip(&framework_package_zip("workflow", "1.0.0"))
        .expect("install workflow framework");
    let zip = workflow_art_package_zip("packaged-workflow-art", "packaged-workflow");
    let body = serde_json::to_string(&json!({
        "zipBase64": format!("data:application/zip;base64,{}", BASE64.encode(zip))
    }))
    .expect("install request");

    let (status, response) = install_art(
        &body,
        &runtime.tool_registry,
        &runtime.framework_registry,
        &runtime.workflow_store,
        &root,
        &runtime.hook_bridge,
        &runtime.bundled_art_sha256_allowlist,
    )
    .expect("install workflow Art");

    assert_eq!(status, 200, "body={response}");
    assert_eq!(
        runtime
            .workflow_store
            .load_workflow("packaged-workflow")
            .expect("load packaged workflow"),
        "name: Package Flow\nnodes: []\n"
    );
    drop(runtime);
    fs::remove_dir_all(root).ok();
}

#[test]
fn plugin_trust_routes_add_list_and_revoke_publishers() {
    let root = unique_temp_dir("plugin-trust-routes");
    let registry = FrameworkRegistry::new(&root);
    let public_key = BASE64.encode([7u8; 32]);
    let add_body = serde_json::to_string(&json!({
        "publisherId": "example.vendor",
        "keyId": "example-key",
        "publicKey": public_key,
        "revoked": false
    }))
    .expect("add body");
    let (status, body) = trust_plugin_publisher(&add_body, &registry).expect("trust");
    assert_eq!(status, 200);
    assert!(body.contains("example.vendor"));

    let (status, body) = list_plugin_trust(&registry).expect("list");
    assert_eq!(status, 200);
    assert!(body.contains("example-key"));
    let (status, body) =
        set_plugin_trust_policy(r#"{"policy":"require_trusted"}"#, &registry).expect("set policy");
    assert_eq!(status, 200);
    assert!(body.contains("\"policy\":\"require_trusted\""));
    assert_eq!(
        registry.trust_store().unwrap().policy,
        TrustPolicy::RequireTrusted
    );

    let revoke_body = serde_json::to_string(&json!({
        "publisherId": "example.vendor",
        "keyId": "example-key"
    }))
    .expect("revoke body");
    let (status, body) = revoke_plugin_publisher(&revoke_body, &registry).expect("revoke");
    assert_eq!(status, 200);
    assert!(body.contains("\"revoked\":true"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn plugin_credential_routes_never_return_secret_values() {
    let root = unique_temp_dir("plugin-credential-routes");
    let body = serde_json::to_string(&json!({
        "name": "api_key",
        "value": "top-secret",
        "scope": {
            "frameworkId": "cloud_api",
            "artId": "example-art"
        }
    }))
    .expect("credential body");
    let (status, response) = save_plugin_credential(&body, &root).expect("save");
    assert_eq!(status, 200);
    assert!(!response.contains("top-secret"));
    assert!(response.contains("\"valueType\":\"string\""));

    let (status, response) = list_plugin_credentials(&root).expect("list");
    assert_eq!(status, 200);
    assert!(response.contains("api_key"));
    assert!(response.contains("\"valueType\":\"string\""));
    assert!(!response.contains("top-secret"));

    let delete = serde_json::to_string(&json!({
        "name": "api_key",
        "scope": {
            "frameworkId": "cloud_api",
            "artId": "example-art"
        }
    }))
    .expect("delete body");
    let (status, response) = reveal_plugin_credential(&delete, &root).expect("reveal");
    assert_eq!(status, 200);
    assert!(response.contains("top-secret"));
    let (status, _) = delete_plugin_credential(&delete, &root).expect("delete");
    assert_eq!(status, 200);
    let _ = fs::remove_dir_all(root);
}

// Cloud API authoring accepts object headers and descriptor-array bodies;
// these tests lock their conversion into executable string templates.
const CONCURRENCY_GATE_TIMEOUT: Duration = Duration::from_secs(5);

fn wait_for_test_gate(gate: &Arc<(Mutex<bool>, Condvar)>, timeout: Duration) -> bool {
    let (gate_lock, gate_signal) = &**gate;
    let released = gate_lock.lock().expect("read test gate");
    let (released, _) = gate_signal
        .wait_timeout_while(released, timeout, |released| !*released)
        .expect("wait test gate");
    *released
}

fn release_test_gate(gate: &Arc<(Mutex<bool>, Condvar)>) {
    let (gate_lock, gate_signal) = &**gate;
    *gate_lock.lock().expect("release test gate") = true;
    gate_signal.notify_all();
}

struct BlockingBrainPlanner {
    entered: Arc<(Mutex<bool>, Condvar)>,
    release: Arc<(Mutex<bool>, Condvar)>,
}

impl brain_plan::BrainPlanner for BlockingBrainPlanner {
    fn plan(
        &self,
        _request: BrainPlanRequest,
    ) -> std::result::Result<brain_plan::BrainPlanResult, brain_plan::BrainPlannerError> {
        let (entered_lock, entered_signal) = &*self.entered;
        *entered_lock.lock().expect("enter planner") = true;
        entered_signal.notify_all();
        if !wait_for_test_gate(&self.release, CONCURRENCY_GATE_TIMEOUT) {
            return Err(brain_plan::BrainPlannerError::InvalidModelOutput(
                "fixture release timed out".to_owned(),
            ));
        }
        Ok(brain_plan::BrainPlanResult {
            summary: "concurrent plan".to_owned(),
            steps: vec!["complete".to_owned()],
            source: brain_plan::BrainPlanSource::Gateway,
            model: Some("fixture-model".to_owned()),
        })
    }

    fn status(&self) -> BrainPlannerStatus {
        BrainPlannerStatus {
            mode: "gateway",
            configured: true,
            model: Some("fixture-model".to_owned()),
            timeout_seconds: Some(30),
        }
    }
}

struct CountingBlockingBrainPlanner {
    entered: Arc<(Mutex<usize>, Condvar)>,
    release: Arc<(Mutex<bool>, Condvar)>,
}

impl brain_plan::BrainPlanner for CountingBlockingBrainPlanner {
    fn plan(
        &self,
        _request: BrainPlanRequest,
    ) -> std::result::Result<brain_plan::BrainPlanResult, brain_plan::BrainPlannerError> {
        let (entered_lock, entered_signal) = &*self.entered;
        *entered_lock.lock().expect("count planner entry") += 1;
        entered_signal.notify_all();
        if !wait_for_test_gate(&self.release, CONCURRENCY_GATE_TIMEOUT) {
            return Err(brain_plan::BrainPlannerError::InvalidModelOutput(
                "fixture release timed out".to_owned(),
            ));
        }
        Ok(brain_plan::BrainPlanResult {
            summary: "concurrent plan".to_owned(),
            steps: vec!["complete".to_owned()],
            source: brain_plan::BrainPlanSource::Gateway,
            model: Some("fixture-model".to_owned()),
        })
    }

    fn status(&self) -> BrainPlannerStatus {
        BrainPlannerStatus {
            mode: "gateway",
            configured: true,
            model: Some("fixture-model".to_owned()),
            timeout_seconds: Some(30),
        }
    }
}

struct ConcurrencyTestFixture {
    releases: Vec<Box<dyn Fn() + Send + Sync>>,
    shutdown_tx: Option<mpsc::Sender<()>>,
    clients: Vec<thread::JoinHandle<()>>,
    server: Option<thread::JoinHandle<Result<()>>>,
}

impl ConcurrencyTestFixture {
    fn new(shutdown_tx: mpsc::Sender<()>, server: thread::JoinHandle<Result<()>>) -> Self {
        Self {
            releases: Vec::new(),
            shutdown_tx: Some(shutdown_tx),
            clients: Vec::new(),
            server: Some(server),
        }
    }

    fn add_release_action<F>(&mut self, action: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.releases.push(Box::new(action));
    }

    fn add_release_gate(&mut self, gate: Arc<(Mutex<bool>, Condvar)>) {
        self.add_release_action(move || release_test_gate(&gate));
    }

    fn release_gates(&self) {
        for release in &self.releases {
            release();
        }
    }

    fn request_shutdown(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
    }

    fn spawn_client<T, F>(&mut self, task: F) -> mpsc::Receiver<T>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        let (result_tx, result_rx) = mpsc::channel();
        self.clients.push(thread::spawn(move || {
            let _ = result_tx.send(task());
        }));
        result_rx
    }

    fn join_clients(&mut self) -> Result<()> {
        let mut client_panicked = false;
        for client in self.clients.drain(..) {
            if client.join().is_err() {
                client_panicked = true;
            }
        }
        if client_panicked {
            anyhow::bail!("Loom concurrency fixture client thread panicked");
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        self.release_gates();
        self.request_shutdown();
        let clients_result = self.join_clients();
        let Some(server) = self.server.take() else {
            return clients_result;
        };
        let server_result = server
            .join()
            .map_err(|_| anyhow::anyhow!("Loom concurrency fixture server thread panicked"))?;
        clients_result?;
        server_result
    }
}

impl Drop for ConcurrencyTestFixture {
    fn drop(&mut self) {
        self.release_gates();
        self.request_shutdown();
        for client in self.clients.drain(..) {
            let _ = client.join();
        }
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}
