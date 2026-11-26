//! Conversion from a validated manifest to runtime server configuration.

use super::*;

pub(super) fn host_platform_candidates() -> Vec<String> {
    let os = match std::env::consts::OS {
        "windows" => "windows",
        "linux" => "linux",
        "macos" => "macos",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    };
    vec![
        format!("{os}-{arch}"),
        os.to_owned(),
        "universal".to_owned(),
    ]
}

pub(super) fn resolve_entry_for_platform<'a>(
    entry: &'a McpPackageEntry,
    candidates: &[String],
) -> (&'a str, &'a [String]) {
    for key in candidates {
        if let Some(platform_entry) = entry.platforms.get(key.as_str()) {
            return (&platform_entry.command, &platform_entry.args);
        }
    }
    (&entry.command, &entry.args)
}

pub(super) fn config_from_manifest(
    manifest: McpServerPackageManifest,
    digest: String,
    target_dir: PathBuf,
    files: BTreeMap<String, String>,
    trust_status: PackageTrustStatus,
) -> Result<McpServerConfig, McpPackageError> {
    let qualified_id = manifest.qualified_id();
    let candidates = host_platform_candidates();
    let (selected_command, selected_args) =
        resolve_entry_for_platform(&manifest.entry, &candidates);
    let selected_args = selected_args.to_vec();
    let mut config = match manifest.transport {
        McpTransport::Stdio => McpServerConfig::new(
            manifest.id.clone(),
            manifest.name.clone(),
            target_dir.join(selected_command).display().to_string(),
        ),
        McpTransport::StreamableHttp => McpServerConfig::remote(
            manifest.id.clone(),
            manifest.name.clone(),
            manifest.entry.url.clone(),
        ),
    };
    config.description = manifest.description;
    config.args = selected_args;
    config.tools = manifest.tools;
    for credential in manifest.credentials {
        match credential.target.kind {
            McpPackageCredentialTargetKind::Env => {
                config
                    .credential_env
                    .insert(credential.target.name, credential.id.clone());
            }
            McpPackageCredentialTargetKind::Header => {
                config
                    .credential_headers
                    .insert(credential.target.name, credential.id.clone());
            }
        }
        config
            .credential_requirements
            .push(McpCredentialRequirement {
                id: credential.id,
                label: credential.label,
                required: credential.required,
            });
    }
    config.package = Some(McpServerPackageState {
        qualified_id,
        publisher_id: manifest.publisher.id,
        version: manifest.version,
        digest,
        package_dir: target_dir,
        files,
        trust_status,
    });
    config
        .validate()
        .map_err(|error| McpPackageError::InvalidManifest(error.to_string()))?;
    Ok(config)
}

#[cfg(test)]
mod config_platform_tests {
    use super::*;

    #[test]
    fn platform_entry_selects_linux_override_while_preserving_windows_default() {
        let mut platforms = BTreeMap::new();
        platforms.insert(
            "linux".to_owned(),
            McpPackagePlatformEntry {
                command: "runtime/server.mjs".to_owned(),
                args: vec!["--stdio".to_owned()],
            },
        );
        let entry = McpPackageEntry {
            command: "runtime/server.ps1".to_owned(),
            args: Vec::new(),
            url: String::new(),
            platforms,
        };
        let (linux_cmd, linux_args) = resolve_entry_for_platform(
            &entry,
            &[
                "linux-x64".to_owned(),
                "linux".to_owned(),
                "universal".to_owned(),
            ],
        );
        assert_eq!(linux_cmd, "runtime/server.mjs");
        assert_eq!(linux_args, &["--stdio".to_owned()]);

        let (win_cmd, win_args) = resolve_entry_for_platform(
            &entry,
            &[
                "windows-x64".to_owned(),
                "windows".to_owned(),
                "universal".to_owned(),
            ],
        );
        assert_eq!(win_cmd, "runtime/server.ps1");
        assert!(win_args.is_empty());
    }
}
