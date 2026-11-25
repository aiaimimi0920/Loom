//! Immutable package resolution plus bounded source read; sourceDigest binds the read bytes to the verified manifest.
use super::*;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    sdk_version: String,
    entry: String,
    source_digest: String,
    permissions: Vec<Permission>,
    services: Vec<String>,
    #[serde(default)]
    resources: Vec<super::source_resources::Entry>,
}
pub(super) struct Loaded {
    pub bytes: Vec<u8>,
    pub directory: PathBuf,
    pub resources: Vec<super::source_resources::Entry>,
    pub ports: Option<loom_protocol::local_art::package::ports::Ports>,
    pub visibility: Option<loom_protocol::local_art::package::visibility::Visibility>,
}
pub(super) fn enabled(s: &Services<'_>, grant: &Grant) -> LocalResult<()> {
    if !s
        .tools
        .get_tool(&grant.binding.art_id)
        .map_err(|_| fail(503, "local_art_registry_unavailable"))?
        .is_some_and(|tool| tool.enabled && tool.qualified_id() == grant.binding.art_id)
    {
        return Err(fail(403, "local_art_disabled"));
    }
    Ok(())
}
pub(super) fn load(s: &Services<'_>, grant: &Grant) -> LocalResult<Vec<u8>> {
    Ok(resolve(s, grant)?.bytes)
}
pub(super) fn resolve(s: &Services<'_>, grant: &Grant) -> LocalResult<Loaded> {
    enabled(s, grant)?;
    let b = &grant.binding;
    let tool = loom_tool_registry::install::resolve_installed_art_package(
        s.root,
        &b.art_id,
        &b.art_version,
        &b.package_digest,
        s.tools,
        s.frameworks,
    )
    .map_err(|_| fail(409, "local_art_package_unavailable"))?;
    if tool.qualified_id() != b.art_id {
        return Err(fail(409, "local_art_package_identity"));
    }
    let metadata = tool
        .metadata
        .as_ref()
        .ok_or_else(|| fail(409, "local_art_manifest_required"))?;
    let manifest: Manifest = serde_json::from_value(
        metadata
            .get("otterExecution")
            .cloned()
            .ok_or_else(|| fail(409, "local_art_manifest_required"))?,
    )
    .map_err(|_| fail(409, "local_art_manifest_invalid"))?;
    if manifest.sdk_version != SDK
        || manifest.permissions.len() > 4
        || manifest.services.len() > 32
        || !grant
            .permissions
            .iter()
            .all(|p| manifest.permissions.contains(p))
        || !grant.services.iter().all(|p| manifest.services.contains(p))
    {
        return Err(fail(409, "local_art_manifest_scope"));
    }
    let mut declared = grant.clone();
    declared.permissions = manifest.permissions;
    declared.services = manifest.services;
    declared
        .validate()
        .map_err(|_| fail(409, "local_art_manifest_scope"))?;
    let entry = &manifest.entry;
    if entry.len() > 512
        || entry.contains('\\')
        || entry.contains(':')
        || !entry.ends_with(".js")
        || entry
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(fail(409, "local_art_entry_invalid"));
    }
    let directory = metadata
        .get("artPackage")
        .and_then(|v| v.get("dir"))
        .and_then(Value::as_str)
        .ok_or_else(|| fail(409, "local_art_package_unavailable"))?;
    super::source_resources::validate(&manifest.resources)?;
    let bytes = load_surface_javascript_source_limited(
        s.root,
        Path::new(directory),
        &loom_protocol::SurfaceVariant {
            runtime: SurfaceRuntimeKind::Javascript,
            entry: entry.clone(),
            required_capabilities: Vec::new(),
        },
        1024 * 1024,
    )
    .map_err(|_| fail(409, "local_art_source_unavailable"))?;
    if bytes.is_empty()
        || bytes.len() > 1024 * 1024
        || bytes.contains(&0)
        || std::str::from_utf8(&bytes).is_err()
        || sha256_bytes(&bytes) != manifest.source_digest
    {
        return Err(fail(409, "local_art_source_invalid"));
    }
    Ok(Loaded {
        // 旧协议不消费端口；无效声明仅在显式新协议被请求时拒绝，不改旧执行合同。
        ports: super::source_ports::resolve(&tool).ok(),
        visibility: super::source_ports::visibility(&tool).ok(),
        bytes,
        directory: PathBuf::from(directory),
        resources: manifest.resources,
    })
}
