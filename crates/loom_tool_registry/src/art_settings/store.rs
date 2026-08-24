use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::model::{
    ArtSettingsError, ArtSettingsFile, ArtUserSettings, ART_SETTINGS_FILE,
    ART_SETTINGS_SCHEMA_VERSION, MAX_ART_SETTINGS_DEPTH, MAX_ART_SETTINGS_FILE_BYTES,
};
use super::validation::{validate_art_reference, validate_file, validate_settings};

const MAX_CORRUPTION_BACKUPS: usize = 3;

#[derive(Clone, Debug)]
pub struct ArtSettingsStore {
    path: PathBuf,
}

impl ArtSettingsStore {
    #[must_use]
    pub fn new(control_plane_root: impl AsRef<Path>) -> Self {
        Self {
            path: control_plane_root.as_ref().join(ART_SETTINGS_FILE),
        }
    }

    pub fn get(&self, art_id: &str) -> Result<ArtUserSettings, ArtSettingsError> {
        Ok(self.get_optional(art_id)?.unwrap_or_default())
    }

    pub fn get_optional(&self, art_id: &str) -> Result<Option<ArtUserSettings>, ArtSettingsError> {
        validate_art_reference(art_id)?;
        let _lock = crate::private_store::lock_private_file(&self.path)?;
        Ok(self.read_file()?.arts.remove(art_id))
    }

    pub fn list(
        &self,
    ) -> Result<std::collections::BTreeMap<String, ArtUserSettings>, ArtSettingsError> {
        let _lock = crate::private_store::lock_private_file(&self.path)?;
        Ok(self.read_file()?.arts)
    }

    pub fn save(
        &self,
        art_id: &str,
        settings: ArtUserSettings,
    ) -> Result<ArtUserSettings, ArtSettingsError> {
        validate_art_reference(art_id)?;
        validate_settings(&settings)?;
        let _lock = crate::private_store::lock_private_file(&self.path)?;
        let mut file = self.read_file()?;
        file.arts.insert(art_id.to_owned(), settings.clone());
        self.write_file(&file)?;
        Ok(settings)
    }

    pub fn delete(&self, art_id: &str) -> Result<bool, ArtSettingsError> {
        validate_art_reference(art_id)?;
        let _lock = crate::private_store::lock_private_file(&self.path)?;
        let mut file = self.read_file()?;
        let deleted = file.arts.remove(art_id).is_some();
        if deleted {
            self.write_file(&file)?;
        }
        Ok(deleted)
    }

    fn read_file(&self) -> Result<ArtSettingsFile, ArtSettingsError> {
        let bytes = match crate::private_store::read_bounded_private_file(
            &self.path,
            MAX_ART_SETTINGS_FILE_BYTES,
        ) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ArtSettingsFile::default());
            }
            Err(error) => return Err(ArtSettingsError::Io(error)),
        };
        let value: Value = match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => return Ok(self.recover_corrupt_file(&bytes)),
        };
        if !loom_security::json::value_is_within_depth(&value, MAX_ART_SETTINGS_DEPTH) {
            return Err(ArtSettingsError::InvalidDocument(format!(
                "nesting exceeds {MAX_ART_SETTINGS_DEPTH} levels"
            )));
        }
        let file: ArtSettingsFile = match serde_json::from_value(value) {
            Ok(file) => file,
            Err(_) => return Ok(self.recover_corrupt_file(&bytes)),
        };
        if file.schema_version != ART_SETTINGS_SCHEMA_VERSION {
            return Err(ArtSettingsError::UnsupportedSchemaVersion {
                actual: file.schema_version,
                expected: ART_SETTINGS_SCHEMA_VERSION,
            });
        }
        validate_file(&file)?;
        Ok(file)
    }

    fn recover_corrupt_file(&self, bytes: &[u8]) -> ArtSettingsFile {
        self.write_corruption_backup(bytes);
        let recovered = ArtSettingsFile::default();
        let _ = self.write_file(&recovered);
        recovered
    }

    fn write_corruption_backup(&self, bytes: &[u8]) {
        let Some(parent) = self.path.parent() else {
            return;
        };
        let Some(name) = self.path.file_name().and_then(|name| name.to_str()) else {
            return;
        };
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for attempt in 0..10u32 {
            let backup = parent.join(format!(
                "{name}.corrupt-{}-{timestamp}-{attempt}",
                std::process::id()
            ));
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let Ok(mut output) = options.open(&backup) else {
                continue;
            };
            let result = (|| -> std::io::Result<()> {
                output.write_all(bytes)?;
                output.sync_all()?;
                loom_plugin_security::restrict_private_path_permissions(&backup, false)
            })();
            if result.is_err() {
                let _ = fs::remove_file(&backup);
            }
            break;
        }
        self.prune_corruption_backups();
    }

    fn prune_corruption_backups(&self) {
        let Some(parent) = self.path.parent() else {
            return;
        };
        let Some(name) = self.path.file_name().and_then(|name| name.to_str()) else {
            return;
        };
        let prefix = format!("{name}.corrupt-");
        let Ok(entries) = fs::read_dir(parent) else {
            return;
        };
        let mut backups = entries
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with(&prefix))
                    && entry.file_type().is_ok_and(|kind| kind.is_file())
            })
            .collect::<Vec<_>>();
        backups.sort_by_key(|entry| {
            (
                entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
                entry.file_name(),
            )
        });
        let remove_count = backups.len().saturating_sub(MAX_CORRUPTION_BACKUPS);
        for backup in backups.into_iter().take(remove_count) {
            let _ = fs::remove_file(backup.path());
        }
    }

    fn write_file(&self, file: &ArtSettingsFile) -> Result<(), ArtSettingsError> {
        validate_file(file)?;
        let mut bytes = serde_json::to_vec_pretty(file)?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_ART_SETTINGS_FILE_BYTES {
            return Err(ArtSettingsError::StoreTooLarge {
                max_bytes: MAX_ART_SETTINGS_FILE_BYTES,
            });
        }
        crate::private_store::write_private_file_atomic(&self.path, &bytes)
            .map_err(ArtSettingsError::Io)
    }
}
