use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use loom_process::{executable_path_within, ProcessSpec};
use loom_protocol::{
    validate_capability_manifest, CapabilityContributions, CapabilityPackageManifest,
    CapabilityRuntimeMessage, CapabilityRuntimeMethod, CapabilityRuntimeStatus,
    CAPABILITY_API_VERSION, CAPABILITY_RUNTIME_PROTOCOL,
};
use serde_json::{json, Value};

use crate::error::{CapabilityHostError, HostResult};
use crate::host::{
    ActivePackage, CapabilityInvocationOutput, CapabilityRuntimePackage, RuntimeHostLimits,
};
use crate::process::RuntimeProcess;
use crate::verification::verify_runtime_package;

static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(super) fn start_runtime(
    package: &CapabilityRuntimePackage,
) -> HostResult<(RuntimeProcess, CapabilityContributions)> {
    // Reverify immediately before every spawn, including lazy restarts.
    verify_runtime_package(package)?;
    let service = package
        .manifest
        .entrypoints
        .service
        .as_ref()
        .ok_or_else(|| {
            CapabilityHostError::InvalidPackage("service entrypoint is missing".to_owned())
        })?;
    let target = service.targets.get(platform_target()).ok_or_else(|| {
        CapabilityHostError::InvalidPackage(format!(
            "package does not support {}",
            platform_target()
        ))
    })?;
    let executable = executable_path_within(&package.package_dir, Path::new(&target.command))
        .map_err(CapabilityHostError::InvalidPackage)?;
    let mut spec = ProcessSpec::new(executable);
    spec.args = target.args.clone();
    spec.current_dir = Some(package.package_dir.clone());
    spec.limits.timeout = Duration::from_secs(package.manifest.resources.timeout_seconds.max(1));
    spec.limits.memory_bytes = usize::try_from(package.manifest.resources.memory_mib)
        .ok()
        .and_then(|mib| mib.checked_mul(1024 * 1024));
    spec.limits.max_processes = Some(package.manifest.resources.max_processes.max(1));
    spec.limits.stdout_bytes = loom_protocol::CAPABILITY_RUNTIME_FRAME_BYTES;
    spec.limits.stderr_bytes = package
        .manifest
        .resources
        .stderr_kib_per_minute
        .unwrap_or(1024)
        .min(8192) as usize
        * 1024;
    let mut process = RuntimeProcess::spawn(&spec)?;
    let initialize = call_method(
        &mut process,
        CapabilityRuntimeMethod::Initialize,
        json!({
            "pluginId": package.manifest.qualified_id(),
            "packageDigest": &package.digest,
            "permissions": &package.manifest.permissions,
            "staticContributions": &package.manifest.contributes,
        }),
        spec.limits.timeout,
    )?;
    let contributions = effective_contributions(&package.manifest, &initialize)?;
    call_method(
        &mut process,
        CapabilityRuntimeMethod::Activate,
        json!({}),
        spec.limits.timeout,
    )?;
    Ok((process, contributions))
}

pub(super) fn ensure_process(
    active: &mut ActivePackage,
    limits: &RuntimeHostLimits,
) -> HostResult<()> {
    if active.process.is_some() {
        return Ok(());
    }
    if active.failures >= limits.maximum_failures || Instant::now() < active.restart_not_before {
        return Err(CapabilityHostError::Unavailable(
            "runtime restart backoff is active".to_owned(),
        ));
    }
    let (process, contributions) = match start_runtime(&active.package) {
        Ok(started) => started,
        Err(error) => {
            record_failure(active);
            return Err(error);
        }
    };
    validate_dynamic_subset(&active.package.manifest.contributes, &contributions)?;
    active.effective_contributions = contributions;
    active.process = Some(process);
    Ok(())
}

pub(super) fn call_method(
    process: &mut RuntimeProcess,
    method: CapabilityRuntimeMethod,
    payload: Value,
    timeout: Duration,
) -> HostResult<CapabilityRuntimeMessage> {
    let response = process.call(runtime_request(next_request_id(), method, payload), timeout)?;
    match &response {
        CapabilityRuntimeMessage::Response {
            status: CapabilityRuntimeStatus::Succeeded,
            ..
        } => Ok(response),
        CapabilityRuntimeMessage::Response { .. } => Err(CapabilityHostError::Protocol(
            "runtime method reported failure".to_owned(),
        )),
        _ => Err(CapabilityHostError::Protocol(
            "runtime returned a non-response message".to_owned(),
        )),
    }
}

pub(super) fn runtime_request(
    request_id: String,
    method: CapabilityRuntimeMethod,
    payload: Value,
) -> CapabilityRuntimeMessage {
    CapabilityRuntimeMessage::Request {
        protocol: CAPABILITY_RUNTIME_PROTOCOL.to_owned(),
        api_version: CAPABILITY_API_VERSION.to_owned(),
        request_id,
        method,
        payload,
    }
}

