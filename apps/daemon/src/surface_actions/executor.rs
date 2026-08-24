// Surface action submission, confirmation, cancellation, and recovery orchestration.
impl SurfaceActionExecutor {
    pub(crate) fn new(
        mcp_servers: SharedMcpServerStore,
        tool_registry: ToolRegistry,
        workflow_store: WorkflowStore,
        framework_registry: FrameworkRegistry,
        control_plane_root: PathBuf,
        surface_instances: SharedSurfaceInstanceStore,
        surface_resources: SharedSurfaceResourceStore,
        hook_bridge: SharedHookBridgeRuntime,
    ) -> std::io::Result<Self> {
        let runner_registry = tool_registry.clone();
        let runner: Arc<SurfaceActionRunner> = Arc::new(move |job| {
            let servers = mcp_servers
                .lock()
                .map_err(|_| execution_error("mcp_store_unavailable", "MCP store is unavailable"))?
                .values()
                .cloned()
                .collect::<Vec<_>>();
            let timeout_millis = job
                .action
                .timeout_ms
                .unwrap_or(DEFAULT_SURFACE_ACTION_TIMEOUT_MILLIS)
                .clamp(1, MAX_SURFACE_ACTION_TIMEOUT_MILLIS);
            let timeout = Duration::from_millis(timeout_millis);
            let arguments = json!({ "surfaceAction": &job.invocation });
            if matches!(
                &job.tool.execution,
                loom_tool_registry::ToolExecution::FrameworkArt { .. }
            ) {
                loom_tool_registry::execute_tool_with_timeout_and_cancellation(
                    &job.tool,
                    &servers,
                    arguments,
                    timeout,
                    job.cancellation.as_ref(),
                )
                .map_err(|error| {
                    execution_error("surface_action_execution_failed", error.to_string())
                })
            } else {
                // The runner is the only place that can hand the flag to a non-framework tool. Until it
                // did, a cancelled MCP or cloud action ran on to its timeout and its result was recorded
                // as if the caller still wanted it.
                execute_tool_with_workflows_timeout_and_cancellation(
                    &job.tool,
                    &servers,
                    &workflow_store,
                    &runner_registry,
                    arguments,
                    timeout,
                    job.cancellation.as_ref(),
                )
                .map_err(|error| {
                    execution_error("surface_action_execution_failed", error.to_string())
                })
            }
        });
        let resolver_registry = tool_registry;
        let resolver: Arc<SurfaceToolResolver> = Arc::new(move |descriptor| {
            loom_tool_registry::install::resolve_installed_art_package(
                &control_plane_root,
                &descriptor.art_id,
                &descriptor.art_version,
                &descriptor.package_digest,
                &resolver_registry,
                &framework_registry,
            )
            .map_err(|error| SurfaceStoreError::Conflict(error.to_string()))
        });
        Self::new_with_components(
            resolver,
            surface_instances,
            surface_resources,
            hook_bridge,
            runner,
            SURFACE_ACTION_WORKERS,
            SURFACE_ACTION_QUEUE_CAPACITY,
        )
    }

    #[cfg(test)]
    fn new_with_runner(
        tool_registry: ToolRegistry,
        surface_instances: SharedSurfaceInstanceStore,
        surface_resources: SharedSurfaceResourceStore,
        hook_bridge: SharedHookBridgeRuntime,
        runner: Arc<SurfaceActionRunner>,
        workers: usize,
        queue_capacity: usize,
    ) -> std::io::Result<Self> {
        let resolver: Arc<SurfaceToolResolver> = Arc::new(move |descriptor| {
            let tool = tool_registry
                .get_tool(&descriptor.art_id)
                .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?
                .ok_or_else(|| SurfaceStoreError::NotFound(descriptor.art_id.clone()))?;
            validate_locked_tool(descriptor, &tool)?;
            Ok(tool)
        });
        Self::new_with_components(
            resolver,
            surface_instances,
            surface_resources,
            hook_bridge,
            runner,
            workers,
            queue_capacity,
        )
    }

