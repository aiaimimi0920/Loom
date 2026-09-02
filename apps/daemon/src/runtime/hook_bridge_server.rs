// Accepts Hook bridge sockets and delegates each connection to a tracked worker.
fn run_hook_bridge_websocket_server(
    listener: TcpListener,
    shutdown_rx: Receiver<()>,
    connected_clients: Arc<AtomicUsize>,
    extension_clients: Arc<AtomicUsize>,
    connections: HookBridgeConnections,
    broadcast_hub: HookBridgeBroadcastHub,
    capability_runtime: SharedCapabilityRuntime,
    capability_resources: SharedCapabilityResourceBroker,
    surface_resources: SharedSurfaceResourceStore,
    mcp_servers: SharedMcpServerStore,
    tool_registry: ToolRegistry,
    workflow_store: WorkflowStore,
    settings: SharedLoomSettingsStore,
    shared_images: SharedImageStoreHandle,
    framework_registry: FrameworkRegistry,
    control_plane_root: PathBuf,
    workflow_root: PathBuf,
    run_store: SharedRunStore,
    surface_instances: SharedSurfaceInstanceStore,
    surface_actions: SharedSurfaceActionExecutor,
) {
    loop {
        if shutdown_rx.try_recv().is_ok() {
            return;
        }

        match listener.accept() {
            Ok((stream, _)) => {
                connections.reap_finished();
                let connection_cancelled = connections.cancellation();
                let connected_clients = Arc::clone(&connected_clients);
                let extension_clients = Arc::clone(&extension_clients);
                let broadcast_hub = broadcast_hub.clone();
                let capability_runtime = Arc::clone(&capability_runtime);
                let capability_resources = Arc::clone(&capability_resources);
                let surface_resources = Arc::clone(&surface_resources);
                let mcp_servers = Arc::clone(&mcp_servers);
                let tool_registry = tool_registry.clone();
                let workflow_store = workflow_store.clone();
                let settings = Arc::clone(&settings);
                let shared_images = Arc::clone(&shared_images);
                let framework_registry = framework_registry.clone();
                let control_plane_root = control_plane_root.clone();
                let workflow_root = workflow_root.clone();
                let run_store = Arc::clone(&run_store);
                let surface_instances = Arc::clone(&surface_instances);
                let surface_actions = Arc::clone(&surface_actions);
                let worker = thread::spawn(move || {
                    handle_hook_bridge_websocket_connection(
                        stream,
                        connection_cancelled,
                        connected_clients,
                        extension_clients,
                        broadcast_hub,
                        capability_runtime,
                        capability_resources,
                        surface_resources,
                        mcp_servers,
                        tool_registry,
                        workflow_store,
                        settings,
                        shared_images,
                        framework_registry,
                        control_plane_root,
                        workflow_root,
                        run_store,
                        surface_instances,
                        surface_actions,
                    );
                });
                connections.track(worker);
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return,
        }
    }
}
