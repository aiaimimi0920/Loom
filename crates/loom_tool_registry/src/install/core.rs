use super::*;

/// Install an art package into an immutable publisher-scoped version directory.
pub fn install_art_from_zip(
    zip_bytes: &[u8],
    control_plane_root: &Path,
    framework_registry: &FrameworkRegistry,
    tool_registry: &ToolRegistry,
) -> Result<ArtInstallReport, ArtInstallError> {
    install_art_from_zip_with_source(
        zip_bytes,
        control_plane_root,
        framework_registry,
        tool_registry,
        ArtInstallSource::ExternalPackage,
    )
}

pub fn install_authored_art_from_zip(
    zip_bytes: &[u8],
    control_plane_root: &Path,
    framework_registry: &FrameworkRegistry,
    tool_registry: &ToolRegistry,
) -> Result<ArtInstallReport, ArtInstallError> {
    install_art_from_zip_with_source(
        zip_bytes,
        control_plane_root,
        framework_registry,
        tool_registry,
        ArtInstallSource::LocalAuthoring,
    )
}

pub fn install_bundled_art_from_zip(
    zip_bytes: &[u8],
    control_plane_root: &Path,
    framework_registry: &FrameworkRegistry,
    tool_registry: &ToolRegistry,
) -> Result<ArtInstallReport, ArtInstallError> {
    install_art_from_zip_with_source(
        zip_bytes,
        control_plane_root,
        framework_registry,
        tool_registry,
        ArtInstallSource::BundledCatalog,
    )
}

