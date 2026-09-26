// Per-daemon ownership isolates control-plane roots and closes peers on account changes.
struct ProjectionOwner {
    runtime: tokio::runtime::Runtime,
    active: Arc<Mutex<Option<Arc<loom_projection::ProjectionRuntime>>>>,
    monitor: tokio::task::JoinHandle<()>,
    closed: Arc<std::sync::atomic::AtomicBool>,
}

impl ProjectionOwner {
    fn new(root: PathBuf) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let active = Arc::new(Mutex::new(None));
        let monitored = Arc::clone(&active);
        let closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop = Arc::clone(&closed);
        let handle = runtime.handle().clone();
        let monitor = runtime.spawn(async move {
            loop {
                let state = Arc::clone(&monitored);
                let root = root.clone();
                let handle = handle.clone();
                let stop = Arc::clone(&stop);
                let _ = tokio::task::spawn_blocking(move || {
                    if let Ok(mut active) = state.lock() {
                        if !stop.load(std::sync::atomic::Ordering::Acquire) {
                            let _ = Self::reconcile(&handle, &mut active, &root);
                        }
                    }
                })
                .await;
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
        Ok(Self {
            runtime,
            active,
            monitor,
            closed,
        })
    }

    fn reconcile(
        runtime: &tokio::runtime::Handle,
        active: &mut Option<Arc<loom_projection::ProjectionRuntime>>,
        root: &Path,
    ) -> std::result::Result<Arc<loom_projection::ProjectionRuntime>, loom_projection::Error> {
        let identity = account_login::projection_identity(root);
        let unchanged = identity.as_ref().ok().is_some_and(|identity| {
            active.as_ref().is_some_and(|running| {
                running.is_active()
                    && running.identity().origin() == identity.origin()
                    && running.identity().session() == identity.session()
            })
        });
        if unchanged {
            return Ok(active.as_ref().expect("checked active runtime").clone());
        }
        if let Some(previous) = active.take() {
            runtime.block_on(previous.close());
        }
        let identity = identity.map_err(|e| loom_projection::Error {
            status: e.status,
            code: e.code,
        })?;
        let running = runtime.block_on(loom_projection::ProjectionRuntime::start(
            identity,
            root.join("projections-v2"),
        ))?;
        *active = Some(running.clone());
        Ok(running)
    }

    fn execute(
        &self,
        root: &Path,
        actor: &str,
        operation: loom_projection::LocalOperation,
    ) -> std::result::Result<Value, loom_projection::Error> {
        let running = {
            let mut active = self.active.lock().map_err(|_| loom_projection::Error {
                status: 503,
                code: "projection_transport_unavailable",
            })?;
            Self::reconcile(self.runtime.handle(), &mut active, root)?
        };
        self.runtime.block_on(running.execute(actor, operation))
    }

    fn account(
        &self,
        root: &Path,
        action: &str,
        body: &str,
    ) -> std::result::Result<Value, account_login::Error> {
        let mut active = self.active.lock().map_err(|_| account_login::Error {
            status: 503,
            code: "account_unavailable",
        })?;
        // Fence the old generation before a logout's potentially offline central request.
        if action == "logout" {
            if let Some(previous) = active.take() {
                self.runtime.block_on(previous.close());
            }
        }
        let result = account_login::handle(root, action, body);
        if result
            .as_ref()
            .ok()
            .is_some_and(|value| value["status"] == "signed_out")
        {
            if let Some(previous) = active.take() {
                self.runtime.block_on(previous.close());
            }
        }
        result
    }
}

impl Drop for ProjectionOwner {
    fn drop(&mut self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::Release);
        self.monitor.abort();
        if let Ok(mut active) = self.active.lock() {
            if let Some(previous) = active.take() {
                self.runtime.block_on(previous.close());
            }
        }
    }
}
