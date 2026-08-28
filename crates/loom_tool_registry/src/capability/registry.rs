use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use loom_protocol::CapabilityPackageManifest;

use super::types::{
    CapabilityInstallError, CapabilityInstalledVersion, CapabilityLifecycleStatus,
    CapabilityPluginRecord, CapabilityRegistryDocument, CapabilityResult,
    CAPABILITY_REGISTRY_MAX_BYTES, CAPABILITY_REGISTRY_SCHEMA_VERSION,
};
use crate::private_store::{
    lock_private_file, read_bounded_private_file, write_private_file_atomic,
};

const REGISTRY_FILE: &str = "registry.json";

#[derive(Clone, Debug)]
pub struct CapabilityPluginRegistry {
    control_plane_root: PathBuf,
}

impl CapabilityPluginRegistry {
    #[must_use]
    pub fn new(control_plane_root: impl AsRef<Path>) -> Self {
        Self {
            control_plane_root: control_plane_root.as_ref().to_path_buf(),
        }
    }

    #[must_use]
    pub fn control_plane_root(&self) -> &Path {
        &self.control_plane_root
    }

    #[must_use]
    pub fn packages_root(&self) -> PathBuf {
        self.control_plane_root.join("capabilities")
    }

    pub fn list(&self) -> CapabilityResult<Vec<CapabilityPluginRecord>> {
        let path = self.registry_path();
        let _lock = lock_private_file(&path)?;
        let mut plugins = self.read_document()?.plugins;
        sort_plugins(&mut plugins);
        Ok(plugins)
    }

    pub fn get(&self, qualified_id: &str) -> CapabilityResult<Option<CapabilityPluginRecord>> {
        Ok(self
            .list()?
            .into_iter()
            .find(|plugin| plugin.qualified_id == qualified_id))
    }

    /// Records a runtime activation fault without discarding the installed version.
    pub fn mark_faulted(&self, qualified_id: &str) -> CapabilityResult<CapabilityPluginRecord> {
        self.mutate_record(qualified_id, |record| {
            record.enabled_intent = false;
            record.status = CapabilityLifecycleStatus::Faulted;
            Ok(())
        })
    }

    /// Restores the pre-transition record when runtime activation fails after a registry commit.
    pub fn compensate_runtime_failure(
        &self,
        expected_digest: Option<&str>,
        previous: CapabilityPluginRecord,
    ) -> CapabilityResult<()> {
        let current = self
            .get(&previous.qualified_id)?
            .ok_or_else(|| CapabilityInstallError::NotFound(previous.qualified_id.clone()))?;
        if current.active_digest.as_deref() != expected_digest {
            return Err(CapabilityInstallError::Conflict(
                "capability state changed while compensating runtime activation".to_owned(),
            ));
        }
        self.restore_record(previous)
    }

    pub(super) fn register_install(
        &self,
        manifest: &CapabilityPackageManifest,
        installed: CapabilityInstalledVersion,
    ) -> CapabilityResult<CapabilityPluginRecord> {
        let path = self.registry_path();
        let _lock = lock_private_file(&path)?;
        let mut document = self.read_document()?;
        let qualified_id = manifest.qualified_id();
        let record = if let Some(existing) = document
            .plugins
            .iter_mut()
            .find(|plugin| plugin.qualified_id == qualified_id)
        {
            existing.name.clone_from(&manifest.name);
            existing.description.clone_from(&manifest.description);
            existing
                .requested_permissions
                .clone_from(&manifest.permissions);
            existing
                .versions
                .retain(|version| version.digest != installed.digest);
            existing.versions.push(installed);
            sort_versions(&mut existing.versions);
            existing.clone()
        } else {
            let record = CapabilityPluginRecord {
                qualified_id,
                publisher_id: manifest.publisher.id.clone(),
                package_id: manifest.id.clone(),
                name: manifest.name.clone(),
                description: manifest.description.clone(),
                enabled_intent: false,
                status: CapabilityLifecycleStatus::InstalledDisabled,
                active_digest: None,
                previous_digest: None,
                requested_permissions: manifest.permissions.clone(),
                versions: vec![installed],
            };
            document.plugins.push(record.clone());
            record
        };
        sort_plugins(&mut document.plugins);
        validate_document(&document)?;
        self.write_document(&document)?;
        Ok(record)
    }

    pub(super) fn mutate_record(
        &self,
        qualified_id: &str,
        mutate: impl FnOnce(&mut CapabilityPluginRecord) -> CapabilityResult<()>,
    ) -> CapabilityResult<CapabilityPluginRecord> {
        let path = self.registry_path();
        let _lock = lock_private_file(&path)?;
        let mut document = self.read_document()?;
        let record = document
            .plugins
            .iter_mut()
            .find(|plugin| plugin.qualified_id == qualified_id)
            .ok_or_else(|| CapabilityInstallError::NotFound(qualified_id.to_owned()))?;
        mutate(record)?;
        let result = record.clone();
        validate_document(&document)?;
        self.write_document(&document)?;
        Ok(result)
    }