    fn new_with_components(
        tool_resolver: Arc<SurfaceToolResolver>,
        surface_instances: SharedSurfaceInstanceStore,
        surface_resources: SharedSurfaceResourceStore,
        hook_bridge: SharedHookBridgeRuntime,
        runner: Arc<SurfaceActionRunner>,
        workers: usize,
        queue_capacity: usize,
    ) -> std::io::Result<Self> {
        let coordinator = Arc::new(Mutex::new(SurfaceActionCoordinator::default()));
        let worker_coordinator = Arc::clone(&coordinator);
        let worker_instances = Arc::clone(&surface_instances);
        let worker_resources = Arc::clone(&surface_resources);
        let worker_bridge = Arc::clone(&hook_bridge);
        let queue = BoundedRequestExecutor::new(
            "loom-surface-action",
            workers,
            queue_capacity,
            move |job| {
                execute_surface_action_job(
                    job,
                    &worker_instances,
                    &worker_resources,
                    &worker_bridge,
                    &worker_coordinator,
                    &runner,
                );
            },
        )?;
        Ok(Self {
            queue,
            coordinator,
            surface_instances,
            tool_resolver,
            manifest_cache: Mutex::new(BTreeMap::new()),
            hook_bridge,
        })
    }

    pub(crate) fn submit(
        &self,
        instance_id: &str,
        event: SurfaceEvent,
    ) -> Result<SurfaceActionAck, SurfaceStoreError> {
        self.submit_internal(instance_id, event, false)
    }

    pub(crate) fn confirm(
        &self,
        decision: SurfaceConfirmationDecision,
    ) -> Result<SurfaceActionAck, SurfaceStoreError> {
        let resolution = self
            .surface_instances
            .lock()
            .map_err(|_| SurfaceStoreError::Conflict("Surface store is unavailable".into()))?
            .resolve_confirmation(decision)?;
        match resolution {
            SurfaceConfirmationResolution::Approved { event, ack } => {
                broadcast_ack(&self.hook_bridge, &ack);
                let instance_id = event.instance_id.clone();
                self.submit_internal(&instance_id, event, true)
            }
            SurfaceConfirmationResolution::Rejected { ack }
            | SurfaceConfirmationResolution::Expired { ack } => {
                broadcast_ack(&self.hook_bridge, &ack);
                Ok(ack)
            }
        }
    }

    pub(crate) fn cancel(
        &self,
        request: SurfaceActionCancelRequest,
    ) -> Result<SurfaceActionAck, SurfaceStoreError> {
        let (event, action) = {
            let (descriptor, event) = {
                let store = self.surface_instances.lock().map_err(|_| {
                    SurfaceStoreError::Conflict("Surface store is unavailable".into())
                })?;
                let instance = store
                    .get(&request.instance_id)
                    .ok_or_else(|| SurfaceStoreError::NotFound(request.instance_id.clone()))?;
                let ack = instance
                    .event_acks
                    .values()
                    .find(|ack| ack.request_id == request.request_id)
                    .ok_or_else(|| SurfaceStoreError::NotFound(request.request_id.clone()))?;
                let event = instance
                    .pending_events
                    .iter()
                    .find(|event| event.event_id == ack.event_id)
                    .cloned()
                    .ok_or_else(|| {
                        SurfaceStoreError::Conflict(
                            "Surface action is no longer pending or running".to_owned(),
                        )
                    })?;
                (instance.descriptor.clone(), event)
            };
            let action_id = event.action.as_deref().ok_or_else(|| {
                SurfaceStoreError::Invalid("Surface event has no declared action".into())
            })?;
            // Resolved with the store lock released: the resolve reads the installed package from disk.
            let (_, action) = self.resolve_action(&descriptor, action_id)?;
            (event, action)
        };
        if !action.cancelable {
            return Err(SurfaceStoreError::Conflict(format!(
                "Surface action {} is not cancelable",
                action.id
            )));
        }
        let (_, ack) = self
            .surface_instances
            .lock()
            .map_err(|_| SurfaceStoreError::Conflict("Surface store is unavailable".into()))?
            .request_cancel(request)?;
        if let Ok(state) = self.coordinator.lock() {
            if let Some(token) = state.cancellation_tokens.get(&ack.request_id) {
                token.store(true, Ordering::Release);
            }
        }
        debug_assert_eq!(event.event_id, ack.event_id);
        broadcast_ack(&self.hook_bridge, &ack);
        Ok(ack)
    }

