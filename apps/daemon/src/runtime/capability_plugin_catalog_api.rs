// Signed official Capability Plugin catalog and download orchestration.
const CAPABILITY_CATALOG_URL_ENV: &str = "LOOM_CAPABILITY_CATALOG_URL";
const CAPABILITY_CATALOG_LOOPBACK_ENV: &str = "LOOM_CAPABILITY_CATALOG_ALLOW_LOOPBACK";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InstallCatalogCapabilityRequest {
    qualified_id: String,
}

fn list_capability_catalog(
    control_plane_root: &Path,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let Some((client, trust_store)) = capability_catalog_client(control_plane_root)? else {
        return Ok((
            200,
            serde_json::to_string(&json!({
                "configured": false,
                "packages": [],
                "diagnostic": "未配置官方能力扩展目录"
            }))?,
        ));
    };
    let document = match client.fetch(&trust_store) {
        Ok(document) => document,
        Err(error) => return capability_catalog_error_response(error),
    };
    let host = capability_host_support(hook_bridge)?;
    let packages = document
        .signed
        .packages
        .iter()
        .map(|entry| {
            let detail = loom_tool_registry::capability::capability_host_compatibility_error(
                entry, &host,
            );
            json!({
                "entry": entry,
                "compatible": detail.is_none(),
                "compatibilityDetail": detail,
            })
        })
        .collect::<Vec<_>>();
    Ok((
        200,
        serde_json::to_string(&json!({
            "configured": true,
            "publisher": document.signed.publisher,
            "generatedAt": document.signed.generated_at,
            "expiresAt": document.signed.expires_at,
            "packages": packages,
        }))?,
    ))
}

fn install_capability_catalog_plugin(
    body: &str,
    control_plane_root: &Path,
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<(u16, String)> {
    let request: InstallCatalogCapabilityRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => return capability_bad_request("invalid_catalog_install", error.to_string()),
    };
    let Some((client, trust_store)) = capability_catalog_client(control_plane_root)? else {
        return capability_catalog_error_response(
            loom_tool_registry::capability::CapabilityCatalogError::NotFound(
                "official catalog is not configured".to_owned(),
            ),
        );
    };
    let document = match client.fetch(&trust_store) {
        Ok(document) => document,
        Err(error) => return capability_catalog_error_response(error),
    };
    let Some(entry) = document
        .signed
        .packages
        .iter()
        .find(|entry| entry.qualified_id == request.qualified_id)
    else {
        return capability_catalog_error_response(
            loom_tool_registry::capability::CapabilityCatalogError::NotFound(
                request.qualified_id,
            ),
        );
    };
    if let Some(detail) = loom_tool_registry::capability::capability_host_compatibility_error(
        entry,
        &capability_host_support(hook_bridge)?,
    ) {
        return capability_catalog_error_response(
            loom_tool_registry::capability::CapabilityCatalogError::Incompatible(detail),
        );
    }
    let downloaded = match client.download(entry) {
        Ok(downloaded) => downloaded,
        Err(error) => return capability_catalog_error_response(error),
    };
    let expectation = loom_tool_registry::capability::CapabilityCatalogInstallExpectation {
        qualified_id: entry.qualified_id.clone(),
        version: entry.version.clone(),
        publisher_key_id: entry.package.signature.key_id.clone(),
        permissions: entry.permissions.clone(),
        host_compatibility: entry.host_compatibility.clone(),
    };
    let registry = capability_registry(control_plane_root)?;
    capability_response(
        loom_tool_registry::capability::install_capability_from_catalog_zip(
            &downloaded.package_bytes,
            &registry,
            &expectation,
        )
        .map(|package| json!({
            "package": package,
            "supplyChainVerified": downloaded.supply_chain_verified,
        })),
    )
}

fn capability_catalog_client(
    control_plane_root: &Path,
) -> Result<
    Option<(
        loom_tool_registry::capability::CapabilityCatalogClient,
        loom_plugin_security::TrustStore,
    )>,
> {
    let Some(url) = std::env::var(CAPABILITY_CATALOG_URL_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let policy = OutboundPolicy {
        allow_http_loopback: std::env::var(CAPABILITY_CATALOG_LOOPBACK_ENV)
            .ok()
            .is_some_and(|value| value == "1"),
        ..OutboundPolicy::default()
    };
    let client = loom_tool_registry::capability::CapabilityCatalogClient::new(url, policy)
        .map_err(anyhow::Error::from)?;
    let trust_store = loom_plugin_security::TrustStore::load(
        &control_plane_root.join("plugin-trust.json"),
    )?;
    Ok(Some((client, trust_store)))
}

fn capability_host_support(
    hook_bridge: &SharedHookBridgeRuntime,
) -> Result<loom_tool_registry::capability::CapabilityHostSupport> {
    let hook_connected = hook_bridge
        .lock()
        .map_err(|_| anyhow::anyhow!("Hook bridge state is unavailable"))?
        .extension_capable_clients
        .load(Ordering::SeqCst)
        > 0;
    Ok(loom_tool_registry::capability::CapabilityHostSupport {
        loom_api_version: loom_protocol::CAPABILITY_API_VERSION.to_owned(),
        loom_features: vec!["commands.v1".to_owned(), "attachments.v1".to_owned()],
        hook_connected,
        hook_api_version: "1.0".to_owned(),
        hook_features: vec!["commands.v1".to_owned(), "unit-overlays.v1".to_owned()],
        surface_api_version: "1.0".to_owned(),
        surface_features: vec![
            "loom_resource".to_owned(),
            "remote_resources".to_owned(),
            "surface.javascript.v1".to_owned(),
            "input.pointer".to_owned(),
            "input.hover".to_owned(),
            "input.touch".to_owned(),
            "input.keyboard".to_owned(),
        ],
    })
}

fn capability_catalog_error_response(
    error: loom_tool_registry::capability::CapabilityCatalogError,
) -> Result<(u16, String)> {
    use loom_tool_registry::capability::CapabilityCatalogError as Error;
    let (status, code) = match &error {
        Error::NotFound(_) => (404, "capability_catalog_not_found"),
        Error::Incompatible(_) => (409, "capability_host_incompatible"),
        Error::Invalid(_) | Error::Trust(_) => (400, "invalid_capability_catalog"),
        Error::Network(_) => (502, "capability_catalog_unavailable"),
    };
    structured_error(status, json!({ "code": code, "message": error.to_string() }))
}

fn capability_plugin_disk_bytes(
    registry: &loom_tool_registry::capability::CapabilityPluginRegistry,
    plugins: &[loom_tool_registry::capability::CapabilityPluginRecord],
) -> BTreeMap<String, u64> {
    plugins
        .iter()
        .map(|plugin| {
            let bytes = plugin
                .versions
                .iter()
                .filter_map(|version| directory_bytes(&registry.packages_root().join(&version.relative_path)))
                .fold(0_u64, u64::saturating_add);
            (plugin.qualified_id.clone(), bytes)
        })
        .collect()
}

fn directory_bytes(root: &Path) -> Option<u64> {
    let mut pending = vec![root.to_path_buf()];
    let mut total = 0_u64;
    let mut visited = 0_usize;
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path).ok()?;
        if metadata.file_type().is_symlink() {
            return None;
        }
        visited += 1;
        if visited > 100_000 {
            return None;
        }
        if metadata.is_file() {
            total = total.checked_add(metadata.len())?;
        } else if metadata.is_dir() {
            pending.extend(fs::read_dir(path).ok()?.filter_map(|entry| entry.ok().map(|entry| entry.path())));
        }
    }
    Some(total)
}