    pub(super) fn restore_record(&self, record: CapabilityPluginRecord) -> CapabilityResult<()> {
        let path = self.registry_path();
        let _lock = lock_private_file(&path)?;
        let mut document = self.read_document()?;
        document
            .plugins
            .retain(|plugin| plugin.qualified_id != record.qualified_id);
        document.plugins.push(record);
        sort_plugins(&mut document.plugins);
        validate_document(&document)?;
        self.write_document(&document)
    }

    pub(super) fn remove_record(
        &self,
        qualified_id: &str,
    ) -> CapabilityResult<CapabilityPluginRecord> {
        let path = self.registry_path();
        let _lock = lock_private_file(&path)?;
        let mut document = self.read_document()?;
        let index = document
            .plugins
            .iter()
            .position(|plugin| plugin.qualified_id == qualified_id)
            .ok_or_else(|| CapabilityInstallError::NotFound(qualified_id.to_owned()))?;
        let removed = document.plugins.remove(index);
        validate_document(&document)?;
        self.write_document(&document)?;
        Ok(removed)
    }

    fn registry_path(&self) -> PathBuf {
        self.packages_root().join(REGISTRY_FILE)
    }

    fn read_document(&self) -> CapabilityResult<CapabilityRegistryDocument> {
        let path = self.registry_path();
        if !path.exists() {
            return Ok(CapabilityRegistryDocument::default());
        }
        let bytes = read_bounded_private_file(&path, CAPABILITY_REGISTRY_MAX_BYTES)?;
        let document: CapabilityRegistryDocument = serde_json::from_slice(&bytes)?;
        validate_document(&document)?;
        Ok(document)
    }

    fn write_document(&self, document: &CapabilityRegistryDocument) -> CapabilityResult<()> {
        let mut bytes = serde_json::to_vec_pretty(document)?;
        bytes.push(b'\n');
        if bytes.len() as u64 > CAPABILITY_REGISTRY_MAX_BYTES {
            return Err(CapabilityInstallError::InvalidRegistry(
                "registry exceeds 4 MiB".to_owned(),
            ));
        }
        write_private_file_atomic(&self.registry_path(), &bytes)?;
        Ok(())
    }
}

fn validate_document(document: &CapabilityRegistryDocument) -> CapabilityResult<()> {
    if document.schema_version != CAPABILITY_REGISTRY_SCHEMA_VERSION {
        return Err(CapabilityInstallError::InvalidRegistry(format!(
            "unsupported schema version {}",
            document.schema_version
        )));
    }
    let mut plugins = HashSet::new();
    for plugin in &document.plugins {
        let expected = format!("{}/{}", plugin.publisher_id, plugin.package_id);
        if plugin.qualified_id != expected || !plugins.insert(plugin.qualified_id.clone()) {
            return Err(CapabilityInstallError::InvalidRegistry(
                "duplicate or mismatched qualified id".to_owned(),
            ));
        }
        let mut versions = HashSet::new();
        for version in &plugin.versions {
            if !versions.insert(version.digest.clone())
                || !valid_digest(&version.digest)
                || !valid_registry_path(&version.relative_path)
            {
                return Err(CapabilityInstallError::InvalidRegistry(format!(
                    "invalid installed version for {}",
                    plugin.qualified_id
                )));
            }
        }
        for digest in [&plugin.active_digest, &plugin.previous_digest]
            .into_iter()
            .flatten()
        {
            if !plugin
                .versions
                .iter()
                .any(|version| &version.digest == digest)
            {
                return Err(CapabilityInstallError::InvalidRegistry(format!(
                    "lifecycle digest for {} is not installed",
                    plugin.qualified_id
                )));
            }
        }
        if plugin.status == CapabilityLifecycleStatus::Active
            && (!plugin.enabled_intent || plugin.active_digest.is_none())
        {
            return Err(CapabilityInstallError::InvalidRegistry(format!(
                "active plugin {} lacks enabled intent or digest",
                plugin.qualified_id
            )));
        }
    }
    Ok(())
}

fn valid_registry_path(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\\')
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn sort_plugins(plugins: &mut [CapabilityPluginRecord]) {
    plugins.sort_by(|left, right| left.qualified_id.cmp(&right.qualified_id));
}

fn sort_versions(versions: &mut [CapabilityInstalledVersion]) {
    versions
        .sort_by(|left, right| (&left.version, &left.digest).cmp(&(&right.version, &right.digest)));
}

pub(super) fn ensure_capability_root(path: &Path) -> CapabilityResult<()> {
    fs::create_dir_all(path)?;
    loom_plugin_security::restrict_private_path_permissions(path, true)?;
    Ok(())
}