pub(super) fn response_output(
    package: &CapabilityRuntimePackage,
    response: CapabilityRuntimeMessage,
) -> HostResult<CapabilityInvocationOutput> {
    let CapabilityRuntimeMessage::Response {
        status,
        payload,
        error,
        ..
    } = response
    else {
        return Err(CapabilityHostError::Protocol(
            "runtime returned a non-response message".to_owned(),
        ));
    };
    Ok(CapabilityInvocationOutput {
        plugin_id: package.manifest.qualified_id(),
        package_digest: package.digest.clone(),
        status,
        payload,
        error: error.map(|mut error| {
            error.message = "capability runtime reported an error".to_owned();
            error
        }),
    })
}

fn effective_contributions(
    manifest: &CapabilityPackageManifest,
    initialize: &CapabilityRuntimeMessage,
) -> HostResult<CapabilityContributions> {
    let CapabilityRuntimeMessage::Response { payload, .. } = initialize else {
        return Err(CapabilityHostError::Protocol(
            "initialize did not return a response".to_owned(),
        ));
    };
    let Some(value) = payload
        .as_ref()
        .and_then(|payload| payload.get("contributions"))
    else {
        return Ok(manifest.contributes.clone());
    };
    let contributions: CapabilityContributions = serde_json::from_value(value.clone())?;
    validate_dynamic_subset(&manifest.contributes, &contributions)?;
    Ok(contributions)
}

fn validate_dynamic_subset(
    static_values: &CapabilityContributions,
    dynamic: &CapabilityContributions,
) -> HostResult<()> {
    if contribution_ids(dynamic).len() != contribution_count(dynamic)
        || !dynamic
            .commands
            .iter()
            .all(|value| static_values.commands.contains(value))
        || !is_exact_subset(&dynamic.shortcuts, &static_values.shortcuts)
        || !is_exact_subset(&dynamic.menus, &static_values.menus)
        || !is_exact_subset(&dynamic.settings, &static_values.settings)
        || !is_exact_subset(&dynamic.data_types, &static_values.data_types)
        || !is_exact_subset(&dynamic.renderers, &static_values.renderers)
        || !is_exact_subset(&dynamic.unit_overlays, &static_values.unit_overlays)
        || !is_exact_subset(&dynamic.background_tasks, &static_values.background_tasks)
        || !is_exact_subset(
            &dynamic.resource_providers,
            &static_values.resource_providers,
        )
        || !is_exact_subset(&dynamic.diagnostics, &static_values.diagnostics)
        || !is_exact_subset(
            &dynamic.event_subscriptions,
            &static_values.event_subscriptions,
        )
    {
        return Err(CapabilityHostError::Protocol(
            "dynamic registration changes or exceeds its signed static envelope".to_owned(),
        ));
    }
    Ok(())
}

fn is_exact_subset<T: PartialEq>(dynamic: &[T], signed: &[T]) -> bool {
    dynamic.iter().all(|value| signed.contains(value))
}

fn contribution_count(contributions: &CapabilityContributions) -> usize {
    contributions.commands.len() + generic_contributions(contributions).count()
}

fn contribution_ids(contributions: &CapabilityContributions) -> HashSet<&str> {
    contributions
        .commands
        .iter()
        .map(|value| value.id.as_str())
        .chain(generic_contributions(contributions).map(|value| value.id.as_str()))
        .collect()
}

fn generic_contributions(
    contributions: &CapabilityContributions,
) -> impl Iterator<Item = &loom_protocol::CapabilityContribution> {
    contributions
        .shortcuts
        .iter()
        .chain(&contributions.menus)
        .chain(&contributions.settings)
        .chain(&contributions.data_types)
        .chain(&contributions.renderers)
        .chain(&contributions.unit_overlays)
        .chain(&contributions.background_tasks)
        .chain(&contributions.resource_providers)
        .chain(&contributions.diagnostics)
        .chain(&contributions.event_subscriptions)
}

pub(super) fn validate_runtime_package(package: &CapabilityRuntimePackage) -> HostResult<()> {
    validate_capability_manifest(&package.manifest)
        .map_err(|error| CapabilityHostError::InvalidPackage(error.to_string()))?;
    if package.digest.len() != 64
        || !package
            .digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(CapabilityHostError::InvalidPackage(
            "package digest is invalid".to_owned(),
        ));
    }
    verify_runtime_package(package)
}

pub(super) fn record_failure(active: &mut ActivePackage) {
    active.failures = active.failures.saturating_add(1);
    let exponent = active.failures.min(5);
    let delay = Duration::from_millis(100u64.saturating_mul(1u64 << exponent));
    active.restart_not_before = Instant::now() + delay;
}

pub(super) fn next_request_id() -> String {
    format!(
        "loom:{}:{}",
        std::process::id(),
        REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn platform_target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x64",
        ("linux", "x86_64") => "linux-x64",
        ("macos", "x86_64") => "macos-x64",
        ("macos", "aarch64") => "macos-arm64",
        _ => "unsupported",
    }
}
