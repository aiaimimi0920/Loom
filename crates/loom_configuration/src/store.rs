use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{
    ConfigRegistry, ManagedAppId, ManagedConfigDocument, ManagedConfigError, ManagedConfigErrorCode,
};

#[derive(Debug, Clone)]
pub struct FileDocumentStore {
    root: PathBuf,
}

impl FileDocumentStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn read_or_create(
        &self,
        app: ManagedAppId,
        registry: &ConfigRegistry,
    ) -> Result<(ManagedConfigDocument, bool), ManagedConfigError> {
        let adapter = registry.get(app).ok_or_else(|| {
            ManagedConfigError::new(
                ManagedConfigErrorCode::UnknownApp,
                format!("unknown managed app: {app}"),
            )
        })?;
        let path = self.path_for(app);
        if path.exists() {
            let raw = std::fs::read_to_string(&path).map_err(storage_error)?;
            let document: ManagedConfigDocument =
                serde_json::from_str(&raw).map_err(storage_error)?;
            return Ok((document, false));
        }

        let document =
            ManagedConfigDocument::new(app, adapter.schema_version(), adapter.default_config());
        self.persist(&document)?;
        Ok((document, true))
    }

    pub fn write_validated(
        &self,
        app: ManagedAppId,
        expected_revision: u64,
        value: Value,
        registry: &ConfigRegistry,
    ) -> Result<ManagedConfigDocument, ManagedConfigError> {
        let adapter = registry.get(app).ok_or_else(|| {
            ManagedConfigError::new(
                ManagedConfigErrorCode::UnknownApp,
                format!("unknown managed app: {app}"),
            )
        })?;
        let (mut document, _) = self.read_or_create(app, registry)?;
        if document.revision != expected_revision {
            return Err(ManagedConfigError::new(
                ManagedConfigErrorCode::RevisionConflict,
                "configuration was updated by another writer",
            ));
        }
        let normalized = adapter.normalize_and_validate(value)?;
        document.replace_config(expected_revision, normalized)?;
        self.persist(&document)?;
        Ok(document)
    }

    fn persist(&self, document: &ManagedConfigDocument) -> Result<(), ManagedConfigError> {
        std::fs::create_dir_all(&self.root).map_err(storage_error)?;
        let body = serde_json::to_string_pretty(document).map_err(storage_error)?;
        std::fs::write(self.path_for(document.app), body).map_err(storage_error)
    }

    fn path_for(&self, app: ManagedAppId) -> PathBuf {
        self.root.join(format!("{app}.json"))
    }
}

pub fn default_configuration_root() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| path.join("Loom").join("configuration").join("apps"))
        .unwrap_or_else(|| {
            PathBuf::from(".runtime")
                .join("loom")
                .join("configuration")
                .join("apps")
        })
}

fn storage_error(error: impl std::fmt::Display) -> ManagedConfigError {
    ManagedConfigError::new(ManagedConfigErrorCode::StorageError, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{built_in_registry, ManagedAppId, ManagedConfigErrorCode};
    use serde_json::json;
    use std::sync::Mutex;

    static APPDATA_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn default_root_uses_appdata_when_present() {
        let _guard = APPDATA_ENV_LOCK.lock().expect("lock APPDATA env");
        let previous = std::env::var_os("APPDATA");
        std::env::set_var("APPDATA", r"C:\Users\demo\AppData\Roaming");
        assert_eq!(
            default_configuration_root(),
            PathBuf::from(r"C:\Users\demo\AppData\Roaming")
                .join("Loom")
                .join("configuration")
                .join("apps")
        );
        match previous {
            Some(value) => std::env::set_var("APPDATA", value),
            None => std::env::remove_var("APPDATA"),
        }
    }

    #[test]
    fn default_root_uses_loom_runtime_fallback_when_appdata_is_absent() {
        let _guard = APPDATA_ENV_LOCK.lock().expect("lock APPDATA env");
        let previous = std::env::var_os("APPDATA");
        std::env::remove_var("APPDATA");
        assert_eq!(
            default_configuration_root(),
            PathBuf::from(".runtime")
                .join("loom")
                .join("configuration")
                .join("apps")
        );
        match previous {
            Some(value) => std::env::set_var("APPDATA", value),
            None => std::env::remove_var("APPDATA"),
        }
    }

    #[test]
    fn read_or_create_persists_default_document() {
        let root = std::env::temp_dir().join(format!("loom-config-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = FileDocumentStore::new(&root);
        let registry = built_in_registry();

        let (document, created) = store
            .read_or_create(ManagedAppId::Tea, &registry)
            .expect("create Tea document");

        assert_eq!(document.revision, 1);
        assert!(created);
        assert!(root.join("tea.json").exists());
        let (reread, reread_created) = store
            .read_or_create(ManagedAppId::Tea, &registry)
            .expect("read Tea document");
        assert_eq!(reread.revision, 1);
        assert!(!reread_created);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn write_validated_rejects_stale_revision() {
        let root =
            std::env::temp_dir().join(format!("loom-config-conflict-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = FileDocumentStore::new(&root);
        let registry = built_in_registry();
        let (_document, created) = store
            .read_or_create(ManagedAppId::Tea, &registry)
            .expect("create");
        assert!(created);

        let error = store
            .write_validated(
                ManagedAppId::Tea,
                99,
                json!({
                    "notifications_enabled": false,
                    "human_ticket_default_approval_policy": "human_before_execute",
                    "hook_ticket_default_approval_policy": "plan_only"
                }),
                &registry,
            )
            .expect_err("stale write fails");

        assert_eq!(error.code(), ManagedConfigErrorCode::RevisionConflict);
        let _ = std::fs::remove_dir_all(&root);
    }
}
