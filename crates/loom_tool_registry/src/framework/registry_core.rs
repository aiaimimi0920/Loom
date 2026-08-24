//! Framework registry construction, queries, recovery, and state persistence.
use super::*;

/// Tracks which framework packages the user has installed, persisted to
/// `<control-plane>/frameworks.json`. `root` also anchors installed framework
/// packages under `<root>/frameworks/<publisher>/<id>/`.
#[derive(Debug, Clone)]
pub struct FrameworkRegistry {
    pub(super) root: PathBuf,
    pub(super) path: PathBuf,
}

impl FrameworkRegistry {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let registry = Self {
            path: root.join(FRAMEWORKS_FILE),
            root,
        };
        let _ = registry.recover_uninstall_tombstones();
        let _ = registry.recover_lifecycle_journals();
        let _ = crate::install::recover_art_uninstall_tombstones(&registry.root);
        let _ = crate::install::recover_art_lifecycle(&registry.root);
        let _ = crate::dependency::RuntimeRegistry::new(&registry.root).prune_stale();
        registry
    }

    /// Directory holding this framework's active immutable package version:
    /// `<root>/frameworks/<publisher>/<id>/versions/<version-digest>/`.
    pub fn runtime_dir(&self, id: &str) -> PathBuf {
        resolve_framework_package_dir(&self.root.join(FRAMEWORK_PACKAGES_DIR), id)
            .unwrap_or_else(|_| self.package_root(id))
    }

    pub(super) fn package_root(&self, reference: &str) -> PathBuf {
        let storage_key = self
            .resolve_state_key(reference)
            .ok()
            .flatten()
            .unwrap_or_else(|| reference.to_owned());
        let relative = framework_storage_path(&storage_key).unwrap_or_else(|| {
            Path::new(".unresolved").join(if is_valid_framework(reference) {
                reference
            } else {
                "invalid"
            })
        });
        self.root.join(FRAMEWORK_PACKAGES_DIR).join(relative)
    }

    pub(super) fn activation_path(&self, id: &str) -> PathBuf {
        self.package_root(id).join(FRAMEWORK_ACTIVE_FILE)
    }

    pub(super) fn activation(&self, id: &str) -> Option<FrameworkActivationState> {
        serde_json::from_slice(
            &read_bounded_file(&self.activation_path(id), FRAMEWORK_METADATA_MAX_BYTES).ok()?,
        )
        .ok()
    }

    pub(super) fn resolve_state_key(
        &self,
        reference: &str,
    ) -> Result<Option<String>, FrameworkError> {
        let states = self.installation_states()?;
        if states.contains_key(reference) {
            return Ok(Some(reference.to_owned()));
        }
        if reference.contains('/') {
            return Ok(None);
        }
        let matches = states
            .keys()
            .filter(|key| framework_local_id(key) == reference)
            .cloned()
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [] => Ok(None),
            [only] => Ok(Some(only.clone())),
            _ => Err(FrameworkError::AmbiguousFramework(reference.to_owned())),
        }
    }

    pub(super) fn write_activation(
        &self,
        id: &str,
        activation: &FrameworkActivationState,
    ) -> Result<(), FrameworkError> {
        let path = self.activation_path(id);
        let parent = path
            .parent()
            .ok_or_else(|| FrameworkError::RuntimeUnavailable {
                id: id.to_owned(),
                reason: "activation path has no parent".to_owned(),
            })?;
        fs::create_dir_all(parent)?;
        let temporary = path.with_extension("json.tmp");
        let mut bytes = serde_json::to_vec_pretty(activation)?;
        bytes.push(b'\n');
        fs::write(&temporary, bytes)?;
        crate::replace_registry_file(&temporary, &path)?;
        Ok(())
    }

    pub(super) fn lifecycle_path(&self, reference: &str) -> PathBuf {
        self.package_root(reference).join(FRAMEWORK_LIFECYCLE_FILE)
    }

    pub(super) fn write_lifecycle_journal(
        &self,
        reference: &str,
        journal: &FrameworkLifecycleJournal,
    ) -> Result<(), FrameworkError> {
        let path = self.lifecycle_path(reference);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("json.tmp");
        let mut bytes = serde_json::to_vec_pretty(journal)?;
        bytes.push(b'\n');
        fs::write(&temporary, bytes)?;
        crate::replace_registry_file(&temporary, &path)?;
        Ok(())
    }

    pub(super) fn clear_lifecycle_journal(&self, reference: &str) {
        let _ = fs::remove_file(self.lifecycle_path(reference));
    }

    pub(super) fn recover_lifecycle_journals(&self) -> Result<(), FrameworkError> {
        let root = self.root.join(FRAMEWORK_PACKAGES_DIR);
        if !is_directory_without_links(&root)? {
            return Ok(());
        }
        let mut package_roots = Vec::new();
        for first in fs::read_dir(&root)? {
            let first = first?.path();
            if !is_directory_without_links(&first)? {
                continue;
            }
            for second in fs::read_dir(&first).into_iter().flatten().flatten() {
                let second = second.path();
                if is_directory_without_links(&second)?
                    && is_file_without_links(&second.join(FRAMEWORK_LIFECYCLE_FILE))?
                {
                    package_roots.push(second);
                }
            }
        }
        for package_root in package_roots {
            let journal_path = package_root.join(FRAMEWORK_LIFECYCLE_FILE);
            let journal: FrameworkLifecycleJournal = match serde_json::from_slice(
                &read_bounded_file(&journal_path, FRAMEWORK_METADATA_MAX_BYTES)?,
            ) {
                Ok(journal) => journal,
                Err(_) => {
                    let _ = fs::rename(&journal_path, journal_path.with_extension("corrupt"));
                    continue;
                }
            };
            if !framework_lifecycle_journal_is_safe(&journal) {
                let _ = fs::rename(&journal_path, journal_path.with_extension("corrupt"));
                continue;
            }
            let activation_path = package_root.join(FRAMEWORK_ACTIVE_FILE);
            let current = serde_json::from_slice::<FrameworkActivationState>(
                &read_bounded_file(&activation_path, FRAMEWORK_METADATA_MAX_BYTES)
                    .unwrap_or_default(),
            )
            .ok();
            if current.as_ref() != Some(&journal.next_activation) {
                if let Some(old) = &journal.old_activation {
                    let temporary = activation_path.with_extension("json.tmp");
                    let mut bytes = serde_json::to_vec_pretty(old)?;
                    bytes.push(b'\n');
                    fs::write(&temporary, bytes)?;
                    crate::replace_registry_file(&temporary, &activation_path)?;
                } else {
                    let _ = fs::remove_file(&activation_path);
                }
                // Only a directory this operation created may be removed. Reused and older
                // directories hold versions that existed before the interrupted operation, and the
                // activation just restored above may well point at one of them.
                if journal.created_target {
                    let target = package_root.join(&journal.target);
                    let _ = remove_framework_tree(&target);
                }
            }
            let _ = fs::remove_file(journal_path);
        }
        Ok(())
    }

    pub(super) fn recover_uninstall_tombstones(&self) -> Result<(), FrameworkError> {
        let root = self.root.join(FRAMEWORK_PACKAGES_DIR);
        if !is_directory_without_links(&root)? {
            return Ok(());
        }
        let mut parents = Vec::new();
        for entry in fs::read_dir(&root)? {
            let path = entry?.path();
            if is_directory_without_links(&path)?
                && path
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(loom_protocol::is_safe_publisher_id)
            {
                parents.push(path);
            }
        }
        // Restore-versus-delete is decided from this map, so a corrupt file must abort the
        // recovery: an empty map would delete every pending tombstone and silently complete
        // uninstalls the operator never asked for.
        let installed = self.installation_states()?;
        for parent in parents {
            for entry in fs::read_dir(&parent)? {
                let tombstone = entry?.path();
                if !is_directory_without_links(&tombstone)? {
                    continue;
                }
                let Some(original_name) = uninstall_tombstone_original_name(
                    &tombstone,
                    FRAMEWORK_UNINSTALL_TOMBSTONE_PREFIX,
                ) else {
                    continue;
                };
                let Some(publisher) = parent.file_name().and_then(OsStr::to_str) else {
                    continue;
                };
                let reference = format!("{publisher}/{original_name}");
                if !is_valid_framework_reference(&reference) {
                    continue;
                }
                let live = parent.join(&original_name);
                if installed.contains_key(&reference) && !live.exists() {
                    fs::rename(&tombstone, &live)?;
                } else {
                    remove_framework_tree(&tombstone)?;
                }
            }
        }
        Ok(())
    }

    pub fn trust_store_path(&self) -> PathBuf {
        self.root.join(PLUGIN_TRUST_STORE_FILE)
    }

    pub fn trust_store(&self) -> Result<TrustStore, FrameworkError> {
        Ok(TrustStore::load(&self.trust_store_path())?)
    }

    pub fn trust_publisher(&self, record: PublisherTrustRecord) -> Result<(), FrameworkError> {
        let mut store = self.trust_store()?;
        store.trust(record);
        store.write_atomic(&self.trust_store_path())?;
        Ok(())
    }

    pub fn revoke_publisher(
        &self,
        publisher_id: &str,
        key_id: &str,
    ) -> Result<bool, FrameworkError> {
        let mut store = self.trust_store()?;
        let changed = store.revoke(publisher_id, key_id);
        if changed {
            store.write_atomic(&self.trust_store_path())?;
        }
        Ok(changed)
    }

    pub fn set_trust_policy(&self, policy: TrustPolicy) -> Result<TrustStore, FrameworkError> {
        let mut store = self.trust_store()?;
        store.set_policy(policy);
        store.write_atomic(&self.trust_store_path())?;
        Ok(store)
    }

    pub fn trust_publisher_directory(
        &self,
        publisher_id: &str,
        records: impl IntoIterator<Item = PublisherTrustRecord>,
    ) -> Result<TrustStore, FrameworkError> {
        let mut store = self.trust_store()?;
        store.untrust_publisher_id(publisher_id);
        store.trust_publisher_id(publisher_id.to_owned());
        for record in records {
            store.trust(record);
        }
        store.write_atomic(&self.trust_store_path())?;
        Ok(store)
    }

    pub fn untrust_publisher(&self, publisher_id: &str) -> Result<TrustStore, FrameworkError> {
        let mut store = self.trust_store()?;
        if store.untrust_publisher_id(publisher_id) {
            store.write_atomic(&self.trust_store_path())?;
        }
        Ok(store)
    }

    /// The set of installed framework ids. A persisted state entry is not
    /// enough by itself: the package manifest must also be present.
    pub fn installed_ids(&self) -> BTreeSet<String> {
        // Reporting cannot fail, so a corrupt state file lists nothing here. It cannot become
        // permanent: every mutating path propagates the error instead of rewriting the file.
        self.installation_states()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(id, _)| {
                if !is_valid_framework_reference(&id) {
                    return None;
                }
                self.package_manifest(&id)
                    .map(|manifest| manifest.qualified_id())
            })
            .collect()
    }

    /// Whether a specific framework is installed.
    pub fn is_installed(&self, id: &str) -> bool {
        self.resolve_state_key(id)
            .ok()
            .flatten()
            .is_some_and(|key| self.package_manifest(&key).is_some())
    }

    /// Whether an installed framework package is enabled for execution.
    pub fn is_enabled(&self, id: &str) -> bool {
        let Some(key) = self.resolve_state_key(id).ok().flatten() else {
            return false;
        };
        self.package_manifest(&key).is_some()
            && self
                .installation_states()
                .unwrap_or_default()
                .get(&key)
                .is_some_and(|state| state.enabled)
    }

    /// Readiness of a framework, probing its installed package manifest and
    /// process entry. Disabled or uninstalled packages are never ready.
    pub fn readiness(&self, id: &str) -> (bool, String) {
        let key = match self.resolve_state_key(id) {
            Ok(Some(key)) if self.package_manifest(&key).is_some() => key,
            Err(error) => return (false, error.to_string()),
            _ => return (false, "未安装".to_owned()),
        };
        if !self.is_installed(&key) {
            return (false, "未安装".to_owned());
        }
        if !self.is_enabled(&key) {
            return (false, "已禁用".to_owned());
        }
        let runtime_root = self.root.join(FRAMEWORK_PACKAGES_DIR);
        framework_ready_in(&key, Some(&runtime_root))
    }

    /// Full status for the host catalog plus any installed third-party
    /// framework packages.
    pub fn statuses(&self) -> Vec<FrameworkStatus> {
        let installed = self.installed_ids();
        let installed_local_ids = installed
            .iter()
            .map(|id| framework_local_id(id).to_owned())
            .collect::<BTreeSet<_>>();
        let mut ids = installed;
        ids.extend(
            FRAMEWORK_IDS
                .iter()
                .filter(|id| !installed_local_ids.contains(**id))
                .map(|id| (*id).to_owned()),
        );
        ids.into_iter().map(|id| self.status_of(&id)).collect()
    }
    pub(super) fn status_of(&self, id: &str) -> FrameworkStatus {
        let manifest = self.package_manifest(id);
        let state_key = self.resolve_state_key(id).ok().flatten();
        let state = state_key
            .as_ref()
            // A status report cannot fail; `resolve_state_key` has already returned `None` above if
            // the state file is corrupt, so this reports the package as not installed.
            .and_then(|key| self.installation_states().ok()?.get(key).cloned());
        let installed = state.is_some() && manifest.is_some();
        let enabled = installed && state.as_ref().map(|value| value.enabled).unwrap_or(false);
        let (name, description, version) = match &manifest {
            Some(manifest) => (
                manifest.name.clone(),
                manifest.description.clone(),
                Some(manifest.version.clone()),
            ),
            None => (
                framework_name(framework_local_id(id)).to_owned(),
                framework_description(framework_local_id(id)).to_owned(),
                None,
            ),
        };
        let (ready, ready_detail) = if !installed {
            (false, "未安装".to_owned())
        } else if !enabled {
            (false, "已禁用".to_owned())
        } else {
            self.readiness(state_key.as_deref().unwrap_or(id))
        };
        let trust_status = manifest
            .as_ref()
            .and_then(|manifest| {
                self.trust_store().ok().and_then(|trust_store| {
                    verify_package_signature(
                        &self.runtime_dir(id),
                        Some(&manifest.publisher),
                        manifest.signature.as_ref(),
                        &trust_store,
                    )
                    .ok()
                })
            })
            .unwrap_or_default();
        FrameworkStatus {
            id: manifest
                .as_ref()
                .map(|manifest| manifest.id.clone())
                .unwrap_or_else(|| framework_local_id(id).to_owned()),
            qualified_id: manifest
                .as_ref()
                .map(FrameworkPackageManifest::qualified_id)
                .unwrap_or_else(|| id.to_owned()),
            name,
            description,
            installed,
            enabled,
            ready,
            ready_detail,
            version,
            runtime_dir: installed.then(|| self.runtime_dir(state_key.as_deref().unwrap_or(id))),
            publisher: manifest.as_ref().map(|value| value.publisher.clone()),
            permission_policy: manifest
                .as_ref()
                .map(|value| value.permission_policy.clone())
                .unwrap_or_default(),
            declared_permissions: manifest
                .as_ref()
                .map(|value| value.permissions.clone())
                .unwrap_or_default(),
            resources: manifest
                .as_ref()
                .map(|value| value.resources.clone())
                .unwrap_or_default(),
            authoring_schema: manifest.and_then(|value| value.authoring_schema),
            trust_status,
        }
    }

    pub(super) fn package_manifest(&self, id: &str) -> Option<FrameworkPackageManifest> {
        let manifest =
            read_framework_manifest(&self.runtime_dir(id).join(FRAMEWORK_MANIFEST_FILE)).ok()?;
        ((manifest.id == id || manifest.qualified_id() == id)
            && loom_protocol::negotiate_framework_protocol(&manifest).is_ok()
            && manifest
                .platforms
                .iter()
                .any(|platform| platform == WINDOWS_X64_PLATFORM))
        .then_some(manifest)
    }

    /// The persisted installation state.
    ///
    /// A missing file means nothing has been installed yet, which is an empty map. Anything else —
    /// an unreadable file or one whose contents are not the expected map — is an error, never an
    /// empty map: reporting corruption as "no frameworks installed" hides intact packages, and the
    /// next write would drop every entry the file still held.
    pub(super) fn installation_states(
        &self,
    ) -> Result<BTreeMap<String, FrameworkInstallationState>, FrameworkError> {
        let bytes = match read_bounded_file(&self.path, FRAMEWORK_METADATA_MAX_BYTES) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(error) => return Err(FrameworkError::Io(error)),
        };
        let installed =
            serde_json::from_slice::<BTreeMap<String, FrameworkInstallationState>>(&bytes)
                .map_err(|error| FrameworkError::CorruptState {
                    path: self.path.display().to_string(),
                    reason: error.to_string(),
                })?;
        if let Some(key) = installed
            .keys()
            .find(|key| !is_valid_framework_reference(key))
        {
            return Err(FrameworkError::CorruptState {
                path: self.path.display().to_string(),
                reason: format!("invalid framework state key `{key}`"),
            });
        }
        Ok(installed)
    }

    pub(super) fn write_installed(
        &self,
        installed: &BTreeMap<String, FrameworkInstallationState>,
    ) -> Result<(), FrameworkError> {
        if let Some(key) = installed
            .keys()
            .find(|key| !is_valid_framework_reference(key))
        {
            return Err(FrameworkError::CorruptState {
                path: self.path.display().to_string(),
                reason: format!("invalid framework state key `{key}`"),
            });
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(installed)?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, format!("{text}\n"))?;
        crate::replace_registry_file(&temporary, &self.path)?;
        Ok(())
    }
}
