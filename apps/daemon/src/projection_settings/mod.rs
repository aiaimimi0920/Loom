//! Loom-owned recipient rules and device groups; identity inputs must be authenticated.
mod directory;
mod model;
use crate::ProjectionError;
pub(crate) use directory::append_groups;
use fs2::FileExt;
pub(crate) use model::{Decision, DeviceRef, Document, Update};
use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
};

const MAX_BYTES: usize = 262_144;
pub(crate) struct ProjectionSettings {
    path: PathBuf,
    document: Document,
    failed: bool,
    _writer: File,
}
impl ProjectionSettings {
    pub(crate) fn open(root: &Path) -> anyhow::Result<Self> {
        fs::create_dir_all(root)?;
        crate::restrict_sensitive_path_permissions(root, true)?;
        let lock_path = root.join("writer.lock");
        let writer = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        crate::restrict_sensitive_path_permissions(&lock_path, false)?;
        writer.try_lock_exclusive()?;
        let path = root.join("settings.json");
        let document = match File::open(&path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
                anyhow::ensure!(
                    bytes.len() <= MAX_BYTES,
                    "projection settings exceed size limit"
                );
                let document: Document = serde_json::from_slice(&bytes)?;
                model::validate(&document).map_err(|error| anyhow::anyhow!("{}", error.code))?;
                document
            }
            Err(error) if error.kind() == ErrorKind::NotFound => Document {
                storage_version: 1,
                revision: 0,
                groups: vec![],
                rules: vec![],
            },
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            path,
            document,
            failed: false,
            _writer: writer,
        })
    }
    pub(crate) fn view(&self) -> Result<&Document, ProjectionError> {
        if self.failed {
            return Err(ProjectionError::new(503, "projection_settings_unavailable"));
        }
        Ok(&self.document)
    }
    pub(crate) fn update(&mut self, input: Update) -> Result<&Document, ProjectionError> {
        if input.expected_revision != self.view()?.revision {
            return Err(ProjectionError::new(
                409,
                "projection_settings_revision_conflict",
            ));
        }
        let next = Document {
            storage_version: 1,
            revision: self.document.revision + 1,
            groups: input.groups,
            rules: input.rules,
        };
        model::validate(&next)?;
        let bytes = serde_json::to_vec(&next)
            .map_err(|_| ProjectionError::new(503, "projection_settings_unavailable"))?;
        if bytes.len() > MAX_BYTES {
            return Err(ProjectionError::new(413, "projection_settings_capacity"));
        }
        if crate::write_bytes_atomically(
            &self.path,
            &bytes,
            crate::AtomicWritePermissions::Restrict,
        )
        .is_err()
        {
            // A post-rename failure has an uncertain durable result; fail closed until reopened.
            self.failed = true;
            return Err(ProjectionError::new(503, "projection_settings_unavailable"));
        }
        self.document = next;
        self.view()
    }
    pub(crate) fn decision(
        &self,
        target: &str,
        source: &DeviceRef,
        verified_user: Option<&str>,
    ) -> Result<Decision, ProjectionError> {
        let document = self.view()?;
        let Some(rule) = document.rules.iter().find(|rule| rule.device_id == target) else {
            return Ok(Decision::Confirm);
        };
        let allowed = rule.whitelist.devices.contains(source)
            || document.groups.iter().any(|group| {
                rule.whitelist.groups.contains(&group.group_id) && group.members.contains(source)
            })
            || verified_user
                .is_some_and(|user| rule.whitelist.users.iter().any(|value| value == user));
        Ok(if allowed {
            Decision::Auto
        } else if rule.blacklist.contains(source) {
            Decision::Reject
        } else {
            rule.policy
        })
    }
}

#[cfg(test)]
mod tests;
