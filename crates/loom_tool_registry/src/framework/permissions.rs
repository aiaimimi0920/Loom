//! Permission declarations, enforcement modes, and audit findings.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginPermissionMode {
    #[default]
    Audit,
    Strict,
}

pub fn plugin_permission_mode() -> Result<PluginPermissionMode, String> {
    parse_plugin_permission_mode(std::env::var("LOOM_PLUGIN_PERMISSION_MODE").ok().as_deref())
}

pub(super) fn parse_plugin_permission_mode(
    value: Option<&str>,
) -> Result<PluginPermissionMode, String> {
    match value
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "" | "audit" => Ok(PluginPermissionMode::Audit),
        "strict" => Ok(PluginPermissionMode::Strict),
        value => Err(format!(
            "invalid LOOM_PLUGIN_PERMISSION_MODE `{value}`; expected audit or strict"
        )),
    }
}

pub fn permission_enforcement_matrix() -> BTreeMap<&'static str, &'static str> {
    let memory_and_process_count = if cfg!(windows) {
        "windows-job-enforced"
    } else {
        "declared-only"
    };
    BTreeMap::from([
        ("packageContainment", "enforced"),
        ("writableStateSeparation", "enforced"),
        ("processTree", "enforced"),
        ("timeoutAndOutput", "enforced"),
        ("memoryAndProcessCount", memory_and_process_count),
        ("credentials", "brokered"),
        ("hostHttp", "policy-enforced"),
        ("directNetwork", "not-os-enforced"),
        ("arbitraryFilesystem", "not-os-enforced"),
        ("gpu", "not-os-enforced"),
        ("clipboard", "not-os-enforced"),
    ])
}

pub fn unsupported_permission_findings(manifest: &FrameworkPackageManifest) -> Vec<String> {
    unsupported_permission_findings_for(&manifest.permissions, &manifest.permission_policy)
}

pub fn unsupported_permission_findings_for(
    permissions: &[String],
    permission_policy: &PermissionPolicy,
) -> Vec<String> {
    let declared = permissions
        .iter()
        .map(|permission| permission.trim().to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut findings = Vec::new();
    if declared
        .iter()
        .any(|permission| permission.starts_with("network."))
        || !permission_policy.network.domains.is_empty()
        || permission_policy.network.allow_localhost
        || permission_policy.network.allow_private_networks
    {
        findings.push("direct_network".to_owned());
    }
    if declared
        .iter()
        .any(|permission| permission.starts_with("file.") || permission.starts_with("filesystem."))
        || !permission_policy.filesystem.read.is_empty()
        || !permission_policy.filesystem.write.is_empty()
    {
        findings.push("arbitrary_filesystem".to_owned());
    }
    if permission_policy.gpu || declared.iter().any(|permission| permission == "gpu") {
        findings.push("gpu".to_owned());
    }
    if permission_policy.clipboard
        || declared
            .iter()
            .any(|permission| permission.starts_with("clipboard"))
    {
        findings.push("clipboard".to_owned());
    }
    findings
}

pub fn enforce_framework_permission_policy(
    manifest: &FrameworkPackageManifest,
) -> Result<(), String> {
    let mode = plugin_permission_mode()?;
    enforce_framework_permission_mode(manifest, mode)
}

pub(super) fn enforce_framework_permission_mode(
    manifest: &FrameworkPackageManifest,
    mode: PluginPermissionMode,
) -> Result<(), String> {
    let findings = unsupported_permission_findings(manifest);
    if mode == PluginPermissionMode::Strict && !findings.is_empty() {
        return Err(format!(
            "strict plugin permission mode cannot OS-enforce: {}",
            findings.join(", ")
        ));
    }
    Ok(())
}
