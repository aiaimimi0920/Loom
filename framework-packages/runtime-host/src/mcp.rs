use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use loom_mcp::{McpClient, McpServerConfig, McpTransport};
use loom_protocol::{CredentialGrant, FrameworkExecuteRequest, FrameworkMcpServer};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The caller-supplied key that carries a Surface invocation. Reserved by this framework, so it is
/// never forwarded to an MCP server as a tool argument.
const SURFACE_ACTION_KEY: &str = "surfaceAction";

#[derive(Debug, Deserialize)]
struct ArtManifest {
    #[serde(default)]
    metadata: ArtMetadata,
}

#[derive(Debug, Default, Deserialize)]
struct ArtMetadata {
    #[serde(default)]
    mcp: Option<McpArtConfig>,
    #[serde(default)]
    dependencies: ArtDependencies,
}

/// The subset of `metadata.dependencies` this framework needs. Deliberately a local mirror of
/// `loom_tool_registry::framework::ArtDependencies` rather than a dependency on that crate: the
/// runtime host is a framework package that only speaks the framework protocol. If the manifest
/// shape changes, this has to change with it.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtDependencies {
    #[serde(default)]
    mcp_servers: Vec<ArtMcpServerDependency>,
}

#[derive(Clone, Debug, Deserialize)]
struct ArtMcpServerDependency {
    id: String,
    version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpArtConfig {
    server_id: String,
    package_id: String,
    version: String,
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    arguments: Value,
    #[serde(default)]
    calls: Vec<McpCallConfig>,
    #[serde(default)]
    surface_actions: BTreeMap<String, McpSurfaceActionConfig>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpCallConfig {
    id: String,
    tool_name: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct McpSurfaceActionConfig {
    #[serde(default)]
    calls: Option<Vec<String>>,
    #[serde(default)]
    arguments: BTreeMap<String, McpArgumentBinding>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct McpArgumentBinding {
    from: Vec<String>,
}

#[derive(Debug)]
struct ResolvedCall {
    id: String,
    tool_name: String,
    arguments: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCallExecution {
    tool_name: String,
    result: Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpExecution {
    server_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    results: BTreeMap<String, McpCallExecution>,
    #[serde(skip_serializing_if = "is_false")]
    skipped: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

pub fn execute(request: &FrameworkExecuteRequest, art_dir: &Path) -> Result<McpExecution, String> {
    let config = load_config(art_dir)?;
    let calls = resolve_calls(request, &config)?;
    let resolved = request
        .context
        .mcp_server
        .as_ref()
        .ok_or_else(|| "MCP dependency was not resolved by the Loom host".to_owned())?;
    validate_resolved_server(&config, resolved)?;
    if calls.is_empty() {
        return Ok(McpExecution {
            server_id: resolved.id.clone(),
            tool_name: None,
            result: None,
            results: BTreeMap::new(),
            skipped: true,
        });
    }
    let transport = match resolved.transport.as_str() {
        "stdio" => McpTransport::Stdio,
        "streamable-http" => McpTransport::StreamableHttp,
        other => return Err(format!("resolved MCP transport `{other}` is unsupported")),
    };
    let environment = build_environment(request, resolved)?;
    let headers = build_headers(request, resolved)?;
    let mut server = match transport {
        McpTransport::Stdio => {
            if resolved.command.trim().is_empty() {
                return Err("resolved stdio MCP command is missing".to_owned());
            }
            McpServerConfig::new(
                resolved.id.clone(),
                format!("{} MCP server", resolved.id),
                resolved.command.clone(),
            )
        }
        McpTransport::StreamableHttp => McpServerConfig::remote(
            resolved.id.clone(),
            format!("{} MCP server", resolved.id),
            expand_runtime_paths(&resolved.url, request, art_dir),
        ),
    };
    server.args = resolved
        .args
        .iter()
        .map(|argument| expand_runtime_paths(argument, request, art_dir))
        .collect();
    server.env = environment;
    server.headers = headers;

    let call_results = execute_tools(&server, &calls)
        .map_err(|error| redact_credentials(error, &request.context.credentials))?;
    if config.calls.is_empty() {
        let call = calls
            .first()
            .ok_or_else(|| "legacy MCP Art did not resolve a tool call".to_owned())?;
        let result = call_results
            .into_iter()
            .next()
            .ok_or_else(|| "legacy MCP Art did not return a tool result".to_owned())?;
        return Ok(McpExecution {
            server_id: resolved.id.clone(),
            tool_name: Some(call.tool_name.clone()),
            result: Some(result),
            results: BTreeMap::new(),
            skipped: false,
        });
    }

    let results = calls
        .into_iter()
        .zip(call_results)
        .map(|(call, result)| {
            (
                call.id,
                McpCallExecution {
                    tool_name: call.tool_name,
                    result,
                },
            )
        })
        .collect();
    Ok(McpExecution {
        server_id: resolved.id.clone(),
        tool_name: None,
        result: None,
        results,
        skipped: false,
    })
}

/// Hold the server the host resolved to what the Art declared: same server id, same package, and a
/// version the declared requirement admits. The version half is the part that used to be missing —
/// `metadata.mcp.version` was parsed and then ignored, so an Art pinned to `=2.9.0` ran happily
/// against whatever the host had installed.
fn validate_resolved_server(
    config: &McpArtConfig,
    resolved: &FrameworkMcpServer,
) -> Result<(), String> {
    if resolved.id != config.server_id {
        return Err(format!(
            "resolved MCP server `{}` does not match Art dependency `{}`",
            resolved.id, config.server_id
        ));
    }
    if resolved.package_id != config.package_id {
        return Err(format!(
            "resolved MCP package `{}` does not match Art dependency `{}`",
            resolved.package_id, config.package_id
        ));
    }
    if resolved.version.trim().is_empty() {
        return Err(format!(
            "resolved MCP package `{}` reported no version, so Art dependency `{}` cannot be checked",
            resolved.package_id, config.version
        ));
    }
    if let Some(bounds) = requirement_bounds(&config.version) {
        if !bounds.admits(&resolved.version) {
            return Err(format!(
                "resolved MCP package `{}` version `{}` does not satisfy Art dependency `{}` (needs >= {} and < {})",
                resolved.package_id,
                resolved.version.trim(),
                config.version.trim(),
                format_version(bounds.lower),
                format_version(bounds.upper)
            ));
        }
    }
    Ok(())
}

fn load_config(art_dir: &Path) -> Result<McpArtConfig, String> {
    let manifest_path = art_dir.join("manifest.json");
    let manifest: ArtManifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?,
    )
    .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    let mut config = manifest
        .metadata
        .mcp
        .ok_or_else(|| "MCP Art metadata.mcp is required".to_owned())?;
    normalize_config(&mut config)?;
    if config.server_id.trim().is_empty() {
        return Err("MCP Art metadata.mcp.serverId is required".to_owned());
    }
    if config.package_id.trim().is_empty() {
        return Err("MCP Art metadata.mcp.packageId is required".to_owned());
    }
    if config.version.trim().is_empty() {
        return Err("MCP Art metadata.mcp.version is required".to_owned());
    }
    validate_declared_dependency(&config, &manifest.metadata.dependencies)?;
    validate_argument_object(&config.arguments, "metadata.mcp.arguments")?;
    validate_call_config(&config)?;
    validate_surface_actions(&config)?;
    Ok(config)
}

/// Re-check, at execution time, the tie between `metadata.mcp` and the dependency the Art
/// declares. The installer already enforces this (`crates/loom_tool_registry/src/install.rs`,
/// `validate_mcp_execution_dependency`), but the installer only sees the package it installs;
/// this sees the manifest that is about to run, so a manifest edited in place after installation
/// fails closed here instead of running against a server nobody declared.
fn validate_declared_dependency(
    config: &McpArtConfig,
    dependencies: &ArtDependencies,
) -> Result<(), String> {
    let package_id = config.package_id.trim();
    let declared = dependencies
        .mcp_servers
        .iter()
        .filter(|dependency| dependency.id.trim() == package_id)
        .collect::<Vec<_>>();
    if declared.len() != 1 {
        return Err(format!(
            "MCP Art metadata.mcp.packageId `{package_id}` needs exactly one matching metadata.dependencies.mcpServers entry, found {}",
            declared.len()
        ));
    }
    if declared[0].version.trim() != config.version.trim() {
        return Err(format!(
            "MCP Art metadata.mcp.version `{}` disagrees with the declared dependency version `{}` for `{package_id}`",
            config.version.trim(),
            declared[0].version.trim()
        ));
    }
    Ok(())
}

/// The half-open version range a declared requirement admits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct VersionBounds {
    lower: [u64; 3],
    upper: [u64; 3],
}

impl VersionBounds {
    /// `false` only when the concrete version is certainly outside the range. A version with a
    /// pre-release tag is admitted rather than judged, because pre-release ordering is exactly
    /// where a hand-written comparison would diverge from the host's real one.
    fn admits(&self, version: &str) -> bool {
        match parse_release_version(version) {
            Some(parsed) => parsed >= self.lower && parsed < self.upper,
            None => true,
        }
    }
}

/// Derive the half-open range for the comparator forms Art manifests actually use: a single `=`,
/// `^`, `~` or bare comparator over one to three numeric components.
///
/// Returns `None` for everything else — conjunctions, inequality comparators, wildcards and
/// pre-release comparators — which means "not checked here", not "satisfied". That is deliberate.
/// The authoritative containment check belongs to the host and already runs with the real `semver`
/// crate in `crates/loom_tool_registry/src/framework_process.rs` before the dependency is
/// resolved; this one exists so that a resolved server the Art never declared cannot slip through
/// a host that skipped, lost or predates that check. It is therefore written to be *sound* rather
/// than complete: it must never reject a version the host would accept, so anything it cannot
/// decide exactly it admits.
///
/// When `semver` can be added to this package's manifest, delete both this and
/// `parse_release_version` and use `VersionReq::parse(requirement)` with `Version::parse(version)`
/// instead. The dependency is absent today only because regenerating
/// `framework-packages/runtime-host/Cargo.lock` is blocked by another lane's uncommitted crate;
/// see F13 and H11 in `docs/progress/phase-78-lane-sync.md`.
fn requirement_bounds(requirement: &str) -> Option<VersionBounds> {
    let requirement = requirement.trim();
    if requirement.contains(',') {
        return None;
    }
    let (operator, rest) = match requirement.chars().next()? {
        '^' => ('^', &requirement[1..]),
        '~' => ('~', &requirement[1..]),
        '=' => ('=', &requirement[1..]),
        '0'..='9' => ('^', requirement),
        _ => return None,
    };
    let components = parse_version_components(rest.trim())?;
    let mut lower = [0_u64; 3];
    for (index, component) in components.iter().enumerate() {
        lower[index] = *component;
    }
    let upper = match operator {
        '=' => match components.len() {
            3 => [lower[0], lower[1], lower[2].saturating_add(1)],
            2 => [lower[0], lower[1].saturating_add(1), 0],
            _ => [lower[0].saturating_add(1), 0, 0],
        },
        '~' => match components.len() {
            1 => [lower[0].saturating_add(1), 0, 0],
            _ => [lower[0], lower[1].saturating_add(1), 0],
        },
        // Caret, including the bare form, bumps the leftmost non-zero component the requirement
        // actually spelled out — so `^0.1` allows `0.1.9` but not `0.2.0`.
        _ => {
            if lower[0] > 0 {
                [lower[0].saturating_add(1), 0, 0]
            } else if components.len() == 1 {
                [1, 0, 0]
            } else if lower[1] > 0 {
                [0, lower[1].saturating_add(1), 0]
            } else if components.len() == 2 {
                [0, 1, 0]
            } else {
                [0, 0, lower[2].saturating_add(1)]
            }
        }
    };
    Some(VersionBounds { lower, upper })
}

/// A concrete version as three numeric components, or `None` when it carries a pre-release tag or
/// anything else this framework will not judge. Build metadata is dropped, because semver ignores
/// it when comparing.
fn parse_release_version(version: &str) -> Option<[u64; 3]> {
    let version = version.trim();
    let version = version.split_once('+').map_or(version, |(head, _)| head);
    let components = parse_version_components(version)?;
    let mut parsed = [0_u64; 3];
    for (index, component) in components.iter().enumerate() {
        parsed[index] = *component;
    }
    Some(parsed)
}

fn parse_version_components(value: &str) -> Option<Vec<u64>> {
    let components = value
        .split('.')
        .map(|component| {
            (!component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| component.parse::<u64>().ok())
                .flatten()
        })
        .collect::<Option<Vec<_>>>()?;
    (1..=3).contains(&components.len()).then_some(components)
}

fn format_version(version: [u64; 3]) -> String {
    format!("{}.{}.{}", version[0], version[1], version[2])
}

fn validate_call_config(config: &McpArtConfig) -> Result<(), String> {
    if config.calls.is_empty() {
        let Some(tool_name) = config.tool_name.as_deref() else {
            return Err(
                "MCP Art metadata.mcp.toolName or metadata.mcp.calls is required".to_owned(),
            );
        };
        // The legacy single-call shape used to be checked for emptiness only, so a megabyte-long
        // tool name or one carrying control characters reached `tools/call` verbatim on this path
        // while the multi-call path rejected it.
        return validate_tool_name(tool_name, "default");
    }
    if config.tool_name.is_some() {
        return Err(
            "MCP Art metadata.mcp.toolName cannot be combined with metadata.mcp.calls".to_owned(),
        );
    }
    if config.calls.len() > 8 {
        return Err("MCP Art metadata.mcp.calls cannot contain more than 8 calls".to_owned());
    }

    let mut ids = BTreeSet::new();
    for call in &config.calls {
        validate_identifier(&call.id, "metadata.mcp.calls[].id")?;
        if !ids.insert(call.id.as_str()) {
            return Err(format!("duplicate MCP call id `{}`", call.id));
        }
        validate_tool_name(&call.tool_name, &call.id)?;
        validate_argument_object(
            &call.arguments,
            &format!("metadata.mcp.calls[{}].arguments", call.id),
        )?;
    }
    Ok(())
}

/// Store the bytes validation accepted. `validate_identifier` trims before checking, so a call
/// declared as `" quote"` used to pass validation and then never match the `"quote"` a Surface
/// action selects — and the untrimmed id was also the key serialized back to the Surface in the
/// `results` map. Normalizing once here keeps validation, comparison, and output on the same bytes.
fn normalize_config(config: &mut McpArtConfig) -> Result<(), String> {
    trim_in_place(&mut config.server_id);
    trim_in_place(&mut config.package_id);
    trim_in_place(&mut config.version);
    if let Some(tool_name) = config.tool_name.as_mut() {
        trim_in_place(tool_name);
    }
    for call in &mut config.calls {
        trim_in_place(&mut call.id);
        trim_in_place(&mut call.tool_name);
    }
    let mut actions = BTreeMap::new();
    for (action_id, mut action) in std::mem::take(&mut config.surface_actions) {
        if let Some(selected_calls) = action.calls.as_mut() {
            for call_id in selected_calls.iter_mut() {
                trim_in_place(call_id);
            }
        }
        let action_id = action_id.trim().to_owned();
        // Two ids that differ only in whitespace collapse to one key here, and silently keeping
        // whichever sorted last would drop a declared action.
        if actions.insert(action_id.clone(), action).is_some() {
            return Err(format!(
                "duplicate MCP Surface action id `{action_id}` after trimming"
            ));
        }
    }
    config.surface_actions = actions;
    Ok(())
}

fn trim_in_place(value: &mut String) {
    if value.trim().len() != value.len() {
        *value = value.trim().to_owned();
    }
}

fn validate_tool_name(tool_name: &str, call_id: &str) -> Result<(), String> {
    if tool_name.trim().is_empty() || tool_name.len() > 256 {
        return Err(format!(
            "MCP call `{call_id}` must declare a non-empty toolName"
        ));
    }
    if tool_name.chars().any(char::is_control) {
        return Err(format!("MCP call `{call_id}` has an invalid toolName"));
    }
    Ok(())
}

fn validate_surface_actions(config: &McpArtConfig) -> Result<(), String> {
    if config.surface_actions.len() > 32 {
        return Err(
            "MCP Art metadata.mcp.surfaceActions cannot contain more than 32 actions".to_owned(),
        );
    }
    let call_ids = if config.calls.is_empty() {
        BTreeSet::from(["default"])
    } else {
        config.calls.iter().map(|call| call.id.as_str()).collect()
    };
    for (action_id, action) in &config.surface_actions {
        validate_identifier(action_id, "metadata.mcp.surfaceActions action id")?;
        if let Some(selected_calls) = &action.calls {
            if selected_calls.len() > 8 {
                return Err(format!(
                    "MCP Surface action `{action_id}` cannot select more than 8 calls"
                ));
            }
            let mut selected_ids = BTreeSet::new();
            for call_id in selected_calls {
                if !selected_ids.insert(call_id.as_str()) {
                    return Err(format!(
                        "MCP Surface action `{action_id}` selects call `{call_id}` more than once"
                    ));
                }
                if !call_ids.contains(call_id.as_str()) {
                    return Err(format!(
                        "MCP Surface action `{action_id}` selects unknown call `{call_id}`"
                    ));
                }
            }
        }
        if action.arguments.len() > 32 {
            return Err(format!(
                "MCP Surface action `{action_id}` cannot bind more than 32 arguments"
            ));
        }
        for (argument_name, binding) in &action.arguments {
            validate_argument_name(argument_name)?;
            if binding.from.is_empty() || binding.from.len() > 4 {
                return Err(format!(
                    "MCP Surface argument `{argument_name}` must declare 1 to 4 source paths"
                ));
            }
            for path in &binding.from {
                validate_binding_path(path)?;
            }
        }
    }
    Ok(())
}

fn validate_argument_object(value: &Value, label: &str) -> Result<(), String> {
    if value.is_null() || value.is_object() {
        Ok(())
    } else {
        Err(format!("MCP Art {label} must be a JSON object"))
    }
}

fn validate_identifier(value: &str, label: &str) -> Result<(), String> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    valid
        .then_some(())
        .ok_or_else(|| format!("invalid {label} `{value}`"))
}

fn validate_argument_name(value: &str) -> Result<(), String> {
    let mut bytes = value.bytes();
    let valid = value.len() <= 128
        && bytes
            .next()
            .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric());
    valid
        .then_some(())
        .ok_or_else(|| format!("invalid MCP Surface argument name `{value}`"))
}

fn validate_binding_path(path: &str) -> Result<(), String> {
    let segments = path.split('.').collect::<Vec<_>>();
    let root_is_allowed = matches!(
        segments.first().copied(),
        Some("payload" | "authoritativeState")
    );
    let segments_are_safe = segments.len() >= 2
        && segments.len() <= 8
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && segment.len() <= 64
                && segment
                    .bytes()
                    .all(|byte| byte == b'_' || byte == b'-' || byte.is_ascii_alphanumeric())
        });
    if root_is_allowed && segments_are_safe {
        Ok(())
    } else {
        Err(format!(
            "MCP Surface binding path `{path}` must be rooted at payload or authoritativeState"
        ))
    }
}

fn build_environment(
    request: &FrameworkExecuteRequest,
    config: &FrameworkMcpServer,
) -> Result<BTreeMap<String, String>, String> {
    let mut environment = config
        .env
        .iter()
        .map(|(name, value)| {
            validate_environment_name(name)?;
            Ok((
                name.clone(),
                expand_runtime_paths(value, request, &request.art_dir),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;

    for (environment_name, credential_name) in &config.credential_env {
        validate_environment_name(environment_name)?;
        let credential_name = credential_name.trim();
        if credential_name.is_empty() {
            return Err(format!(
                "MCP credential mapping for `{environment_name}` is empty"
            ));
        }
        let credential = request
            .context
            .credentials
            .iter()
            .find(|credential| credential.name == credential_name)
            .ok_or_else(|| {
                format!(
                    "MCP Art requires credential `{credential_name}` for `{environment_name}`; available aliases: {}",
                    available_credential_aliases(request)
                )
            })?;
        environment.insert(environment_name.clone(), credential.value.clone());
    }
    for (environment_name, credential_name) in &config.optional_credential_env {
        validate_environment_name(environment_name)?;
        // A name in both maps means the optional mapping would silently overwrite the required one
        // with a different credential, and only for operators who happen to hold that alias.
        if config.credential_env.contains_key(environment_name) {
            return Err(format!(
                "MCP credential mapping for `{environment_name}` is declared as both required and optional"
            ));
        }
        let credential_name = credential_name.trim();
        if credential_name.is_empty() {
            return Err(format!(
                "MCP optional credential mapping for `{environment_name}` is empty"
            ));
        }
        if let Some(credential) = request
            .context
            .credentials
            .iter()
            .find(|credential| credential.name == credential_name)
        {
            environment.insert(environment_name.clone(), credential.value.clone());
        }
    }
    Ok(environment)
}

fn build_headers(
    request: &FrameworkExecuteRequest,
    config: &FrameworkMcpServer,
) -> Result<BTreeMap<String, String>, String> {
    let mut headers = config
        .headers
        .iter()
        .map(|(name, value)| {
            validate_header_name(name)?;
            Ok((
                name.clone(),
                expand_runtime_paths(value, request, &request.art_dir),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    for (header_name, credential_name) in &config.credential_headers {
        validate_header_name(header_name)?;
        let credential_name = credential_name.trim();
        if credential_name.is_empty() {
            return Err(format!(
                "MCP credential mapping for header `{header_name}` is empty"
            ));
        }
        let credential = request
            .context
            .credentials
            .iter()
            .find(|credential| credential.name == credential_name)
            .ok_or_else(|| {
                format!(
                    "MCP Art requires credential `{credential_name}` for `{header_name}`; available aliases: {}",
                    available_credential_aliases(request)
                )
            })?;
        headers.insert(header_name.clone(), credential.value.clone());
    }
    for (header_name, credential_name) in &config.optional_credential_headers {
        validate_header_name(header_name)?;
        // Same collision as `build_environment`: the optional mapping must not be able to redirect
        // a required credential header to a weaker alias.
        if config.credential_headers.contains_key(header_name) {
            return Err(format!(
                "MCP credential mapping for header `{header_name}` is declared as both required and optional"
            ));
        }
        let credential_name = credential_name.trim();
        if credential_name.is_empty() {
            return Err(format!(
                "MCP optional credential mapping for header `{header_name}` is empty"
            ));
        }
        if let Some(credential) = request
            .context
            .credentials
            .iter()
            .find(|credential| credential.name == credential_name)
        {
            headers.insert(header_name.clone(), credential.value.clone());
        }
    }
    Ok(headers)
}

fn available_credential_aliases(request: &FrameworkExecuteRequest) -> String {
    let aliases = request
        .context
        .credentials
        .iter()
        .map(|credential| credential.name.as_str())
        .collect::<Vec<_>>();
    if aliases.is_empty() {
        "<none>".to_owned()
    } else {
        aliases.join(", ")
    }
}

fn validate_header_name(name: &str) -> Result<(), String> {
    let valid = !name.trim().is_empty()
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        });
    valid
        .then_some(())
        .ok_or_else(|| format!("invalid MCP HTTP header name `{name}`"))
}

fn validate_environment_name(name: &str) -> Result<(), String> {
    let mut characters = name.chars();
    let valid = characters
        .next()
        .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
        && characters.all(|character| character == '_' || character.is_ascii_alphanumeric());
    if valid {
        Ok(())
    } else {
        Err(format!("invalid MCP environment variable name `{name}`"))
    }
}

fn expand_runtime_paths(value: &str, request: &FrameworkExecuteRequest, art_dir: &Path) -> String {
    value
        .replace("{artDir}", &art_dir.to_string_lossy())
        .replace("{cacheDir}", &request.context.cache_dir.to_string_lossy())
        .replace("{tempDir}", &request.context.temp_dir.to_string_lossy())
}

fn resolve_calls(
    request: &FrameworkExecuteRequest,
    config: &McpArtConfig,
) -> Result<Vec<ResolvedCall>, String> {
    let configured_calls = if config.calls.is_empty() {
        vec![McpCallConfig {
            id: "default".to_owned(),
            tool_name: config
                .tool_name
                .clone()
                .ok_or_else(|| "legacy MCP Art toolName is missing".to_owned())?,
            arguments: Value::Null,
        }]
    } else {
        config.calls.clone()
    };

    let surface_action = find_surface_action(request)?;
    if let Some(surface_action) = surface_action.filter(|_| !config.surface_actions.is_empty()) {
        let action_id = surface_action
            .get("actionId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "MCP Surface invocation actionId is required".to_owned())?;
        let action = config.surface_actions.get(action_id).ok_or_else(|| {
            format!("MCP Surface action `{action_id}` is not declared by this Art")
        })?;
        let mapped_arguments = resolve_surface_argument_bindings(action, surface_action)?;
        let selected_ids = action.calls.clone().unwrap_or_else(|| {
            configured_calls
                .iter()
                .map(|call| call.id.clone())
                .collect()
        });
        let calls_by_id = configured_calls
            .iter()
            .map(|call| (call.id.as_str(), call))
            .collect::<BTreeMap<_, _>>();
        return selected_ids
            .iter()
            .map(|call_id| {
                let call = calls_by_id.get(call_id.as_str()).ok_or_else(|| {
                    format!("MCP Surface action `{action_id}` selected unknown call `{call_id}`")
                })?;
                let mut arguments = Map::new();
                merge_argument_object(&mut arguments, &config.arguments, "metadata.mcp.arguments")?;
                merge_argument_object(
                    &mut arguments,
                    &call.arguments,
                    &format!("metadata.mcp.calls[{call_id}].arguments"),
                )?;
                arguments.extend(
                    mapped_arguments
                        .iter()
                        .map(|(name, value)| (name.clone(), value.clone())),
                );
                for name in &request.disabled_params {
                    arguments.remove(name);
                }
                let arguments = Value::Object(arguments);
                validate_resolved_arguments(&arguments)?;
                Ok(ResolvedCall {
                    id: call.id.clone(),
                    tool_name: call.tool_name.clone(),
                    arguments,
                })
            })
            .collect();
    }

    let caller_allowlist = declared_argument_allowlist(config);
    configured_calls
        .into_iter()
        .map(|call| {
            let arguments = build_call_arguments(
                request,
                &config.arguments,
                &call.arguments,
                &call.id,
                caller_allowlist.as_ref(),
            )?;
            validate_resolved_arguments(&arguments)?;
            Ok(ResolvedCall {
                id: call.id,
                tool_name: call.tool_name,
                arguments,
            })
        })
        .collect()
}

/// The argument names an Art that declares Surface bindings is allowed to receive from the caller.
///
/// An Art that declares `surfaceActions` has told the host exactly which arguments a caller may
/// influence, and `resolve_surface_argument_bindings` holds callers to that list on the Surface
/// path. Without this, the ordinary path — any execution that carries no `surfaceAction`, which is
/// every plain render — merged `inputs` and `params` wholesale, so the allowlist was worth nothing
/// to anyone who simply left the invocation object out.
///
/// The allowlist is the union of every argument name the manifest spells out: `metadata.mcp`
/// defaults, per-call defaults, and every Surface binding target. It is not the Surface bindings
/// alone, because the plain path has no invocation object to bind against — Stock Monitor's `code`
/// arrives as an Art param there and only as `payload.code` on the Surface path — so
/// bindings-only would leave that Art unable to run at all outside a Surface action.
///
/// `None` means "no allowlist": an Art that declares no bindings has expressed no policy, and
/// filtering it would break every existing MCP Art that passes its params straight through.
fn declared_argument_allowlist(config: &McpArtConfig) -> Option<BTreeSet<String>> {
    if config.surface_actions.is_empty() {
        return None;
    }
    let mut names = BTreeSet::new();
    collect_argument_names(&config.arguments, &mut names);
    for call in &config.calls {
        collect_argument_names(&call.arguments, &mut names);
    }
    for action in config.surface_actions.values() {
        names.extend(action.arguments.keys().cloned());
    }
    Some(names)
}

fn collect_argument_names(arguments: &Value, names: &mut BTreeSet<String>) {
    if let Some(arguments) = arguments.as_object() {
        names.extend(arguments.keys().cloned());
    }
}

fn find_surface_action(request: &FrameworkExecuteRequest) -> Result<Option<&Value>, String> {
    let from_inputs = request.inputs.get(SURFACE_ACTION_KEY);
    let from_params = request.params.get(SURFACE_ACTION_KEY);
    match (from_inputs, from_params) {
        (Some(left), Some(right)) if left != right => {
            Err("conflicting MCP Surface invocations were provided".to_owned())
        }
        (Some(value), _) | (_, Some(value)) => {
            if value.is_object() {
                Ok(Some(value))
            } else {
                Err("MCP Surface invocation must be a JSON object".to_owned())
            }
        }
        (None, None) => Ok(None),
    }
}

fn resolve_surface_argument_bindings(
    action: &McpSurfaceActionConfig,
    invocation: &Value,
) -> Result<Map<String, Value>, String> {
    action
        .arguments
        .iter()
        .map(|(argument_name, binding)| {
            let value = binding
                .from
                .iter()
                .find_map(|path| value_at_binding_path(invocation, path))
                .ok_or_else(|| {
                    format!(
                        "MCP Surface argument `{argument_name}` is missing from all declared source paths"
                    )
                })?;
            validate_bound_value(argument_name, value)?;
            Ok((argument_name.clone(), value.clone()))
        })
        .collect()
}

fn value_at_binding_path<'a>(invocation: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = invocation;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    (!current.is_null()).then_some(current)
}

fn validate_bound_value(argument_name: &str, value: &Value) -> Result<(), String> {
    let encoded = serde_json::to_vec(value).map_err(|error| {
        format!("cannot encode MCP Surface argument `{argument_name}`: {error}")
    })?;
    if encoded.len() > 65_536 {
        return Err(format!(
            "MCP Surface argument `{argument_name}` exceeds the 64 KiB limit"
        ));
    }
    if !value_is_within_depth(value, 0, 16) {
        return Err(format!(
            "MCP Surface argument `{argument_name}` exceeds the nesting limit"
        ));
    }
    Ok(())
}

fn value_is_within_depth(value: &Value, depth: usize, max_depth: usize) -> bool {
    if depth > max_depth {
        return false;
    }
    match value {
        Value::Array(values) => values
            .iter()
            .all(|value| value_is_within_depth(value, depth + 1, max_depth)),
        Value::Object(values) => values
            .values()
            .all(|value| value_is_within_depth(value, depth + 1, max_depth)),
        _ => true,
    }
}

fn validate_resolved_arguments(arguments: &Value) -> Result<(), String> {
    let encoded = serde_json::to_vec(arguments)
        .map_err(|error| format!("cannot encode resolved MCP arguments: {error}"))?;
    if encoded.len() > 262_144 {
        return Err("resolved MCP arguments exceed the 256 KiB limit".to_owned());
    }
    if !value_is_within_depth(arguments, 0, 24) {
        return Err("resolved MCP arguments exceed the nesting limit".to_owned());
    }
    Ok(())
}

#[cfg(test)]
fn build_arguments(request: &FrameworkExecuteRequest, configured: &Value) -> Result<Value, String> {
    build_call_arguments(request, configured, &Value::Null, "default", None)
}

fn build_call_arguments(
    request: &FrameworkExecuteRequest,
    configured: &Value,
    call_configured: &Value,
    call_id: &str,
    caller_allowlist: Option<&BTreeSet<String>>,
) -> Result<Value, String> {
    let mut arguments = Map::new();
    merge_argument_object(&mut arguments, configured, "metadata.mcp.arguments")?;
    merge_argument_object(
        &mut arguments,
        call_configured,
        &format!("metadata.mcp.calls[{call_id}].arguments"),
    )?;
    merge_caller_arguments(&mut arguments, &request.inputs, "inputs", caller_allowlist)?;
    merge_caller_arguments(&mut arguments, &request.params, "params", caller_allowlist)?;
    for name in &request.disabled_params {
        arguments.remove(name);
    }
    Ok(Value::Object(arguments))
}

/// Merge caller-supplied values, minus the Surface control key and minus anything outside the
/// Art's declared argument names when it has declared any.
fn merge_caller_arguments(
    target: &mut Map<String, Value>,
    source: &Value,
    label: &str,
    allowlist: Option<&BTreeSet<String>>,
) -> Result<(), String> {
    if source.is_null() {
        return Ok(());
    }
    let source = source
        .as_object()
        .ok_or_else(|| format!("MCP Art {label} must be a JSON object"))?;
    for (name, value) in source {
        // `surfaceAction` is how a caller addresses this framework, never a tool argument. An Art
        // that declares no `surfaceActions` used to forward the whole invocation object — payload,
        // authoritative state and all — to the MCP server under that name.
        if name == SURFACE_ACTION_KEY {
            continue;
        }
        if allowlist.is_some_and(|allowlist| !allowlist.contains(name)) {
            continue;
        }
        target.insert(name.clone(), value.clone());
    }
    Ok(())
}

fn merge_argument_object(
    target: &mut Map<String, Value>,
    source: &Value,
    label: &str,
) -> Result<(), String> {
    if source.is_null() {
        return Ok(());
    }
    let source = source
        .as_object()
        .ok_or_else(|| format!("MCP Art {label} must be a JSON object"))?;
    target.extend(
        source
            .iter()
            .map(|(name, value)| (name.clone(), value.clone())),
    );
    Ok(())
}

fn execute_tools(server: &McpServerConfig, calls: &[ResolvedCall]) -> Result<Vec<Value>, String> {
    let mut client = McpClient::connect(server)
        .map_err(|error| format!("failed to connect MCP server: {error}"))?;
    let result = (|| {
        client
            .initialize()
            .map_err(|error| format!("MCP initialize failed: {error}"))?;
        let tools = client
            .list_tools()
            .map_err(|error| format!("MCP tools/list failed: {error}"))?;
        calls
            .iter()
            .map(|call| {
                let schema = find_tool_input_schema(&tools, &call.tool_name).ok_or_else(|| {
                    format!("MCP server does not expose tool `{}`", call.tool_name)
                })?;
                let normalized_arguments = normalize_arguments(&call.arguments, schema);
                client
                    .call_tool(&call.tool_name, normalized_arguments)
                    .map_err(|error| format!("MCP tools/call `{}` failed: {error}", call.tool_name))
            })
            .collect()
    })();
    client.cancel();
    result
}

fn find_tool_input_schema<'a>(tools: &'a Value, tool_name: &str) -> Option<&'a Value> {
    tools
        .get("tools")?
        .as_array()?
        .iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some(tool_name))
        .and_then(|tool| tool.get("inputSchema"))
}

fn normalize_arguments(arguments: &Value, schema: &Value) -> Value {
    let Some(arguments) = arguments.as_object() else {
        return arguments.clone();
    };
    let properties = schema.get("properties").and_then(Value::as_object);
    let has_pattern_or_composed_properties =
        ["patternProperties", "allOf", "anyOf", "oneOf", "$ref"]
            .iter()
            .any(|name| schema.get(*name).is_some());
    let rejects_undeclared = schema.get("additionalProperties") == Some(&Value::Bool(false))
        && !has_pattern_or_composed_properties;
    Value::Object(
        arguments
            .iter()
            .filter(|(name, _)| {
                !rejects_undeclared
                    || properties.is_some_and(|properties| properties.contains_key(*name))
            })
            .map(|(name, value)| {
                let schema = properties.and_then(|properties| properties.get(name));
                (name.clone(), normalize_argument(name, value, schema))
            })
            .collect(),
    )
}

fn normalize_argument(name: &str, value: &Value, schema: Option<&Value>) -> Value {
    if name.eq_ignore_ascii_case("search_lang") {
        if let Some(raw) = value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let mapped = match raw.to_ascii_lowercase().as_str() {
                "zh" | "zh-cn" => "zh-hans",
                "zh-tw" => "zh-hant",
                _ => raw,
            };
            if let Some(canonical) = canonical_enum_value(schema, mapped) {
                return Value::String(canonical);
            }
            if mapped != raw {
                return Value::String(mapped.to_owned());
            }
        }
    }
    let Some(schema) = schema else {
        return value.clone();
    };
    if schema_type_matches(schema, "integer") {
        if let Some(parsed) = value
            .as_i64()
            .or_else(|| value.as_str().and_then(|value| value.trim().parse().ok()))
        {
            return Value::from(parsed);
        }
    }
    if schema_type_matches(schema, "number") {
        if let Some(parsed) = value
            .as_f64()
            .or_else(|| value.as_str().and_then(|value| value.trim().parse().ok()))
        {
            return Value::from(parsed);
        }
    }
    if schema_type_matches(schema, "boolean") {
        if let Some(parsed) = value.as_bool().or_else(|| {
            value
                .as_str()
                .and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
                    "1" | "true" | "yes" | "on" => Some(true),
                    "0" | "false" | "no" | "off" => Some(false),
                    _ => None,
                })
        }) {
            return Value::from(parsed);
        }
    }
    if let Some(raw) = value.as_str() {
        if let Some(canonical) = canonical_enum_value(Some(schema), raw) {
            return Value::String(canonical);
        }
    }
    value.clone()
}

fn canonical_enum_value(schema: Option<&Value>, expected: &str) -> Option<String> {
    schema?
        .get("enum")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .find(|candidate| candidate.eq_ignore_ascii_case(expected))
        .map(str::to_owned)
}

fn schema_type_matches(schema: &Value, expected: &str) -> bool {
    match schema.get("type") {
        Some(Value::String(actual)) => actual == expected,
        Some(Value::Array(actual)) => actual.iter().any(|value| value.as_str() == Some(expected)),
        _ => false,
    }
}

fn redact_credentials(mut message: String, credentials: &[CredentialGrant]) -> String {
    let mut values = credentials
        .iter()
        .map(|credential| credential.value.as_str())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    values.sort_unstable();
    values.dedup();
    values.sort_unstable_by_key(|value| std::cmp::Reverse(value.len()));
    for value in values {
        message = message.replace(value, "[REDACTED]");
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_protocol::FrameworkExecutionContext;
    use serde_json::json;
    use std::path::PathBuf;
    #[cfg(windows)]
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn request() -> FrameworkExecuteRequest {
        FrameworkExecuteRequest {
            protocol_version: "loom.framework.v1".to_owned(),
            supported_protocol_versions: vec!["loom.framework.v1".to_owned()],
            framework_id: "mcp".to_owned(),
            art_id: "fixture".to_owned(),
            art_dir: PathBuf::from("art"),
            inputs: json!({ "input": "from-input", "disabled": true }),
            params: json!({ "query": "loom", "count": "2" }),
            disabled_params: vec!["disabled".to_owned()],
            context: FrameworkExecutionContext {
                mcp_server: Some(resolved_server()),
                credentials: vec![CredentialGrant {
                    name: "api_key".to_owned(),
                    value: "secret-value".to_owned(),
                    expires_at: None,
                }],
                ..FrameworkExecutionContext::default()
            },
        }
    }

    fn resolved_server() -> FrameworkMcpServer {
        FrameworkMcpServer {
            id: "fixture".to_owned(),
            package_id: "publisher.test/fixture".to_owned(),
            version: "1.0.0".to_owned(),
            transport: "stdio".to_owned(),
            command: "fixture".to_owned(),
            credential_env: BTreeMap::from([("BRAVE_API_KEY".to_owned(), "api_key".to_owned())]),
            ..FrameworkMcpServer::default()
        }
    }

    #[test]
    fn arguments_merge_defaults_inputs_and_params() {
        let arguments = build_arguments(
            &request(),
            &json!({ "query": "default", "safesearch": "strict" }),
        )
        .unwrap();
        assert_eq!(
            arguments,
            json!({
                "input": "from-input",
                "query": "loom",
                "count": "2",
                "safesearch": "strict"
            })
        );
    }

    fn multi_call_config() -> McpArtConfig {
        McpArtConfig {
            server_id: "stock-api".to_owned(),
            package_id: "neuro.official/stock-api".to_owned(),
            version: "=2.9.0".to_owned(),
            tool_name: None,
            arguments: json!({ "source": "auto" }),
            calls: vec![
                McpCallConfig {
                    id: "quote".to_owned(),
                    tool_name: "get_stock".to_owned(),
                    arguments: Value::Null,
                },
                McpCallConfig {
                    id: "history".to_owned(),
                    tool_name: "get_klines".to_owned(),
                    arguments: json!({ "period": "day", "count": 60 }),
                },
            ],
            surface_actions: BTreeMap::from([
                (
                    "stock_refresh".to_owned(),
                    McpSurfaceActionConfig {
                        calls: None,
                        arguments: BTreeMap::from([(
                            "code".to_owned(),
                            McpArgumentBinding {
                                from: vec!["authoritativeState.code".to_owned()],
                            },
                        )]),
                    },
                ),
                (
                    "stock_symbol_commit".to_owned(),
                    McpSurfaceActionConfig {
                        calls: None,
                        arguments: BTreeMap::from([(
                            "code".to_owned(),
                            McpArgumentBinding {
                                from: vec![
                                    "payload.value".to_owned(),
                                    "authoritativeState.code".to_owned(),
                                ],
                            },
                        )]),
                    },
                ),
                (
                    "stock_interval_commit".to_owned(),
                    McpSurfaceActionConfig {
                        calls: Some(Vec::new()),
                        arguments: BTreeMap::new(),
                    },
                ),
            ]),
        }
    }

    #[test]
    fn surface_action_maps_only_declared_values_into_multiple_calls() {
        let mut request = request();
        request.inputs = json!({
            "surfaceAction": {
                "actionId": "stock_symbol_commit",
                "payload": { "value": "SZ000034", "ignored": "do-not-forward" },
                "authoritativeState": { "code": "SH600000" }
            },
            "untrusted": "do-not-forward"
        });
        request.params = json!({ "alsoUntrusted": true });

        let calls = resolve_calls(&request, &multi_call_config()).unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "quote");
        assert_eq!(
            calls[0].arguments,
            json!({ "source": "auto", "code": "SZ000034" })
        );
        assert_eq!(calls[1].id, "history");
        assert_eq!(
            calls[1].arguments,
            json!({
                "source": "auto",
                "period": "day",
                "count": 60,
                "code": "SZ000034"
            })
        );
    }

    #[test]
    fn surface_action_can_explicitly_skip_mcp_calls() {
        let mut request = request();
        request.inputs = json!({
            "surfaceAction": {
                "actionId": "stock_interval_commit",
                "payload": { "value": 120 },
                "authoritativeState": { "code": "SZ000034" }
            }
        });
        request.params = json!({});

        let calls = resolve_calls(&request, &multi_call_config()).unwrap();
        assert!(calls.is_empty());
    }

    #[test]
    fn surface_bindings_reject_context_and_credential_paths() {
        let mut config = multi_call_config();
        config.surface_actions.insert(
            "stock_unsafe".to_owned(),
            McpSurfaceActionConfig {
                calls: None,
                arguments: BTreeMap::from([(
                    "code".to_owned(),
                    McpArgumentBinding {
                        from: vec!["context.credentials.0.value".to_owned()],
                    },
                )]),
            },
        );
        assert!(validate_surface_actions(&config)
            .unwrap_err()
            .contains("payload or authoritativeState"));
    }

    #[test]
    fn credential_alias_maps_to_server_environment() {
        let config = resolved_server();
        let environment = build_environment(&request(), &config).unwrap();
        assert_eq!(
            environment.get("BRAVE_API_KEY"),
            Some(&"secret-value".to_owned())
        );
    }

    #[test]
    fn credential_mapping_declared_required_and_optional_is_rejected() {
        let mut config = resolved_server();
        config
            .optional_credential_env
            .insert("BRAVE_API_KEY".to_owned(), "weaker_alias".to_owned());
        assert!(build_environment(&request(), &config)
            .unwrap_err()
            .contains("both required and optional"));

        let mut config = resolved_server();
        config
            .credential_headers
            .insert("X-Api-Key".to_owned(), "api_key".to_owned());
        config
            .optional_credential_headers
            .insert("X-Api-Key".to_owned(), "weaker_alias".to_owned());
        assert!(build_headers(&request(), &config)
            .unwrap_err()
            .contains("both required and optional"));
    }

    fn declared_dependency(id: &str, version: &str) -> ArtDependencies {
        ArtDependencies {
            mcp_servers: vec![ArtMcpServerDependency {
                id: id.to_owned(),
                version: version.to_owned(),
            }],
        }
    }

    #[test]
    fn declared_dependency_must_match_the_mcp_package_and_version() {
        let config = multi_call_config();
        validate_declared_dependency(
            &config,
            &declared_dependency("neuro.official/stock-api", "=2.9.0"),
        )
        .unwrap();

        assert!(
            validate_declared_dependency(&config, &ArtDependencies::default())
                .unwrap_err()
                .contains("found 0")
        );
        assert!(validate_declared_dependency(
            &config,
            &declared_dependency("neuro.official/stock-api", "=2.9.1")
        )
        .unwrap_err()
        .contains("disagrees with the declared dependency version"));
    }

    #[test]
    fn requirement_bounds_cover_the_comparator_forms_art_manifests_use() {
        for (requirement, lower, upper) in [
            ("=2.9.0", [2, 9, 0], [2, 9, 1]),
            ("=2.9", [2, 9, 0], [2, 10, 0]),
            ("^0.1", [0, 1, 0], [0, 2, 0]),
            ("^0.0.3", [0, 0, 3], [0, 0, 4]),
            ("^0", [0, 0, 0], [1, 0, 0]),
            ("1.2.3", [1, 2, 3], [2, 0, 0]),
            ("~1.2", [1, 2, 0], [1, 3, 0]),
            ("~1.2.3", [1, 2, 3], [1, 3, 0]),
            ("~1", [1, 0, 0], [2, 0, 0]),
        ] {
            assert_eq!(
                requirement_bounds(requirement),
                Some(VersionBounds { lower, upper }),
                "{requirement}"
            );
        }
    }

    #[test]
    fn undecidable_requirements_are_left_to_the_authoritative_checker() {
        // `None` means "this framework will not judge it", which is why nothing downstream may read
        // it as "satisfied".
        for requirement in [
            ">=1.0.0",
            "<2",
            "*",
            ">=1.0.0, <2.0.0",
            "1.x",
            "^1.0.0-rc.1",
            "",
        ] {
            assert_eq!(requirement_bounds(requirement), None, "{requirement}");
        }
    }

    #[test]
    fn resolved_version_outside_the_declared_range_is_rejected() {
        let mut config = multi_call_config();
        config.server_id = "fixture".to_owned();
        config.package_id = "publisher.test/fixture".to_owned();
        config.version = "^0.1".to_owned();

        let mut resolved = resolved_server();
        resolved.version = "0.1.9".to_owned();
        validate_resolved_server(&config, &resolved).unwrap();

        resolved.version = "0.2.0".to_owned();
        let error = validate_resolved_server(&config, &resolved).unwrap_err();
        assert!(
            error.contains("does not satisfy Art dependency `^0.1`"),
            "{error}"
        );
        assert!(error.contains("needs >= 0.1.0 and < 0.2.0"), "{error}");

        resolved.version = "   ".to_owned();
        assert!(validate_resolved_server(&config, &resolved)
            .unwrap_err()
            .contains("reported no version"));

        // Build metadata is ignored, pre-release ordering is not judged, and a requirement this
        // framework cannot parse leaves the decision to the host.
        resolved.version = "0.1.4+build.7".to_owned();
        validate_resolved_server(&config, &resolved).unwrap();
        resolved.version = "0.2.0-rc.1".to_owned();
        validate_resolved_server(&config, &resolved).unwrap();
        config.version = ">=0.1.0".to_owned();
        resolved.version = "9.9.9".to_owned();
        validate_resolved_server(&config, &resolved).unwrap();
    }

    #[test]
    fn config_identifiers_are_stored_trimmed_so_selections_match() {
        let mut config = multi_call_config();
        config.version = " =2.9.0 ".to_owned();
        config.calls[0].id = " quote ".to_owned();
        config.calls[0].tool_name = " get_stock ".to_owned();
        config.surface_actions.insert(
            " stock_trimmed ".to_owned(),
            McpSurfaceActionConfig {
                calls: Some(vec![" quote ".to_owned()]),
                arguments: BTreeMap::new(),
            },
        );

        normalize_config(&mut config).unwrap();
        assert_eq!(config.version, "=2.9.0");
        assert_eq!(config.calls[0].id, "quote");
        assert_eq!(config.calls[0].tool_name, "get_stock");
        assert_eq!(
            config.surface_actions["stock_trimmed"].calls.as_deref(),
            Some(["quote".to_owned()].as_slice())
        );
        validate_call_config(&config).unwrap();
        validate_surface_actions(&config).unwrap();
    }

    #[test]
    fn surface_action_ids_that_collide_after_trimming_are_rejected() {
        let mut config = multi_call_config();
        config.surface_actions.insert(
            " stock_refresh".to_owned(),
            McpSurfaceActionConfig::default(),
        );
        assert!(normalize_config(&mut config)
            .unwrap_err()
            .contains("duplicate MCP Surface action id"));
    }

    #[test]
    fn legacy_tool_name_is_held_to_the_multi_call_rules() {
        let mut config = multi_call_config();
        config.calls.clear();
        config.surface_actions.clear();

        config.tool_name = Some("get\u{7}stock".to_owned());
        assert!(validate_call_config(&config)
            .unwrap_err()
            .contains("invalid toolName"));
        config.tool_name = Some("x".repeat(257));
        assert!(validate_call_config(&config)
            .unwrap_err()
            .contains("non-empty toolName"));
        config.tool_name = Some("get_stock".to_owned());
        validate_call_config(&config).unwrap();
    }

    #[test]
    fn plain_calls_drop_caller_arguments_the_art_never_declared() {
        let mut request = request();
        request.inputs = json!({});
        request.params = json!({ "code": "SZ000034", "interval_seconds": 120 });
        request.disabled_params = Vec::new();

        let calls = resolve_calls(&request, &multi_call_config()).unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[0].arguments,
            json!({ "source": "auto", "code": "SZ000034" })
        );
        assert_eq!(
            calls[1].arguments,
            json!({ "source": "auto", "period": "day", "count": 60, "code": "SZ000034" })
        );
    }

