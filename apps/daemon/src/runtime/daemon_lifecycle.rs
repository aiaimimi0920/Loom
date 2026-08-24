// Daemon construction, owned runtime state, binding, serving, and shutdown.
pub struct LoomDaemon {
    listener: TcpListener,
    runtime: Arc<DaemonRuntime>,
    request_executor: RequestExecutorConfig,
}

#[cfg(test)]
static TEST_BOUND_DAEMON_TOKENS: OnceLock<Mutex<HashMap<u16, String>>> = OnceLock::new();

#[cfg(test)]
fn record_test_bound_daemon_token(port: u16, token: &str) {
    TEST_BOUND_DAEMON_TOKENS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("record test daemon token")
        .insert(port, token.to_owned());
}

#[cfg(test)]
fn test_bound_daemon_token(port: u16) -> Option<String> {
    TEST_BOUND_DAEMON_TOKENS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("read test daemon token")
        .get(&port)
        .cloned()
}

impl LoomDaemon {
    pub fn bind(mut config: DaemonConfig) -> Result<Self> {
        if config.surface_resource_gc_min_age_ms < MIN_RESOURCE_GC_AGE_MILLIS {
            anyhow::bail!(
                "Surface resource GC minimum age must be at least {MIN_RESOURCE_GC_AGE_MILLIS} ms"
            );
        }
        if !config.tls_terminated && !is_loopback_bind_host(&config.host) {
            anyhow::bail!(
                "loom daemon refuses plaintext non-loopback bind {}; place it behind an authenticated TLS terminator and set LOOM_TLS_TERMINATED=1",
                config.host
            );
        }
        if config.manifest_dir.is_some() && !is_loopback_bind_host(&config.host) {
            anyhow::bail!(
                "loom discovery manifest requires a loopback bind host, got {}",
                config.host
            );
        }
        let brain_planner = build_brain_planner(config.brain_planner)?;
        let listener = TcpListener::bind((config.host.as_str(), config.port))
            .with_context(|| format!("bind loom daemon to {}:{}", config.host, config.port))?;
        listener
            .set_nonblocking(true)
            .context("set daemon listener nonblocking")?;
        let local_addr = listener
            .local_addr()
            .context("read daemon local addr for manifest")?;
        let control_plane_root = config.control_plane_root.take().unwrap_or_else(|| {
            #[cfg(test)]
            {
                std::env::temp_dir().join(format!("loom-daemon-test-{}", Uuid::new_v4()))
            }
            #[cfg(not(test))]
            {
                std::env::var_os("LOOM_CONTROL_PLANE_ROOT")
                    .map(PathBuf::from)
                    .unwrap_or_else(default_control_plane_root)
            }
        });
        let config_root = config.configuration_root.take().unwrap_or_else(|| {
            #[cfg(test)]
            {
                control_plane_root.join("configuration")
            }
            #[cfg(not(test))]
            {
                std::env::var_os("LOOM_CONFIGURATION_ROOT")
                    .map(PathBuf::from)
                    .unwrap_or_else(default_configuration_root)
            }
        });
        #[cfg(not(test))]
        {
            let framework_runtime_root = control_plane_root.join("frameworks");
            std::env::set_var("LOOM_CONTROL_PLANE_ROOT", &control_plane_root);
            std::env::set_var("LOOM_FRAMEWORK_PACKAGES_DIR", &framework_runtime_root);
        }
        repair_legacy_control_plane_permissions(&control_plane_root).with_context(|| {
            format!(
                "repair Loom control-plane permissions in {}",
                control_plane_root.display()
            )
        })?;
        let auth_token = resolve_daemon_auth_token(config.auth_token.take(), &control_plane_root)?;
        #[cfg(test)]
        record_test_bound_daemon_token(local_addr.port(), &auth_token);
        let settings_base_url = settings_url_with_token(
            &std::env::var("LOOM_SETTINGS_BASE_URL")
                .unwrap_or_else(|_| format!("http://{local_addr}/settings")),
            &auth_token,
        );
        let mut run_store: Box<dyn RunEvidenceStore> = match &config.run_store {
            RunStoreConfig::Memory => Box::new(InMemoryRunEvidenceStore::default()),
            RunStoreConfig::Sqlite(path) => {
                Box::new(SqliteRunEvidenceStore::open(path).map_err(|error| {
                    anyhow::anyhow!("open Loom run store `{}`: {error}", path.display())
                })?)
            }
        };
        run_store
            .recover_interrupted_runs()
            .map_err(|error| anyhow::anyhow!("recover Loom run store: {error}"))?;
        let run_store_status = run_store.status();
        if let Some(manifest_dir) = config.manifest_dir.as_deref() {
            if let Err(error) =
                write_local_capability_manifest(manifest_dir, local_addr, Some(auth_token.as_str()))
            {
                handle_capability_manifest_error(local_addr, error)?;
            }
        }
        let request_executor = config.request_executor;
        let settings_store =
            LoomSettingsStore::new(control_plane_root.join("settings").join("settings.json"));
        apply_runtime_settings(&settings_store.settings);
        let mcp_servers = Arc::new(Mutex::new(load_persisted_mcp_servers(&control_plane_root)));
        let tool_registry = ToolRegistry::new(control_plane_root.join("tools"));
        let workflow_store = WorkflowStore::new(control_plane_root.join("workflows"));
        let hook_bridge = Arc::new(Mutex::new(HookBridgeRuntime::new(
            control_plane_root.join("workflows"),
        )));
        let surface_instances = Arc::new(Mutex::new(
            SurfaceInstanceStore::new(
                control_plane_root
                    .join("surface-instances")
                    .join("instances.json"),
            )
            .context("open Surface instance store")?,
        ));
        let surface_resources = Arc::new(Mutex::new(
            SurfaceResourceStore::new_with_gc_min_age(
                control_plane_root.join("surface-resources"),
                config.surface_resource_gc_min_age_ms,
            )
            .context("open Surface resource store")?,
        ));
        // Startup is the one moment the whole reference set is knowable and nothing is mid-flight:
        // every lease minted by the previous process is either persisted or gone, and no request has
        // been accepted yet. Objects whose carrying instance was deleted while the daemon was down
        // are collected here; a running daemon collects on delete instead.
        collect_surface_resource_garbage_logged(&surface_instances, &surface_resources, "startup");
        let surface_actions = Arc::new(
            SurfaceActionExecutor::new(
                Arc::clone(&mcp_servers),
                tool_registry.clone(),
                workflow_store.clone(),
                FrameworkRegistry::new(&control_plane_root),
                control_plane_root.to_path_buf(),
                Arc::clone(&surface_instances),
                Arc::clone(&surface_resources),
                Arc::clone(&hook_bridge),
            )
            .context("start Surface action executor")?,
        );
        surface_actions.recover_pending();
        let runtime = DaemonRuntime {
            hook_settings: config.hook_settings,
            run_store: Arc::new(Mutex::new(run_store)),
            auth_token,
            config_registry: Arc::new(built_in_registry()),
            config_store: FileDocumentStore::new(config_root),
            mcp_servers,
            tool_registry,
            workflow_store,
            canvas_workflow_root: control_plane_root.join("canvas-workflows"),
            framework_registry: FrameworkRegistry::new(&control_plane_root),
            control_plane_root: control_plane_root.to_path_buf(),
            bundled_art_sha256_allowlist: config.bundled_art_sha256_allowlist,
            hook_bridge,
            device_registry: Arc::new(Mutex::new(
                DeviceRegistryStore::new(
                    control_plane_root.join("settings").join("devices.json"),
                    local_addr,
                )
                .context("open device registry")?,
            )),
            surface_instances,
            surface_actions,
            surface_resources,
            settings: Arc::new(Mutex::new(settings_store)),
            shared_images: Arc::new(Mutex::new(SharedImageStore::new())),
            ocr_provider: Arc::new(Mutex::new(OcrProvider::from_env())),
            settings_base_url,
            mcp_registry_endpoint: config.mcp_registry_endpoint,
            brain_planner,
            run_store_status,
            request_executor_status: request_executor.status(),
            serialized_route_lock: Mutex::new(()),
            #[cfg(test)]
            serialized_route_observer: None,
            #[cfg(test)]
            request_submission_observer: None,
            #[cfg(test)]
            shutdown_observer: None,
            #[cfg(test)]
            connection_accept_observer: None,
        };
        Ok(Self {
            listener,
            runtime: Arc::new(runtime),
            request_executor,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.listener.local_addr().context("read daemon local addr")
    }

    pub fn serve_until(self, shutdown: Receiver<()>) -> Result<()> {
        let worker_runtime = Arc::clone(&self.runtime);
        let mut executor = match self.request_executor {
            RequestExecutorConfig::Inline => None,
            RequestExecutorConfig::Bounded {
                workers,
                queue_capacity,
            } => Some(BoundedRequestExecutor::new(
                "loom-request",
                workers,
                queue_capacity,
                move |job: RequestJob| handle_request_job(job, &worker_runtime),
            )?),
        };
        let surface_stream_runtime = Arc::clone(&self.runtime);
        let mut surface_stream_executor = BoundedRequestExecutor::new(
            "loom-surface-stream",
            SURFACE_STREAM_WORKERS,
            SURFACE_STREAM_QUEUE_CAPACITY,
            move |job: RequestJob| handle_request_job(job, &surface_stream_runtime),
        )?;

        // Reads happen on their own pool and come back through `ready_rx`, so the accept thread
        // never touches a client's byte stream. See `CONNECTION_READ_WORKERS`.
        let (ready_tx, ready_rx) = mpsc::channel::<ReadyConnection>();
        let read_draining = Arc::new(AtomicBool::new(false));
        let reader_draining = Arc::clone(&read_draining);
        let mut read_stage = BoundedRequestExecutor::new(
            "loom-read",
            CONNECTION_READ_WORKERS,
            CONNECTION_READ_QUEUE_CAPACITY,
            move |job: ConnectionReadJob| read_connection(job, &reader_draining),
        )?;
        let peer_read_admission = PeerReadAdmission::new(CONNECTION_READ_PER_PEER_LIMIT);

        let mut read_stage_result: std::io::Result<()> = Ok(());
        let serve_result: Result<()> = 'serve: loop {
            if shutdown.try_recv().is_ok() {
                // Read the backlog before the listener goes away: shutdown can be observed before
                // the first accept, and dropping a queued connection resets it.
                let drained = drain_accept_backlog(&self.listener);
                begin_shutdown(
                    &self.runtime,
                    &mut executor,
                    &mut surface_stream_executor,
                    &read_draining,
                );
                // The readers are joined before the drain so that every connection which finished
                // reading is answered, rather than being dropped by a worker racing the drain.
                read_stage_result = read_stage.shutdown();
                for ready in drained {
                    dispatch_connection(
                        ready,
                        &self.runtime,
                        &executor,
                        &surface_stream_executor,
                        true,
                    );
                }
                while let Ok(ready) = ready_rx.try_recv() {
                    dispatch_connection(
                        ready,
                        &self.runtime,
                        &executor,
                        &surface_stream_executor,
                        true,
                    );
                }
                break Ok(());
            }

            let mut accepted = false;
            match self.listener.accept() {
                Ok((stream, peer)) => {
                    accepted = true;
                    if let Some(stream) = prepare_connection(stream) {
                        if let Some(peer_permit) = peer_read_admission.try_acquire(peer.ip()) {
                            let job = ConnectionReadJob {
                                stream,
                                ready: ready_tx.clone(),
                                _peer_permit: peer_permit,
                            };
                            match read_stage.try_submit(job) {
                                Ok(()) => record_connection_accepted(&self.runtime),
                                Err(SubmitError::Full(job)) => {
                                    let (status, body) = daemon_busy_response();
                                    drain_and_write_refusal(job.stream, status, &body);
                                }
                                Err(SubmitError::Closed(job)) => {
                                    let (status, body) = daemon_shutting_down_response();
                                    drain_and_write_refusal(job.stream, status, &body);
                                }
                            }
                        } else {
                            let (status, body) = daemon_busy_response();
                            drain_and_write_refusal(stream, status, &body);
                        }
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => break Err(error).context("accept daemon connection"),
            }

            // A pass that accepted something takes only the reads already waiting; a pass that did
            // not waits briefly for one. That wait is what keeps this loop from spinning.
            let mut pending = if accepted {
                ready_rx.try_recv().ok()
            } else {
                let wait = Duration::from_millis(ACCEPT_IDLE_WAIT_MILLIS);
                ready_rx.recv_timeout(wait).ok()
            };
            while let Some(ready) = pending {
                let shutdown_after_read = shutdown.try_recv().is_ok();
                if shutdown_after_read {
                    begin_shutdown(
                        &self.runtime,
                        &mut executor,
                        &mut surface_stream_executor,
                        &read_draining,
                    );
                }
                let outcome = dispatch_connection(
                    ready,
                    &self.runtime,
                    &executor,
                    &surface_stream_executor,
                    shutdown_after_read,
                );
                if matches!(outcome, DispatchOutcome::Stop) {
                    // The listener is about to go away here too, so the backlog gets the same
                    // treatment it gets at the top of the loop.
                    let drained = drain_accept_backlog(&self.listener);
                    read_stage_result = read_stage.shutdown();
                    for ready in drained {
                        dispatch_connection(
                            ready,
                            &self.runtime,
                            &executor,
                            &surface_stream_executor,
                            true,
                        );
                    }
                    break 'serve Ok(());
                }
                pending = ready_rx.try_recv().ok();
            }
        };

        // Sockets still queued for a read are answered rather than read: the daemon is on its way
        // out, and a queued client may be one that never finishes sending.
        read_draining.store(true, Ordering::SeqCst);
        let shutdown_result = executor
            .as_mut()
            .map(BoundedRequestExecutor::shutdown)
            .transpose();
        let surface_stream_shutdown_result = surface_stream_executor.shutdown();
        let read_stage_shutdown_result = read_stage.shutdown();
        if let Err(error) = serve_result {
            let _ = shutdown_result;
            let _ = surface_stream_shutdown_result;
            let _ = read_stage_shutdown_result;
            let _ = read_stage_result;
            return Err(error);
        }
        shutdown_result.context("shutdown Loom request executor")?;
        surface_stream_shutdown_result.context("shutdown Loom Surface stream executor")?;
        read_stage_result.context("shutdown Loom connection reader")?;
        read_stage_shutdown_result.context("shutdown Loom connection reader")?;
        Ok(())
    }
}
