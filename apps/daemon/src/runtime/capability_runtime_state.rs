// Capability runtime bootstrap and verified registry-to-process package resolution.
type SharedCapabilityRuntime = Arc<CapabilityRuntimeHost>;

fn build_capability_runtime(control_plane_root: &Path) -> Result<SharedCapabilityRuntime> {
    let registry = loom_tool_registry::capability::CapabilityPluginRegistry::new(control_plane_root);
    registry
        .recover_capability_lifecycle()
        .context("recover Capability Plugin lifecycle")?;
    let host = Arc::new(CapabilityRuntimeHost::new(RuntimeHostLimits::default()));
    for record in registry.list().context("list Capability Plugins")? {
        if !record.enabled_intent
            || record.status != loom_tool_registry::capability::CapabilityLifecycleStatus::Active
        {
            continue;
        }
        // The crash-loop guard is the durable failure *count*, and it already survives restarts:
        // the fifth failure inside the window sets `Faulted`, which the status filter above skips.
        //
        // The restart backoff deadline is deliberately not consulted here. It throttles repeated
        // restarts inside one host process, and this is a fresh process with no restarts behind it.
        // Gating startup on it meant a daemon restart that happened to land inside the window — as
        // little as one second after a single transient failure — permanently faulted a plugin that
        // was never crash-looping, because a `Faulted` record needs an explicit re-enable to come
        // back. A daemon that really does fail activation every boot still converges on `Faulted`
        // through the count.
        let Some(digest) = record.active_digest.as_deref() else {
            registry.mark_faulted(&record.qualified_id)?;
            continue;
        };
        let activation = runtime_package(&registry, &record.qualified_id, digest)
            .and_then(|package| {
                host.activate(package).map_err(|error| {
                    loom_tool_registry::capability::CapabilityInstallError::InvalidState(
                        error.to_string(),
                    )
                })
            });
        if let Err(error) = activation {
            runtime_log_warn(format!(
                "Capability Plugin {} was faulted during startup: {error}",
                record.qualified_id
            ));
            registry.record_runtime_failure(&record.qualified_id)?;
            registry.mark_runtime_faulted(&record.qualified_id)?;
        }
    }
    Ok(host)
}

fn runtime_package(
    registry: &loom_tool_registry::capability::CapabilityPluginRegistry,
    qualified_id: &str,
    digest: &str,
) -> std::result::Result<
    CapabilityRuntimePackage,
    loom_tool_registry::capability::CapabilityInstallError,
> {
    let verified = registry.verify_installed_version(qualified_id, digest)?;
    let grant_store = loom_tool_registry::capability::CapabilityGrantStore::new(
        registry.control_plane_root(),
    );
    let granted_permissions = grant_store
        .list()?
        .into_iter()
        .find(|grant| grant.qualified_id == qualified_id && grant.package_digest == digest)
        .map(|grant| grant.permissions)
        .unwrap_or_default();
    if !verified
        .manifest
        .permissions
        .iter()
        .all(|permission| granted_permissions.contains(permission))
    {
        return Err(
            loom_tool_registry::capability::CapabilityInstallError::PermissionRequired(
                qualified_id.to_owned(),
            ),
        );
    }
    let permission_grant_digest = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&json!({
            "qualifiedId": qualified_id,
            "packageDigest": digest,
            "permissions": granted_permissions,
        }))?)
    );
    Ok(CapabilityRuntimePackage {
        manifest: verified.manifest,
        package_dir: verified.package_dir,
        digest: verified.digest,
        trust_store_path: registry.control_plane_root().join("plugin-trust.json"),
        trust_status: verified.trust_status,
        permission_grant_digest,
    })
}