    #[test]
    fn surface_invocation_object_is_never_forwarded_as_a_tool_argument() {
        // An Art with no declared actions gets no allowlist, but the control key is still not an
        // argument: the whole invocation object used to reach the MCP server as `surfaceAction`.
        let mut config = multi_call_config();
        config.surface_actions.clear();
        let mut request = request();
        request.inputs = json!({
            "surfaceAction": {
                "actionId": "stock_symbol_commit",
                "payload": { "value": "SZ000034" }
            }
        });
        request.params = json!({ "code": "SZ000034", "interval_seconds": 120 });
        request.disabled_params = Vec::new();

        let calls = resolve_calls(&request, &config).unwrap();
        assert_eq!(calls.len(), 2);
        for call in &calls {
            assert!(call.arguments.get(SURFACE_ACTION_KEY).is_none());
        }
        assert_eq!(
            calls[0].arguments,
            json!({ "source": "auto", "code": "SZ000034", "interval_seconds": 120 })
        );
    }

    #[test]
    fn schema_normalizes_mcp_argument_types() {
        let normalized = normalize_arguments(
            &json!({ "count": "2", "spellcheck": "true", "search_lang": "zh-cn" }),
            &json!({
                "properties": {
                    "count": { "type": "integer" },
                    "spellcheck": { "type": "boolean" },
                    "search_lang": { "type": "string", "enum": ["en", "zh-hans"] }
                }
            }),
        );
        assert_eq!(
            normalized,
            json!({ "count": 2, "spellcheck": true, "search_lang": "zh-hans" })
        );
    }