    /// Resolves the locked Art package for `descriptor` and picks `action_id` out of its Surface
    /// manifest.
    ///
    /// Callers must not hold the Surface store lock across this: the resolver reads the installed
    /// package from disk, so holding the store lock made every other Surface request — for any
    /// instance — queue behind one instance's package I/O.
    fn resolve_action(
        &self,
        descriptor: &loom_protocol::SurfaceInstanceDescriptor,
        action_id: &str,
    ) -> Result<(ToolDefinition, SurfaceActionDefinition), SurfaceStoreError> {
        let tool = (self.tool_resolver)(descriptor)?;
        let manifest = self.surface_manifest(descriptor, &tool)?;
        let action = manifest
            .actions
            .iter()
            .find(|action| action.id == action_id)
            .cloned()
            .ok_or_else(|| {
                SurfaceStoreError::Invalid(format!(
                    "Surface action {action_id} is not declared by the locked Art package"
                ))
            })?;
        Ok((tool, action))
    }

    /// Returns the Surface manifest of a resolved package, parsing it at most once per locked package
    /// identity.
    ///
    /// The key is `art_id`, `art_version` and `package_digest`, which together pin the package
    /// content, so a cached manifest cannot describe anything but the package the caller resolved. A
    /// poisoned cache is treated as a cache miss rather than an error: the manifest is still available
    /// from the tool, and a Surface action failing because a cache lock was poisoned would be worse
    /// than parsing it again.
    fn surface_manifest(
        &self,
        descriptor: &loom_protocol::SurfaceInstanceDescriptor,
        tool: &ToolDefinition,
    ) -> Result<Arc<SurfacePackageManifest>, SurfaceStoreError> {
        let key = format!(
            "{}@{}#{}",
            descriptor.art_id, descriptor.art_version, descriptor.package_digest
        );
        if let Ok(cache) = self.manifest_cache.lock() {
            if let Some(manifest) = cache.get(&key) {
                return Ok(Arc::clone(manifest));
            }
        }
        let manifest = tool
            .surface_manifest()
            .map_err(|error| SurfaceStoreError::Invalid(error.to_string()))?
            .ok_or_else(|| SurfaceStoreError::Invalid("Art has no Surface manifest".into()))?;
        let manifest = Arc::new(manifest);
        if let Ok(mut cache) = self.manifest_cache.lock() {
            if cache.len() >= SURFACE_MANIFEST_CACHE_LIMIT {
                cache.clear();
            }
            cache.insert(key, Arc::clone(&manifest));
        }
        Ok(manifest)
    }

