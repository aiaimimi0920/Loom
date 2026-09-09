use std::collections::HashSet;
use std::path::{Component, Path};

use semver::{Version, VersionReq};
use serde_json::Value;
use thiserror::Error;

use super::{CapabilityApiRequirement, CapabilityContribution, CapabilityPackageManifest};
use crate::capability::CAPABILITY_SCHEMA_VERSION;

pub const MAX_CAPABILITY_MANIFEST_BYTES: usize = 256 * 1024;
pub const MAX_CAPABILITY_JSON_DEPTH: usize = 32;
/// Ceilings for the sandbox budgets a package declares for itself. These mirror
/// `protocol/schemas/capability-package.v1.schema.json`, which is the contract package authors
/// write against; the schema is documentation only, so the numbers have to be enforced here too.
pub const MIN_CAPABILITY_MEMORY_MIB: u64 = 16;
pub const MAX_CAPABILITY_MEMORY_MIB: u64 = 4096;
pub const MAX_CAPABILITY_PROCESSES: u32 = 16;
pub const MAX_CAPABILITY_TIMEOUT_SECONDS: u64 = 120;
pub const MAX_CAPABILITY_DISK_MIB: u64 = 2048;
pub const MAX_CAPABILITY_STDERR_KIB_PER_MINUTE: u64 = 256;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CapabilityValidationError {
    #[error("capability manifest exceeds {MAX_CAPABILITY_MANIFEST_BYTES} bytes")]
    ManifestTooLarge,
    #[error("capability manifest is invalid JSON: {0}")]
    InvalidJson(String),
    #[error("capability manifest exceeds JSON depth {MAX_CAPABILITY_JSON_DEPTH}")]
    JsonTooDeep,
    #[error("unsupported capability schema version {0}")]
    UnsupportedSchema(u32),
    #[error("package kind must be `capability`")]
    InvalidKind,
    #[error("unsafe {field}: {value}")]
    UnsafeId { field: &'static str, value: String },
    #[error("invalid version `{0}`")]
    InvalidVersion(String),
    #[error("unsupported API version `{0}`")]
    UnsupportedApi(String),
    #[error("unsafe package-relative path `{0}`")]
    UnsafePath(String),
    #[error("contribution `{0}` is outside the package namespace")]
    InvalidNamespace(String),
    #[error("duplicate contribution id `{0}`")]
    DuplicateContribution(String),
    #[error("{kind} contribution count exceeds {maximum}")]
    ContributionLimit { kind: &'static str, maximum: usize },
    #[error("invalid permission `{0}`")]
    InvalidPermission(String),
    #[error("command permission `{0}` is not declared by the package")]
    UndeclaredCommandPermission(String),
    #[error("capability package requires a service or Hook UI entrypoint")]
    MissingEntrypoint,
    #[error("invalid signature declaration")]
    InvalidSignature,
    #[error("invalid dependency `{0}`")]
    InvalidDependency(String),
    #[error("invalid setting contribution `{0}`")]
    InvalidSetting(String),
    #[error("resource limit `{field}` is outside the supported range")]
    InvalidResourceLimit { field: &'static str },
}

pub fn parse_capability_manifest(
    bytes: &[u8],
) -> Result<CapabilityPackageManifest, CapabilityValidationError> {
    if bytes.len() > MAX_CAPABILITY_MANIFEST_BYTES {
        return Err(CapabilityValidationError::ManifestTooLarge);
    }
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| CapabilityValidationError::InvalidJson(error.to_string()))?;
    if json_depth(&value) > MAX_CAPABILITY_JSON_DEPTH {
        return Err(CapabilityValidationError::JsonTooDeep);
    }
    let manifest = serde_json::from_value(value)
        .map_err(|error| CapabilityValidationError::InvalidJson(error.to_string()))?;
    validate_capability_manifest(&manifest)?;
    Ok(manifest)
}

pub fn validate_capability_manifest(
    manifest: &CapabilityPackageManifest,
) -> Result<(), CapabilityValidationError> {
    if manifest.schema_version != CAPABILITY_SCHEMA_VERSION {
        return Err(CapabilityValidationError::UnsupportedSchema(
            manifest.schema_version,
        ));
    }
    if manifest.kind != "capability" {
        return Err(CapabilityValidationError::InvalidKind);
    }
    validate_local_id("package id", &manifest.id)?;
    validate_publisher_id(&manifest.publisher.id)?;
    Version::parse(&manifest.version)
        .map_err(|_| CapabilityValidationError::InvalidVersion(manifest.version.clone()))?;
    validate_api_requirement(&manifest.host_compatibility.loom_capability_api)?;
    validate_api_requirement(&manifest.host_compatibility.hook_extension_api)?;
    if let Some(requirement) = &manifest.host_compatibility.surface_api {
        validate_api_requirement(requirement)?;
    }
    validate_entrypoints(manifest)?;
    validate_permissions(&manifest.permissions)?;
    validate_contributions(manifest)?;
    validate_resource_limits(manifest)?;
    validate_dependencies(manifest)?;
    if manifest.signature.algorithm != "ed25519"
        || manifest.signature.key_id != manifest.publisher.key_id
        || validate_relative_path(&manifest.signature.file).is_err()
    {
        return Err(CapabilityValidationError::InvalidSignature);
    }
    Ok(())
}

fn validate_entrypoints(
    manifest: &CapabilityPackageManifest,
) -> Result<(), CapabilityValidationError> {
    if manifest.entrypoints.service.is_none() && manifest.entrypoints.hook_ui.is_none() {
        return Err(CapabilityValidationError::MissingEntrypoint);
    }
    if let Some(service) = &manifest.entrypoints.service {
        if service.targets.is_empty() || service.targets.len() > 16 {
            return Err(CapabilityValidationError::ContributionLimit {
                kind: "platform target",
                maximum: 16,
            });
        }
        for target in service.targets.values() {
            validate_relative_path(&target.command)?;
            if target.args.len() > 64 {
                return Err(CapabilityValidationError::ContributionLimit {
                    kind: "entrypoint argument",
                    maximum: 64,
                });
            }
        }
    }
    if let Some(surface) = &manifest.entrypoints.hook_ui {
        if surface.kind != "surface" {
            return Err(CapabilityValidationError::InvalidKind);
        }
        validate_relative_path(&surface.manifest)?;
    }
    Ok(())
}

fn validate_contributions(
    manifest: &CapabilityPackageManifest,
) -> Result<(), CapabilityValidationError> {
    let contributions = &manifest.contributes;
    check_limit("command", contributions.commands.len(), 128)?;
    for (kind, list, maximum) in [
        ("shortcut", &contributions.shortcuts, 64),
        ("menu", &contributions.menus, 256),
        ("setting", &contributions.settings, 128),
        ("data type", &contributions.data_types, 64),
        ("renderer", &contributions.renderers, 64),
        ("unit overlay", &contributions.unit_overlays, 32),
        ("background task", &contributions.background_tasks, 32),
        ("resource provider", &contributions.resource_providers, 32),
        ("diagnostic", &contributions.diagnostics, 64),
        ("event subscription", &contributions.event_subscriptions, 64),
    ] {
        check_limit(kind, list.len(), maximum)?;
    }

    let namespace = format!("{}/{}.", manifest.publisher.id, manifest.id);
    let mut seen = HashSet::new();
    for command in &contributions.commands {
        validate_contribution_id(&command.id, &namespace, &mut seen)?;
        validate_permissions(&command.permissions)?;
        if let Some(permission) = command
            .permissions
            .iter()
            .find(|permission| !manifest.permissions.contains(permission))
        {
            return Err(CapabilityValidationError::UndeclaredCommandPermission(
                permission.clone(),
            ));
        }
        for path in [&command.input_schema, &command.output_schema]
            .into_iter()
            .flatten()
        {
            validate_relative_path(path)?;
        }
    }
    for contribution in generic_contributions(manifest) {
        validate_contribution_id(&contribution.id, &namespace, &mut seen)?;
        if let Some(command) = &contribution.command {
            validate_contribution_id(command, &namespace, &mut HashSet::new())?;
        }
        if let Some(schema) = &contribution.schema {
            validate_relative_path(schema)?;
        }
    }
    for setting in &contributions.settings {
        validate_setting_definition(setting)?;
    }
    Ok(())
}

fn validate_setting_definition(
    setting: &CapabilityContribution,
) -> Result<(), CapabilityValidationError> {
    let payload = setting
        .payload
        .as_object()
        .ok_or_else(|| CapabilityValidationError::InvalidSetting(setting.id.clone()))?;
    let kind = payload
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| CapabilityValidationError::InvalidSetting(setting.id.clone()))?;
    let default = payload.get("default");
    let valid_default = match kind {
        "string" => default.is_none_or(Value::is_string),
        "number" => default.is_none_or(Value::is_number),
        "boolean" => default.is_none_or(Value::is_boolean),
        "json" => true,
        "enum" => payload
            .get("options")
            .and_then(Value::as_array)
            .is_some_and(|options| {
                !options.is_empty()
                    && options.len() <= 128
                    && options.iter().all(Value::is_string)
                    && default.is_none_or(|value| options.contains(value))
            }),
        _ => false,
    };
    if !valid_default {
        return Err(CapabilityValidationError::InvalidSetting(
            setting.id.clone(),
        ));
    }
    if payload
        .get("description")
        .is_some_and(|value| value.as_str().is_none_or(|text| text.len() > 1024))
    {
        return Err(CapabilityValidationError::InvalidSetting(
            setting.id.clone(),
        ));
    }
    Ok(())
}

fn generic_contributions(
    manifest: &CapabilityPackageManifest,
) -> impl Iterator<Item = &CapabilityContribution> {
    let contributions = &manifest.contributes;
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

/// Bounds the sandbox budgets a package can ask the host to enforce on its own behalf.
///
/// Every field here is turned into a live limit: `timeoutSeconds` becomes the ceiling on a
/// command that declares no `timeoutMs`, `memoryMiB` and `maxProcesses` become job-object
/// limits. Leaving them unbounded lets a package opt out of its own containment - an
/// out-of-range `memoryMiB` overflows the byte conversion and drops the memory cap entirely,
/// and an enormous `timeoutSeconds` pins an invocation slot for as long as the host runs.
fn validate_resource_limits(
    manifest: &CapabilityPackageManifest,
) -> Result<(), CapabilityValidationError> {
    let resources = &manifest.resources;
    let out_of_range = [
        (
            "memoryMiB",
            !(MIN_CAPABILITY_MEMORY_MIB..=MAX_CAPABILITY_MEMORY_MIB)
                .contains(&resources.memory_mib),
        ),
        (
            "maxProcesses",
            !(1..=MAX_CAPABILITY_PROCESSES).contains(&resources.max_processes),
        ),
        (
            "timeoutSeconds",
            !(1..=MAX_CAPABILITY_TIMEOUT_SECONDS).contains(&resources.timeout_seconds),
        ),
        (
            "diskMiB",
            resources
                .disk_mib
                .is_some_and(|disk| !(1..=MAX_CAPABILITY_DISK_MIB).contains(&disk)),
        ),
        (
            "stderrKiBPerMinute",
            resources
                .stderr_kib_per_minute
                .is_some_and(|rate| !(1..=MAX_CAPABILITY_STDERR_KIB_PER_MINUTE).contains(&rate)),
        ),
    ]
    .into_iter()
    .find_map(|(field, invalid)| invalid.then_some(field));
    if let Some(field) = out_of_range {
        return Err(CapabilityValidationError::InvalidResourceLimit { field });
    }
    // A per-command timeout replaces the package-wide one rather than narrowing it, so it needs
    // the same ceiling.
    for command in &manifest.contributes.commands {
        if command
            .timeout_ms
            .is_some_and(|timeout| !(1..=MAX_CAPABILITY_TIMEOUT_SECONDS * 1_000).contains(&timeout))
        {
            return Err(CapabilityValidationError::InvalidResourceLimit {
                field: "command timeoutMs",
            });
        }
    }
    Ok(())
}

fn validate_dependencies(
    manifest: &CapabilityPackageManifest,
) -> Result<(), CapabilityValidationError> {
    if manifest.dependencies.len() > 64 {
        return Err(CapabilityValidationError::ContributionLimit {
            kind: "dependency",
            maximum: 64,
        });
    }
    for dependency in &manifest.dependencies {
        let valid_id = dependency
            .id
            .split_once('/')
            .is_some_and(|(publisher, package)| {
                is_safe_id(publisher, true) && is_safe_id(package, false)
            });
        let valid_version = VersionReq::parse(&dependency.version).is_ok();
        let valid_digest = dependency.sha256.as_ref().is_none_or(|digest| {
            digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        });
        if !valid_id || !valid_version || !valid_digest {
            return Err(CapabilityValidationError::InvalidDependency(
                dependency.id.clone(),
            ));
        }
    }
    Ok(())
}

fn validate_api_requirement(
    requirement: &CapabilityApiRequirement,
) -> Result<(), CapabilityValidationError> {
    for version in std::iter::once(&requirement.minimum).chain(requirement.maximum.iter()) {
        let Some((major, minor)) = version.split_once('.') else {
            return Err(CapabilityValidationError::UnsupportedApi(version.clone()));
        };
        if major != "1" || minor.parse::<u32>().is_err() {
            return Err(CapabilityValidationError::UnsupportedApi(version.clone()));
        }
    }
    if requirement.required_features.len() > 128 || requirement.optional_features.len() > 128 {
        return Err(CapabilityValidationError::ContributionLimit {
            kind: "API feature",
            maximum: 128,
        });
    }
    Ok(())
}

fn validate_permissions(permissions: &[String]) -> Result<(), CapabilityValidationError> {
    if permissions.len() > 64 {
        return Err(CapabilityValidationError::ContributionLimit {
            kind: "permission",
            maximum: 64,
        });
    }
    for permission in permissions {
        if !is_valid_capability_permission(permission) {
            return Err(CapabilityValidationError::InvalidPermission(
                permission.clone(),
            ));
        }
    }
    Ok(())
}

/// Validates a single permission identifier without weakening manifest limits.
#[must_use]
pub fn is_valid_capability_permission(permission: &str) -> bool {
    matches!(
        permission,
        "hook.selection.read"
            | "hook.unit.metadata.read"
            | "hook.unit.image.read"
            | "hook.unit.attachments.read"
            | "hook.unit.attachments.write"
            | "hook.overlay.render"
            | "hook.notice.show"
            | "hook.clipboard.write"
            | "hook.external.open"
            | "loom.network.brokered"
            | "loom.pluginState.readWrite"
    ) || permission
        .strip_prefix("loom.credentials.use:")
        .is_some_and(|name| is_safe_id(name, true))
}

fn validate_contribution_id(
    id: &str,
    namespace: &str,
    seen: &mut HashSet<String>,
) -> Result<(), CapabilityValidationError> {
    let local_id = id.strip_prefix(namespace);
    // The local part may still be dotted — `menu.main` and `result.v1` are ordinary shapes — and
    // that stays safe now that the namespace it hangs off cannot itself be extended with a dot.
    if id.len() > 384 || !local_id.is_some_and(|value| is_safe_id(value, true)) {
        return Err(CapabilityValidationError::InvalidNamespace(id.to_owned()));
    }
    let folded = id.to_ascii_lowercase();
    if !seen.insert(folded) {
        return Err(CapabilityValidationError::DuplicateContribution(
            id.to_owned(),
        ));
    }
    Ok(())
}

fn check_limit(
    kind: &'static str,
    actual: usize,
    maximum: usize,
) -> Result<(), CapabilityValidationError> {
    if actual > maximum {
        return Err(CapabilityValidationError::ContributionLimit { kind, maximum });
    }
    Ok(())
}

fn validate_local_id(field: &'static str, value: &str) -> Result<(), CapabilityValidationError> {
    if is_safe_id(value, false) {
        Ok(())
    } else {
        Err(CapabilityValidationError::UnsafeId {
            field,
            value: value.to_owned(),
        })
    }
}

fn validate_publisher_id(value: &str) -> Result<(), CapabilityValidationError> {
    if is_safe_id(value, true) {
        Ok(())
    } else {
        Err(CapabilityValidationError::UnsafeId {
            field: "publisher id",
            value: value.to_owned(),
        })
    }
}

/// Validates a capability package id: the local half of a `"{publisher}/{package}"` qualified id.
///
/// This is deliberately stricter than the package-wide [`crate::is_safe_package_id`], which allows
/// dots because framework and art ids are dotted by convention. Callers that decide whether a
/// capability package *could* be installed — the signed remote catalog, for one — have to apply
/// the manifest's rule, otherwise they advertise packages that only fail at install time, after
/// the download and signature verification have already been paid for.
#[must_use]
pub fn is_safe_capability_package_id(value: &str) -> bool {
    is_safe_id(value, false)
}

/// Validates a capability publisher id. Dots stay legal so reverse-DNS publishers keep working.
#[must_use]
pub fn is_safe_capability_publisher_id(value: &str) -> bool {
    is_safe_id(value, true)
}

/// Validates one identifier segment, optionally allowing `.` as an inner separator.
///
/// A package id must not contain `.`, because everything the package owns is namespaced by
/// `"{publisher}/{package}."` and matched with a prefix test. With dots allowed there, namespaces
/// nest: package `a` claims the prefix `pub/a.`, and every id belonging to package `a.b` —
/// `pub/a.b.thing` — also starts with it. That let one package mint contribution ids, attachment
/// ids, data type ids and resource ids inside another package's namespace. Publisher ids keep
/// their dots: the `/` separator is not part of any id alphabet, so reverse-DNS publishers cannot
/// nest the same way.
///
/// Both halves of a qualified id become directory components under the packages root, so they also
/// carry the reserved-device-name rule the package-wide validator applies. Windows resolves those
/// names to devices whatever the extension, which turns an install into an unexplained I/O error
/// instead of a validation failure.
fn is_safe_id(value: &str, allow_dots: bool) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !crate::is_windows_reserved_device_name(value)
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_')
                || (allow_dots && byte == b'.')
        })
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        && !value.contains("..")
        && !value
            .as_bytes()
            .last()
            .is_some_and(|byte| matches!(byte, b'.' | b'-' | b'_'))
}

fn validate_relative_path(path: &str) -> Result<(), CapabilityValidationError> {
    let safe = !path.is_empty()
        && path.len() <= 1024
        && !path.contains('\\')
        && !path.contains(':')
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if safe {
        Ok(())
    } else {
        Err(CapabilityValidationError::UnsafePath(path.to_owned()))
    }
}

fn json_depth(value: &Value) -> usize {
    match value {
        Value::Array(values) => 1 + values.iter().map(json_depth).max().unwrap_or(0),
        Value::Object(values) => 1 + values.values().map(json_depth).max().unwrap_or(0),
        _ => 1,
    }
}
