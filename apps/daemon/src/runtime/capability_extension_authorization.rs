// Single-use, connection-scoped permission leases. Snapshot declarations alone are not grants.
const EXTENSION_AUTHORIZATION_TTL: Duration = Duration::from_secs(30);
const MAX_EXTENSION_AUTHORIZATIONS: usize = 32;

struct ExtensionResourceAuthorization {
    plugin: loom_protocol::ExtensionPluginBinding,
    command_id: String,
    generation: u64,
    target: ExtensionTarget,
    issued: std::time::Instant,
}

fn extension_grant_is_current(
    state: &ExtensionConnectionState,
    plugin: &loom_protocol::ExtensionPluginBinding,
) -> bool {
    let Some(root) = &state.control_plane_root else {
        return false;
    };
    let registry = loom_tool_registry::capability::CapabilityPluginRegistry::new(root);
    let Ok(package) = runtime_package(&registry, &plugin.id, &plugin.package_digest) else {
        return false;
    };
    package.trust_status == loom_protocol::PackageTrustStatus::Trusted
        && plugin.trust_status == loom_protocol::ExtensionTrustStatus::Trusted
        && package.permission_grant_digest == plugin.permission_grant_digest
        && package.effective_permissions == plugin.effective_permissions
}

fn handle_extension_authorization(
    request: loom_protocol::ExtensionCommandAuthorizeRequest,
    state: &mut ExtensionConnectionState,
    runtime: &SharedCapabilityRuntime,
) -> ExtensionBridgeTextResult {
    let denied = || {
        extension_bridge_failure(
            &request.request_id,
            "extension_authorization_denied",
            "extension resource authorization was denied",
            false,
        )
    };
    if !state.extension_session_matches(&request.session_id)
        || !state.has_feature(loom_protocol::EXTENSION_FEATURE_RESOURCE_AUTHORIZATION)
        || !state.has_feature(loom_protocol::EXTENSION_FEATURE_COMMANDS)
        || [
            &request.request_id,
            &request.plugin_id,
            &request.command_id,
            &request.target.unit_id,
        ]
        .iter()
        .any(|value| value.is_empty() || value.len() > 384)
    {
        return denied();
    }
    let Ok(snapshot) = runtime.contribution_snapshot() else {
        return denied();
    };
    if snapshot.generation != request.snapshot_generation {
        return denied();
    }
    let Some(plugin) = snapshot
        .plugins
        .iter()
        .find(|plugin| plugin.id == request.plugin_id)
    else {
        return denied();
    };
    let Some(command) = snapshot.contributions.commands.iter().find(|command| {
        command.id == request.command_id
            && command.plugin_id == plugin.id
            && command.scope_id == plugin.scope_id
    }) else {
        return denied();
    };
    let permissions_valid = command
        .payload
        .get("permissions")
        .and_then(Value::as_array)
        .is_some_and(|permissions| {
            permissions.iter().all(|permission| {
                permission.as_str().is_some_and(|permission| {
                    plugin
                        .effective_permissions
                        .iter()
                        .any(|grant| grant == permission)
                })
            })
        });
    if !permissions_valid || !extension_grant_is_current(state, plugin) {
        return denied();
    }
    if request.check_only {
        return extension_bridge_success(&request.request_id, json!({ "authorized": true }), false);
    }
    state
        .resource_authorizations
        .retain(|_, authorization| authorization.issued.elapsed() < EXTENSION_AUTHORIZATION_TTL);
    if state.resource_authorizations.len() >= MAX_EXTENSION_AUTHORIZATIONS {
        return denied();
    }
    let id = format!("extension-auth:{}", Uuid::new_v4());
    state.resource_authorizations.insert(
        id.clone(),
        ExtensionResourceAuthorization {
            plugin: plugin.clone(),
            command_id: request.command_id,
            generation: snapshot.generation,
            target: request.target,
            issued: std::time::Instant::now(),
        },
    );
    extension_bridge_success(&request.request_id, json!({ "authorizationId": id }), false)
}

fn consume_extension_authorization(
    state: &mut ExtensionConnectionState,
    id: Option<&str>,
    invocation: &ExtensionInvocation,
    snapshot: &ContributionSnapshot,
) -> bool {
    let Some(id) = id else {
        return false;
    };
    // Remove even a mismatched ticket: failures cannot be probed repeatedly or replayed.
    let Some(authorization) = state.resource_authorizations.remove(id) else {
        return false;
    };
    authorization.issued.elapsed() < EXTENSION_AUTHORIZATION_TTL
        && authorization.generation == snapshot.generation
        && authorization.generation == invocation.snapshot_generation
        && authorization.command_id == invocation.command_id
        && authorization.plugin.id == invocation.plugin_id
        && authorization.target == invocation.target
        && snapshot
            .plugins
            .iter()
            .any(|plugin| plugin == &authorization.plugin)
        && extension_grant_is_current(state, &authorization.plugin)
}