    fn submit_internal(
        &self,
        instance_id: &str,
        event: SurfaceEvent,
        recovering: bool,
    ) -> Result<SurfaceActionAck, SurfaceStoreError> {
        let action_id = event
            .action
            .as_deref()
            .ok_or_else(|| {
                SurfaceStoreError::Invalid("Surface event has no declared action".into())
            })?
            .to_owned();
        let mut attempt = 0;
        let (tool, action, invocation, existing_ack, cancellation) = loop {
            attempt += 1;
            // Read the locked package, then let go of the store. Nothing is reserved or accepted yet,
            // so releasing the lock here costs only the re-read in the third phase below.
            let descriptor = {
                let store = self.surface_instances.lock().map_err(|_| {
                    SurfaceStoreError::Conflict("Surface store is unavailable".into())
                })?;
                let previous_ack = store.event_ack(instance_id, &event.event_id);
                if let Some(ack) = settled_ack(previous_ack.as_ref(), recovering) {
                    return Ok(ack);
                }
                store
                    .descriptor(instance_id)
                    .ok_or_else(|| SurfaceStoreError::NotFound(instance_id.to_owned()))?
            };
            // Resolved with no lock held: the resolver reads the installed package from disk, and the
            // manifest parse behind it is pure CPU.
            let (tool, action) = self.resolve_action(&descriptor, &action_id)?;

            let mut store = self
                .surface_instances
                .lock()
                .map_err(|_| SurfaceStoreError::Conflict("Surface store is unavailable".into()))?;
            let instance = store
                .get(instance_id)
                .ok_or_else(|| SurfaceStoreError::NotFound(instance_id.to_owned()))?;
            if !same_locked_package(&instance.descriptor, &descriptor) {
                // The instance migrated to a different package while its manifest was being read, so
                // the action definition in hand may not be the one the instance now declares.
                drop(store);
                if attempt >= SURFACE_ACTION_PREPARE_ATTEMPTS {
                    return Err(SurfaceStoreError::Conflict(
                        "Surface instance kept changing packages while its action was prepared"
                            .to_owned(),
                    ));
                }
                continue;
            }
            // Re-read the ack under the second lock: another submit of the same event may have been
            // accepted while the package was resolving, and that ack is the one the caller must see.
            let previous_ack = store.event_ack(instance_id, &event.event_id);
            if let Some(ack) = settled_ack(previous_ack.as_ref(), recovering) {
                return Ok(ack);
            }
            // `pending_events` only receives a confirmation-bound event after Host approval. Every
            // recoverable persisted status therefore represents an already approved action; asking
            // again would look for a pending confirmation that no longer exists and strand recovery.
            let already_confirmed = recovering
                && previous_ack
                    .as_ref()
                    .is_some_and(|ack| is_recoverable_action_status(&ack.status));
            if action.confirmation && !already_confirmed {
                let (ack, confirmation) =
                    store.await_confirmation(instance_id, event.clone(), action.risk.clone())?;
                drop(store);
                broadcast_confirmation(&self.hook_bridge, &confirmation);
                broadcast_ack(&self.hook_bridge, &ack);
                return Ok(ack);
            }
            let cancellation = reserve_action(&self.coordinator, instance_id, &action, &event)?;
            let ack = match previous_ack {
                Some(ack) => ack,
                None => match store.accept_event(instance_id, event.clone()) {
                    Ok(ack) => ack,
                    Err(error) => {
                        let request_id = request_id_for_event(&event.event_id);
                        release_reservation(
                            &self.coordinator,
                            instance_id,
                            &action,
                            Some(&request_id),
                        );
                        return Err(error);
                    }
                },
            };
            if ack.status == SurfaceActionStatus::CancelRequested {
                cancellation.store(true, Ordering::Release);
            }
            let invocation = SurfaceActionInvocation {
                protocol_version: SURFACE_PROTOCOL_VERSION.to_owned(),
                instance_id: instance_id.to_owned(),
                attachment_id: event.attachment_id.clone(),
                request_id: ack.request_id.clone(),
                event_id: event.event_id.clone(),
                action_id: action.id.clone(),
                event_class: event.class.clone(),
                generation: event.generation,
                base_revision: event.base_revision,
                payload: event.payload.clone(),
                authoritative_state: instance.authoritative_state,
            };
            break (tool, action, invocation, ack, cancellation);
        };

        let job = SurfaceActionJob {
            event,
            ack: existing_ack.clone(),
            action: action.clone(),
            tool,
            invocation,
            cancellation,
        };
        match self.queue.try_submit(job) {
            Ok(()) => Ok(existing_ack),
            Err(SubmitError::Full(job)) | Err(SubmitError::Closed(job)) => {
                release_reservation(
                    &self.coordinator,
                    instance_id,
                    &job.action,
                    Some(&job.ack.request_id),
                );
                let error = execution_error(
                    "surface_action_queue_full",
                    "Surface action executor is unavailable or full",
                );
                let failed = SurfaceActionAck {
                    status: SurfaceActionStatus::Failed,
                    error: Some(error),
                    ..job.ack
                };
                persist_ack(&self.surface_instances, &failed, true);
                broadcast_ack(&self.hook_bridge, &failed);
                Err(SurfaceStoreError::Conflict(
                    "Surface action executor is unavailable or full".into(),
                ))
            }
        }
    }

    pub(crate) fn recover_pending(&self) {
        let expired = self
            .surface_instances
            .lock()
            .ok()
            .and_then(|mut store| store.expire_confirmations().ok())
            .unwrap_or_default();
        for ack in expired {
            broadcast_ack(&self.hook_bridge, &ack);
        }
        let pending = self
            .surface_instances
            .lock()
            .map(|store| store.pending_events())
            .unwrap_or_default();
        for event in pending {
            let instance_id = event.instance_id.clone();
            if self.submit_internal(&instance_id, event, true).is_err() {
                continue;
            }
        }
    }
}