pub(super) fn install_art_from_zip_with_source(
    zip_bytes: &[u8],
    control_plane_root: &Path,
    framework_registry: &FrameworkRegistry,
    tool_registry: &ToolRegistry,
    source: ArtInstallSource,
) -> Result<ArtInstallReport, ArtInstallError> {
    let mut tool = read_manifest_from_zip(zip_bytes)?;
    if !is_safe_art_id(&tool.id) {
        return Err(ArtInstallError::InvalidArtId(tool.id.clone()));
    }
    tool.validate()
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
    let qualified_id = qualified_art_id(&tool)?;

    // Framework must be installed + ready before we lay down files.
    let deps = read_dependencies(&tool);
    let framework = deps.framework.clone().unwrap_or_else(|| {
        crate::framework::framework_id_for_execution(&tool.execution).to_owned()
    });
    if !framework_registry.is_installed(&framework) {
        return Err(ArtInstallError::FrameworkNotReady {
            art_id: tool.id.clone(),
            framework,
            reason: "installed".to_owned(),
        });
    }
    let (ready, _) = framework_registry.readiness(&framework);
    if !ready {
        return Err(ArtInstallError::FrameworkNotReady {
            art_id: tool.id.clone(),
            framework,
            reason: "ready".to_owned(),
        });
    }
    if let Some(requirement) = deps.framework_version.as_deref() {
        let requirement = semver::VersionReq::parse(requirement).map_err(|error| {
            ArtInstallError::InvalidPackage(format!(
                "invalid frameworkVersion requirement `{requirement}`: {error}"
            ))
        })?;
        let installed_version = framework_registry
            .statuses()
            .into_iter()
            .find(|status| status.qualified_id == framework || status.id == framework)
            .and_then(|status| status.version)
            .ok_or_else(|| ArtInstallError::FrameworkNotReady {
                art_id: tool.id.clone(),
                framework: framework.clone(),
                reason: "versioned".to_owned(),
            })?;
        let installed_version = semver::Version::parse(&installed_version).map_err(|error| {
            ArtInstallError::InvalidPackage(format!(
                "installed framework version `{installed_version}` is invalid: {error}"
            ))
        })?;
        if !requirement.matches(&installed_version) {
            return Err(ArtInstallError::FrameworkNotReady {
                art_id: tool.id.clone(),
                framework: framework.clone(),
                reason: format!(
                    "compatible: requires {requirement}, installed {installed_version}"
                ),
            });
        }
    }

    validate_mcp_execution_dependency(&tool, &deps.mcp_servers)?;

    let arts_root = control_plane_root.join("arts");
    std::fs::create_dir_all(&arts_root)?;
    let art_root = art_root_for_tool(control_plane_root, &tool)?;
    let mut locked_dependencies = resolve_art_dependency_locks(
        control_plane_root,
        &deps.arts,
        framework_registry,
        tool_registry,
    )?;
    locked_dependencies.extend(resolve_mcp_dependency_locks(
        control_plane_root,
        &deps.mcp_servers,
    )?);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let staging = control_plane_root.join(format!(".loom-art-{}-{nonce}", tool.id));
    let result = (|| {
        if staging.exists() {
            std::fs::remove_dir_all(&staging)?;
        }
        std::fs::create_dir_all(&staging)?;
        let installed_files = crate::secure_zip::extract_zip_securely(zip_bytes, &staging)
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;

        let security = read_art_package_security(&tool);
        let trust_store = TrustStore::load(&control_plane_root.join("plugin-trust.json"))
            .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        let trust_status = verify_package_signature(
            &staging,
            security.publisher.as_ref(),
            security.signature.as_ref(),
            &trust_store,
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        if source == ArtInstallSource::ExternalPackage {
            trust_store
                .effective_policy()
                .enforce(trust_status.clone())
                .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        }

        // Resolve declared third-party binaries before activation. Bundled
        // files are verified in staging; downloads cannot alter the active Art.
        let binaries = resolve_binaries(&deps.binaries, &staging, &installed_files)?;
        let digest = canonical_package_digest(
            &staging,
            security
                .signature
                .as_ref()
                .map(|signature| signature.file.as_str()),
        )
        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
        let version = required_art_package_version(&security)?;
        let version_dir = format!("{}-{}", sanitize_version_for_path(version), &digest[..12]);
        let mut active_relative = Path::new("versions").join(&version_dir);
        let mut art_dir = art_root.join(&active_relative);
        std::fs::create_dir_all(art_dir.parent().expect("Art version parent"))?;
        // A version directory may predate the Windows private-ACL repair. On
        // Windows `symlink_metadata` can still succeed for such a directory
        // even though reading its manifest (and therefore reusing it) is
        // denied. Treat an unreadable immutable target as a collision and
        // install the verified package under a recovered version name instead
        // of failing later while writing its lock/activation state. The old
        // directory is intentionally left untouched.
        let target_exists = match std::fs::symlink_metadata(&art_dir) {
            Ok(metadata) => {
                if metadata_has_link_semantics(&metadata) || !metadata.is_dir() {
                    return Err(ArtInstallError::InvalidPackage(
                        "existing Art version target is not a plain directory".to_owned(),
                    ));
                }
                match std::fs::read(art_dir.join(MANIFEST_NAME)) {
                    Ok(_) => {
                        let existing_digest = canonical_package_digest(
                            &art_dir,
                            security
                                .signature
                                .as_ref()
                                .map(|signature| signature.file.as_str()),
                        )
                        .map_err(|error| ArtInstallError::InvalidPackage(error.to_string()))?;
                        if existing_digest != digest {
                            return Err(ArtInstallError::InvalidPackage(
                                "existing immutable Art version content does not match its digest"
                                    .to_owned(),
                            ));
                        }
                        true
                    }
                    Err(_) => {
                        let nonce = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos();
                        active_relative =
                            Path::new("versions").join(format!("{version_dir}-recovered-{nonce}"));
                        art_dir = art_root.join(&active_relative);
                        false
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                let nonce = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                active_relative =
                    Path::new("versions").join(format!("{version_dir}-recovered-{nonce}"));
                art_dir = art_root.join(&active_relative);
                false
            }
            Err(error) => return Err(ArtInstallError::Io(error)),
        };
        let target_created = if target_exists {
            std::fs::remove_dir_all(&staging)?;
            false
        } else {
            std::fs::rename(&staging, &art_dir)?;
            true
        };

        let state_dir = art_root.join("state");
        let cache_dir = art_root.join("cache");
        let output_dir = art_root.join("outputs");
        let locks_dir = art_root.join("locks");
        for directory in [&state_dir, &cache_dir, &output_dir, &locks_dir] {
            std::fs::create_dir_all(directory)?;
        }
        let lockfile = locks_dir.join(format!("{digest}.json"));
        write_art_lockfile(
            &lockfile,
            &qualified_id,
            version,
            &framework,
            framework_registry,
            &deps.binaries,
            &art_dir,
            &locked_dependencies,
        )?;
        set_tree_readonly(&art_dir, true)?;

        let active_path = art_root.join("active.json");
        let old_activation = read_art_activation(&active_path);
        let active_text = active_relative.to_string_lossy().replace('\\', "/");
        let active = ArtVersionPointer {
            path: active_text,
            version: version.to_owned(),
            digest: digest.clone(),
            lockfile: lockfile.to_string_lossy().to_string(),
        };
        let previous = old_activation
            .as_ref()
            .and_then(|activation| {
                (activation.active.path != active.path).then(|| activation.active.clone())
            })
            .or_else(|| {
                old_activation
                    .as_ref()
                    .and_then(|activation| activation.previous.clone())
            });
        let activation = ArtActivationState {
            active,
            previous,
            local_authoring: source == ArtInstallSource::LocalAuthoring,
            bundled_catalog: source == ArtInstallSource::BundledCatalog,
        };
        write_art_lifecycle(
            &art_root,
            &ArtLifecycleJournal {
                old_activation: old_activation.clone(),
                next_activation: activation.clone(),
                target: active_relative.to_string_lossy().replace('\\', "/"),
                created_target: target_created,
            },
        )?;
        if let Err(error) = write_art_activation(&active_path, &activation) {
            clear_art_lifecycle(&art_root);
            if target_created {
                let _ = remove_tree(&art_dir);
            }
            return Err(error);
        }

        record_art_package_directory(
            &mut tool.metadata,
            ArtPackagePaths {
                qualified_id: &qualified_id,
                art_dir: &art_dir,
                state_dir: &state_dir,
                cache_dir: &cache_dir,
                output_dir: &output_dir,
                lockfile: &lockfile,
                version,
                digest: &digest,
                trust_status: &trust_status,
            },
        );
        let tool_id = tool.id.clone();
        if let Err(error) = tool_registry.save_packaged_tool(tool) {
            if let Some(old_activation) = old_activation {
                let _ = write_art_activation(&active_path, &old_activation);
            } else {
                let _ = std::fs::remove_file(&active_path);
            }
            if target_created {
                let _ = remove_tree(&art_dir);
            }
            clear_art_lifecycle(&art_root);
            return Err(ArtInstallError::Registry(error.to_string()));
        }
        let _ = prune_art_versions(&art_root, &activation);
        clear_art_lifecycle(&art_root);

        Ok(ArtInstallReport {
            tool_id,
            framework,
            art_dir,
            installed_files,
            binaries,
            dependent_arts: deps.arts,
            trust_status,
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}
