//! Conversion from a validated manifest to runtime server configuration.

use super::*;

pub(super) fn config_from_manifest(
    manifest: McpServerPackageManifest,
    digest: String,
    target_dir: PathBuf,
    files: BTreeMap<String, String>,
    trust_status: PackageTrustStatus,
) -> Result<McpServerConfig, McpPackageError> {
    let qualified_id = manifest.qualified_id();
    let mut config = match manifest.transport {
        McpTransport::Stdio => McpServerConfig::new(
            manifest.id.clone(),
            manifest.name.clone(),
            target_dir
                .join(&manifest.entry.command)
                .display()
                .to_string(),
        ),
        McpTransport::StreamableHttp => McpServerConfig::remote(
            manifest.id.clone(),
            manifest.name.clone(),
            manifest.entry.url.clone(),
        ),
    };
    config.description = manifest.description;
    config.args = manifest.entry.args;
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
