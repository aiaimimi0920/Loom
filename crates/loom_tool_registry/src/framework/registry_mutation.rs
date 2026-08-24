//! Framework lifecycle mutations: install, upgrade, rollback, enable, and uninstall.
use super::*;

impl FrameworkRegistry {
    /// Install a framework package from the configured store. The package
    /// manifest and process entry must be present before the state is saved.
    pub fn install(&self, id: &str) -> Result<FrameworkStatus, FrameworkError> {
        self.install_with_runtime_fetcher(id, &default_runtime_fetcher)
    }

    /// Install variant with an injectable package fetcher (a closure returning
    /// the framework package zip bytes for a framework id). Testable without
    /// the network.
    pub fn install_with_runtime_fetcher<F>(
        &self,
        id: &str,
        fetch_runtime: &F,
    ) -> Result<FrameworkStatus, FrameworkError>
    where
        F: Fn(&str) -> Result<Vec<u8>, FrameworkError>,
    {
        if !is_valid_framework(id) {
            return Err(FrameworkError::UnknownFramework(id.to_owned()));
        }
        let package = fetch_runtime(id)?;
        self.install_framework_package_zip(&package, Some(id))
    }

    /// Install a framework package supplied as a ZIP. The ZIP must contain a
    /// root `framework.manifest.json` and the manifest's process entry.
    pub fn install_framework_package_from_zip(
        &self,
        zip_bytes: &[u8],
    ) -> Result<FrameworkStatus, FrameworkError> {
        self.install_framework_package_zip(zip_bytes, None)
    }

    /// Upgrade a package by replacing its installed directory with a fully
    /// validated new ZIP. Installation and upgrade share the same atomic path
    /// so a bad package cannot leave a half-written runtime behind.
    pub fn upgrade_framework_package_from_zip(
        &self,
        zip_bytes: &[u8],
    ) -> Result<FrameworkStatus, FrameworkError> {
        self.install_framework_package_zip(zip_bytes, None)
    }

    /// Upgrade a specific installed framework package and reject a ZIP whose
    /// manifest belongs to another framework.
    pub fn upgrade_framework_package(
        &self,
        id: &str,
        zip_bytes: &[u8],
    ) -> Result<FrameworkStatus, FrameworkError> {
        if !is_valid_framework_reference(id) {
            return Err(FrameworkError::UnknownFramework(id.to_owned()));
        }
        let key = self
            .resolve_state_key(id)?
            .ok_or_else(|| FrameworkError::FrameworkNotInstalled(id.to_owned()))?;
        self.install_framework_package_zip(zip_bytes, Some(&key))
    }

    /// Enable an installed framework package.
    pub fn enable(&self, id: &str) -> Result<FrameworkStatus, FrameworkError> {
        self.set_enabled(id, true)
    }

    /// Disable an installed framework package.
    pub fn disable(&self, id: &str) -> Result<FrameworkStatus, FrameworkError> {
        self.set_enabled(id, false)
    }

    pub(super) fn set_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<FrameworkStatus, FrameworkError> {
        if !is_valid_framework_reference(id) {
            return Err(FrameworkError::UnknownFramework(id.to_owned()));
        }
        let key = self
            .resolve_state_key(id)?
            .ok_or_else(|| FrameworkError::FrameworkNotInstalled(id.to_owned()))?;
        let mut installed = self.installation_states()?;
        let state = installed
            .get_mut(&key)
            .ok_or_else(|| FrameworkError::FrameworkNotInstalled(id.to_owned()))?;
        state.enabled = enabled;
        if let Some(manifest) = self.package_manifest(&key) {
            state.version = manifest.version;
        }
        self.write_installed(&installed)?;
        Ok(self.status_of(&key))
    }