    #[test]
    fn schema_excludes_arguments_rejected_by_the_mcp_tool() {
        let normalized = normalize_arguments(
            &json!({
                "query": "red panda",
                "count": "2",
                "result_index": 1,
                "__exec_manualTrigger": 123,
                "force_update": 456
            }),
            &json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "count": { "type": "integer" }
                },
                "additionalProperties": false
            }),
        );
        assert_eq!(normalized, json!({ "query": "red panda", "count": 2 }));
    }

    #[test]
    fn schema_preserves_arguments_when_pattern_properties_define_dynamic_keys() {
        let normalized = normalize_arguments(
            &json!({ "header_x": "visible" }),
            &json!({
                "type": "object",
                "properties": {},
                "patternProperties": { "^header_": { "type": "string" } },
                "additionalProperties": false
            }),
        );
        assert_eq!(normalized, json!({ "header_x": "visible" }));
    }

    #[test]
    fn credential_values_are_redacted_from_mcp_errors() {
        assert_eq!(
            redact_credentials(
                "server printed secret-value".to_owned(),
                &request().context.credentials
            ),
            "server printed [REDACTED]"
        );
    }

    #[test]
    fn short_credential_values_are_redacted_from_mcp_errors() {
        assert_eq!(
            redact_credentials(
                "server printed key=abc".to_owned(),
                &[CredentialGrant {
                    name: "short_key".to_owned(),
                    value: "abc".to_owned(),
                    expires_at: None,
                }]
            ),
            "server printed key=[REDACTED]"
        );
    }

    #[cfg(windows)]
    #[test]
    fn independent_image_search_server_executes_through_mcp_framework() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture image API");
        let address = listener.local_addr().expect("fixture image API address");
        let fixture = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept image API request");
            let mut reader = BufReader::new(stream.try_clone().expect("clone fixture stream"));
            let mut request = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).expect("read request line");
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                request.push_str(&line);
            }
            assert!(request.starts_with(
                "GET /res/v1/images/search?q=loom%20framework&count=2&safesearch=strict HTTP/1.1\r\n"
            ));
            assert!(request
                .to_ascii_lowercase()
                .contains("x-subscription-token: fixture-api-key\r\n"));

            let body = json!({
                "results": [
                    {
                        "title": "Loom first",
                        "url": "https://cdn.example.test/image-1.png",
                        "source": "https://example.test/source/1",
                        "thumbnail": { "src": "https://cdn.example.test/thumb-1.jpg" },
                        "properties": {
                            "url": "https://cdn.example.test/image-1.png",
                            "width": 640,
                            "height": 480
                        }
                    },
                    {
                        "title": "Loom second",
                        "url": "https://cdn.example.test/image-2.png",
                        "source": "https://example.test/source/2",
                        "thumbnail": { "src": "https://cdn.example.test/thumb-2.jpg" },
                        "properties": {}
                    }
                ]
            })
            .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .expect("write fixture response");
            stream.flush().expect("flush fixture response");
        });

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let art_dir = std::env::temp_dir().join(format!(
            "loom-image-search-mcp-framework-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&art_dir).expect("create staged Art directory");
        let art_source_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../art-packages/samples/image-search");
        let mcp_source_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../mcp-server-packages/image-search");
        let manifest: Value = serde_json::from_slice(
            &fs::read(art_source_dir.join("manifest.json")).expect("read image-search manifest"),
        )
        .expect("parse image-search manifest");
        fs::write(
            art_dir.join("manifest.json"),
            serde_json::to_vec(&manifest).expect("serialize staged manifest"),
        )
        .expect("write staged manifest");

        let mut request = request();
        request.art_id = "custom-image-search".to_owned();
        request.art_dir = art_dir.clone();
        request.inputs = json!({});
        request.params = json!({
            "query": "loom framework",
            "count": "2",
            "result_index": 1,
            "__exec_manualTrigger": 123,
            "force_update": 456
        });
        request.context.credentials = vec![CredentialGrant {
            name: "brave_api_key".to_owned(),
            value: "fixture-api-key".to_owned(),
            expires_at: None,
        }];
        request.context.mcp_server = Some(FrameworkMcpServer {
            id: "neuro-image-search".to_owned(),
            package_id: "neuro.official/neuro-image-search".to_owned(),
            version: "0.1.0".to_owned(),
            transport: "stdio".to_owned(),
            command: mcp_source_dir
                .join("runtime/image-search-mcp.ps1")
                .display()
                .to_string(),
            // The endpoint is fixed in the server, because whoever picks it picks where the Brave
            // subscription key is sent. Pointing it at the fixture goes through the loopback-only
            // environment override, which a package manifest cannot set.
            env: BTreeMap::from([(
                "LOOM_IMAGE_SEARCH_ENDPOINT_OVERRIDE".to_owned(),
                format!("http://{address}/res/v1/images/search"),
            )]),
            credential_env: BTreeMap::from([(
                "BRAVE_API_KEY".to_owned(),
                "brave_api_key".to_owned(),
            )]),
            ..FrameworkMcpServer::default()
        });

        let execution = execute(&request, &art_dir).expect("execute independent MCP server");
        assert_eq!(execution.server_id, "neuro-image-search");
        assert_eq!(execution.tool_name.as_deref(), Some("brave_image_search"));
        let result = execution.result.as_ref().expect("legacy MCP result");
        assert_eq!(result["structuredContent"]["count"], 2);
        assert!(
            serde_json::to_value(&execution)
                .expect("serialize MCP execution")
                .get("arguments")
                .is_none(),
            "MCP arguments must not be echoed into the Art runtime payload"
        );
        assert_eq!(
            result["structuredContent"]["candidates"][0]["imageUrl"],
            "https://cdn.example.test/image-1.png"
        );
        assert_eq!(
            result["structuredContent"]["candidates"][1]["imageUrl"],
            "https://cdn.example.test/image-2.png"
        );

        fixture.join().expect("image API fixture thread");
        fs::remove_dir_all(&art_dir).expect("remove staged image-search Art");
    }
}