    pub(super) fn install_framework_package_zip(
        &self,
        zip_bytes: &[u8],
        expected_id: Option<&str>,
    ) -> Result<FrameworkStatus, FrameworkError> {
        let staging = self.staging_dir(framework_local_id(expected_id.unwrap_or("package")));
        let mut staging_owned = false;
        let result = (|| {
            unpack_runtime_zip(expected_id.unwrap_or("package"), zip_bytes, &staging)?;
            staging_owned = true;
            let manifest = read_framework_manifest(&staging.join(FRAMEWORK_MANIFEST_FILE))
                .map_err(|reason| FrameworkError::InvalidPackage {
                    id: expected_id.unwrap_or("package").to_owned(),
                    reason,
                })?;
            if !is_valid_framework(&manifest.id) {
                return Err(FrameworkError::UnknownFramework(manifest.id));
            }
            if let Some(expected_id) = expected_id {
                if manifest.id != expected_id && manifest.qualified_id() != expected_id {
                    return Err(FrameworkError::InvalidPackage {
                        id: expected_id.to_owned(),
                        reason: format!("manifest id is {}", manifest.id),
                    });
                }
            }
            validate_framework_manifest(&manifest, &staging)?;
            enforce_framework_permission_policy(&manifest).map_err(|reason| {
                FrameworkError::InvalidPackage {
                    id: manifest.qualified_id(),
                    reason,
                }
            })?;
            let resolved_dependencies =
                resolve_framework_dependencies(&self.root, &manifest, &staging)?;
            let trust_store = self.trust_store()?;
            let trust_status = verify_package_signature(
                &staging,
                Some(&manifest.publisher),
                manifest.signature.as_ref(),
                &trust_store,
            )?;
            trust_store.effective_policy().enforce(trust_status)?;
            run_framework_self_test(&manifest, &staging)?;

            let storage_key = manifest.qualified_id();
            let packages_root = self.root.join(FRAMEWORK_PACKAGES_DIR);
            fs::create_dir_all(&packages_root)?;
            // Read the persisted state before any package file moves into place: a corrupt state
            // file has to abort the install rather than leave a package on disk that no state
            // entry describes.
            let mut installed = self.installation_states()?;
            let package_root = self.package_root(&storage_key);
            let versions_root = package_root.join(FRAMEWORK_VERSIONS_DIR);
            fs::create_dir_all(&versions_root)?;
            let digest = canonical_package_digest(
                &staging,
                manifest
                    .signature
                    .as_ref()
                    .map(|signature| signature.file.as_str()),
            )?;
            let version_dir = format!(
                "{}-{}",
                sanitize_version_for_path(&manifest.version),
                &digest[..12]
            );
            let mut active_relative = Path::new(FRAMEWORK_VERSIONS_DIR).join(&version_dir);
            let mut target = package_root.join(&active_relative);
            let target_exists = match fs::symlink_metadata(&target) {
                Ok(metadata) => {
                    if metadata_has_link_semantics(&metadata) || !metadata.is_dir() {
                        return Err(FrameworkError::InvalidPackage {
                            id: storage_key.clone(),
                            reason: "existing framework version target is not a plain directory"
                                .to_owned(),
                        });
                    }
                    match read_bounded_file(
                        &target.join(FRAMEWORK_MANIFEST_FILE),
                        FRAMEWORK_METADATA_MAX_BYTES,
                    ) {
                        Ok(_) => {
                            let existing_digest = canonical_package_digest(
                                &target,
                                manifest
                                    .signature
                                    .as_ref()
                                    .map(|signature| signature.file.as_str()),
                            )?;
                            if existing_digest != digest {
                                return Err(FrameworkError::InvalidPackage {
                                    id: storage_key.clone(),
                                    reason: "existing immutable framework version content does not match its digest"
                                        .to_owned(),
                                });
                            }
                            true
                        }
                        Err(_) => {
                            // Preserve an unreadable legacy version and activate the verified
                            // package under a collision-free recovery path.
                            let nonce = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos();
                            active_relative = Path::new(FRAMEWORK_VERSIONS_DIR)
                                .join(format!("{version_dir}-recovered-{nonce}"));
                            target = package_root.join(&active_relative);
                            false
                        }
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                    // A legacy package directory can retain an ACL owned by a
                    // previous Windows token. Keep it immutable and activate
                    // the freshly verified package under a new path.
                    let nonce = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos();
                    active_relative = Path::new(FRAMEWORK_VERSIONS_DIR)
                        .join(format!("{version_dir}-recovered-{nonce}"));
                    target = package_root.join(&active_relative);
                    false
                }
                Err(error) => return Err(FrameworkError::Io(error)),
            };
            let active_relative_text = active_relative.to_string_lossy().replace('\\', "/");
            let target_created = if target_exists {
                remove_framework_tree(&staging)?;
                false
            } else {
                move_framework_tree_with_retry(&staging, &target)?;
                true
            };
            set_framework_tree_readonly(&target, true)?;
            register_framework_runtimes(&self.root, &manifest, &target)?;
            if let Err(error) = write_framework_lockfile(
                &package_root,
                &storage_key,
                &manifest.version,
                &digest,
                resolved_dependencies,
            ) {
                if target_created {
                    let _ = remove_framework_tree(&target);
                }
                return Err(error);
            }

            let old_activation = self.activation(&storage_key);
            let previous = old_activation
                .as_ref()
                .and_then(|activation| {
                    (activation.active != active_relative_text).then(|| activation.active.clone())
                })
                .or_else(|| {
                    old_activation
                        .as_ref()
                        .and_then(|activation| activation.previous.clone())
                });
            let activation = FrameworkActivationState {
                active: active_relative_text,
                previous,
            };
            self.write_lifecycle_journal(
                &storage_key,
                &FrameworkLifecycleJournal {
                    old_activation: old_activation.clone(),
                    next_activation: activation.clone(),
                    target: active_relative.to_string_lossy().replace('\\', "/"),
                    created_target: target_created,
                },
            )?;
            if let Err(error) = self.write_activation(&storage_key, &activation) {
                if target_created {
                    let _ = remove_framework_tree(&target);
                }
                self.clear_lifecycle_journal(&storage_key);
                return Err(error);
            }

            installed.insert(
                storage_key.clone(),
                FrameworkInstallationState {
                    version: manifest.version.clone(),
                    enabled: true,
                },
            );
            if let Err(error) = self.write_installed(&installed) {
                if let Some(old_activation) = old_activation {
                    let _ = self.write_activation(&storage_key, &old_activation);
                } else {
                    let _ = fs::remove_file(self.activation_path(&storage_key));
                }
                if target_created {
                    let _ = remove_framework_tree(&target);
                }
                self.clear_lifecycle_journal(&storage_key);
                return Err(error);
            }
            prune_framework_versions(&package_root, &activation)?;
            let _ = crate::dependency::RuntimeRegistry::new(&self.root).prune_stale();
            self.clear_lifecycle_journal(&storage_key);
            Ok(self.status_of(&storage_key))
        })();
        if result.is_err() && staging_owned {
            let _ = remove_framework_tree(&staging);
        }
        result
    }

    pub(super) fn staging_dir(&self, id: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        self.root.join(format!(".loom-framework-{id}-{nonce}"))
    }

    pub fn rollback(&self, id: &str) -> Result<FrameworkStatus, FrameworkError> {
        let key = self
            .resolve_state_key(id)?
            .ok_or_else(|| FrameworkError::FrameworkNotInstalled(id.to_owned()))?;
        let activation = self
            .activation(&key)
            .ok_or_else(|| FrameworkError::NoRollback { id: id.to_owned() })?;
        if !framework_activation_is_safe(&activation) {
            return Err(FrameworkError::InvalidPackage {
                id: key.clone(),
                reason: "activation state contains an unsafe version path".to_owned(),
            });
        }
        let previous = activation
            .previous
            .clone()
            .ok_or_else(|| FrameworkError::NoRollback { id: id.to_owned() })?;
        let next = FrameworkActivationState {
            active: previous,
            previous: Some(activation.active.clone()),
        };
        let target = self.package_root(&key).join(&next.active);
        if !is_directory_without_links(&target)?
            || !is_file_without_links(&target.join(FRAMEWORK_MANIFEST_FILE))?
        {
            return Err(FrameworkError::NoRollback { id: id.to_owned() });
        }
        let manifest =
            read_framework_manifest(&target.join(FRAMEWORK_MANIFEST_FILE)).map_err(|reason| {
                FrameworkError::InvalidPackage {
                    id: key.clone(),
                    reason,
                }
            })?;
        if manifest.qualified_id() != key && manifest.id != key {
            return Err(FrameworkError::InvalidPackage {
                id: key.clone(),
                reason: "rollback package identity does not match the installed publisher"
                    .to_owned(),
            });
        }
        let trust_store = self.trust_store()?;
        let trust_status = verify_package_signature(
            &target,
            Some(&manifest.publisher),
            manifest.signature.as_ref(),
            &trust_store,
        )?;
        trust_store.effective_policy().enforce(trust_status)?;
        let digest = canonical_package_digest(
            &target,
            manifest
                .signature
                .as_ref()
                .map(|signature| signature.file.as_str()),
        )?;
        if !next.active.ends_with(&digest[..12]) {
            return Err(FrameworkError::InvalidPackage {
                id: key.clone(),
                reason: "rollback package digest does not match its immutable version path"
                    .to_owned(),
            });
        }
        enforce_framework_permission_policy(&manifest).map_err(|reason| {
            FrameworkError::InvalidPackage {
                id: key.clone(),
                reason,
            }
        })?;
        run_framework_self_test(&manifest, &target)?;
        self.write_lifecycle_journal(
            &key,
            &FrameworkLifecycleJournal {
                old_activation: Some(activation.clone()),
                next_activation: next.clone(),
                target: next.active.clone(),
                // A rollback activates a version that is already on disk; recovery must never
                // delete it.
                created_target: false,
            },
        )?;
        if let Err(error) = self.write_activation(&key, &next) {
            // The journal describes an activation that was never written. Leaving it behind would
            // make the next startup restore `old_activation` over an activation that already holds
            // exactly that value, so drop it instead.
            self.clear_lifecycle_journal(&key);
            return Err(error);
        }
        // A corrupt state file gets the same treatment as a failed state write: the activation
        // that was just written has to go back, or the package would run at the rolled-back
        // version while the state file still claims the newer one.
        let mut installed = match self.installation_states() {
            Ok(installed) => installed,
            Err(error) => {
                let _ = self.write_activation(&key, &activation);
                self.clear_lifecycle_journal(&key);
                return Err(error);
            }
        };
        if let Some(state) = installed.get_mut(&key) {
            state.version = manifest.version;
        }
        if let Err(error) = self.write_installed(&installed) {
            let _ = self.write_activation(&key, &activation);
            self.clear_lifecycle_journal(&key);
            return Err(error);
        }
        self.clear_lifecycle_journal(&key);
        Ok(self.status_of(&key))
    }

    /// Mark a framework uninstalled and remove any downloaded runtime. Errors on
    /// an unknown id.
    pub fn uninstall(&self, id: &str) -> Result<FrameworkStatus, FrameworkError> {
        if !is_valid_framework_reference(id) {
            return Err(FrameworkError::UnknownFramework(id.to_owned()));
        }
        let key = self
            .resolve_state_key(id)?
            .ok_or_else(|| FrameworkError::FrameworkNotInstalled(id.to_owned()))?;
        let package_root = self.package_root(&key);
        let tombstone = if package_root.exists() {
            let tombstone =
                uninstall_tombstone_path(&package_root, FRAMEWORK_UNINSTALL_TOMBSTONE_PREFIX)?;
            fs::rename(&package_root, &tombstone)?;
            Some(tombstone)
        } else {
            None
        };
        // `resolve_state_key` above already refused a corrupt state file, so this read only fails
        // if the file was damaged while the uninstall was in flight. Put the package back either
        // way rather than leaving it in a tombstone that no state entry mentions.
        let mut installed = match self.installation_states() {
            Ok(installed) => installed,
            Err(error) => {
                if let Some(tombstone) = &tombstone {
                    let _ = fs::rename(tombstone, &package_root);
                }
                return Err(error);
            }
        };
        installed.remove(&key);
        if let Err(error) = self.write_installed(&installed) {
            if let Some(tombstone) = &tombstone {
                let _ = fs::rename(tombstone, &package_root);
            }
            return Err(error);
        }
        if let Some(tombstone) = tombstone {
            remove_framework_tree(&tombstone)?;
        }
        let _ = crate::dependency::RuntimeRegistry::new(&self.root).prune_stale();
        Ok(self.status_of(framework_local_id(&key)))
    }
}
