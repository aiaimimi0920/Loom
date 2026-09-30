//! Per-account durable outbox. Persist before central mutation; retain only current/pending PNGs.
use crate::{
    error, validation, CentralOperation, Envelope, Identity, Result, Snapshot, Status, View,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const MAX_ENTRIES: usize = 64;
const MAX_BYTES: usize = 48 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 12 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Entry {
    pub owner: String,
    pub unit_id: String,
    pub source: bool,
    pub envelope: Envelope,
    pub snapshot: Snapshot,
    pub pending_snapshot: Option<Snapshot>,
    pub pending: Option<CentralOperation>,
    pub view: Option<View>,
    pub stopped: bool,
    #[serde(skip)]
    pub transfer: Option<crate::TransferPath>,
    #[serde(skip)]
    pub last_error: Option<String>,
}

impl Entry {
    pub fn id(&self) -> &str {
        &self.envelope.projection_id
    }
    pub fn bytes(&self) -> usize {
        self.snapshot.png.len() + self.pending_snapshot.as_ref().map_or(0, |s| s.png.len())
    }

    pub fn merge_view(&mut self, mut view: View) -> Result<()> {
        if self.envelope != view.record.envelope {
            return Err(error(403, "projection_source_mismatch"));
        }
        if let Some(previous) = &self.view {
            // A sync begun before publish/accept may finish later. Revocation still wins.
            let stale = view.record.revision < previous.record.revision
                || view.record.updated_at_ms < previous.record.updated_at_ms
                || (previous.record.status != Status::Invited
                    && view.record.status == Status::Invited);
            if stale && view.record.status != Status::Stopped {
                if view.available {
                    return Ok(());
                }
                view.record = previous.record.clone();
            }
            if previous.record.status == Status::Stopped {
                return Ok(());
            }
        }
        self.view = Some(view);
        Ok(())
    }
    fn validate(&self, identity: &Identity) -> Result<()> {
        self.envelope.validate(identity.origin())?;
        if !validation::identifier(&self.owner)
            || !validation::identifier(&self.unit_id)
            || self.envelope.source.account_id != identity.session().account_id
            || self.source != (self.envelope.source.device_id == identity.session().device_id)
            || (self.source && self.unit_id != self.envelope.source.unit_id)
        {
            return Err(error(403, "projection_state_invalid"));
        }
        for snapshot in std::iter::once(&self.snapshot).chain(self.pending_snapshot.iter()) {
            snapshot.validate()?;
            if snapshot.projection_id != self.id()
                || snapshot.source_session_id != self.envelope.source.session_id
            {
                return Err(error(400, "projection_state_invalid"));
            }
        }
        if let Some(pending) = &self.pending {
            let valid = match pending {
                CentralOperation::Create { envelope, .. } => {
                    self.source && envelope == &self.envelope && self.snapshot.revision == 1
                }
                CentralOperation::Accept {
                    envelope,
                    receiver_unit_id,
                    expected_revision,
                    expected_digest,
                    confirmed,
                } => {
                    !self.source
                        && envelope == &self.envelope
                        && receiver_unit_id == &self.unit_id
                        && *confirmed
                        && *expected_revision == self.snapshot.revision
                        && expected_digest == &self.snapshot.digest
                }
                CentralOperation::Publish {
                    projection_id,
                    source_session_id,
                    prior_revision,
                    revision,
                    digest,
                    width,
                    height,
                    byte_length,
                } => {
                    self.source
                        && projection_id == self.id()
                        && source_session_id == &self.envelope.source.session_id
                        && *prior_revision == self.snapshot.revision
                        && *revision == prior_revision + 1
                        && self.pending_snapshot.as_ref().is_some_and(|s| {
                            s.revision == *revision
                                && &s.digest == digest
                                && s.width == *width
                                && s.height == *height
                                && s.png.len() == *byte_length
                        })
                }
                CentralOperation::Unlink { projection_id } => {
                    projection_id == self.id() && self.stopped
                }
                _ => false,
            };
            if !valid {
                return Err(error(400, "projection_state_invalid"));
            }
        } else if self.pending_snapshot.is_some() {
            return Err(error(400, "projection_state_invalid"));
        }
        Ok(())
    }
}

pub(crate) struct Store {
    root: PathBuf,
    pub entries: BTreeMap<String, Entry>,
}

impl Store {
    pub fn load(root: &Path, identity: &Identity) -> Result<Self> {
        let namespace = format!(
            "{}\n{}\n{}\n{}",
            identity.origin(),
            identity.session().account_id,
            identity.session().device_id,
            identity.session().public_key
        );
        let root = root.join(format!("{:x}", Sha256::digest(namespace.as_bytes())));
        fs::create_dir_all(&root).map_err(io_error)?;
        let mut store = Self {
            root,
            entries: BTreeMap::new(),
        };
        for file in fs::read_dir(&store.root).map_err(io_error)? {
            let file = file.map_err(io_error)?;
            if file.path().extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let metadata = fs::symlink_metadata(file.path()).map_err(io_error)?;
            if !metadata.is_file()
                || metadata.len() > MAX_FILE_BYTES
                || store.entries.len() >= MAX_ENTRIES
            {
                return Err(error(413, "projection_storage_budget"));
            }
            let mut entry: Entry =
                serde_json::from_slice(&fs::read(file.path()).map_err(io_error)?)
                    .map_err(|_| error(400, "projection_state_invalid"))?;
            entry.validate(identity)?;
            if file.file_name() != std::ffi::OsStr::new(&format!("{}.json", &entry.id()[11..])) {
                return Err(error(400, "projection_state_invalid"));
            }
            if let Some(view) = &mut entry.view {
                view.available = false;
                view.authorized_until_ms = 0;
                view.peer = None;
            }
            store.admit(&entry)?;
            store.entries.insert(entry.id().to_owned(), entry);
        }
        Ok(store)
    }

    pub fn owned(&self, id: &str, owner: &str) -> Result<&Entry> {
        let entry = self
            .entries
            .get(id)
            .ok_or(error(409, "projection_account_mismatch"))?;
        if entry.owner != owner {
            return Err(error(403, "projection_access_denied"));
        }
        Ok(entry)
    }

    fn admit(&self, entry: &Entry) -> Result<()> {
        if !self.entries.contains_key(entry.id()) && self.entries.len() >= MAX_ENTRIES {
            return Err(error(429, "projection_storage_budget"));
        }
        let others = self.entries.values().filter(|e| e.id() != entry.id());
        if entry.source
            && !entry.stopped
            && others.clone().filter(|e| e.source && !e.stopped).count() >= 8
        {
            return Err(error(429, "projection_source_limit"));
        }
        if others.map(Entry::bytes).sum::<usize>() + entry.bytes() > MAX_BYTES {
            return Err(error(413, "projection_storage_budget"));
        }
        Ok(())
    }

    pub async fn save(&mut self, entry: Entry) -> Result<()> {
        self.admit(&entry)?;
        let path = self.root.join(format!("{}.json", &entry.id()[11..]));
        let persisted = entry.clone();
        tokio::task::spawn_blocking(move || atomic_save(&path, &persisted))
            .await
            .map_err(|_| error(503, "projection_storage_failed"))??;
        self.entries.insert(entry.id().to_owned(), entry);
        Ok(())
    }
}

fn atomic_save(path: &Path, entry: &Entry) -> Result<()> {
    let bytes = serde_json::to_vec(entry).map_err(|_| error(500, "projection_storage_failed"))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(error(413, "projection_storage_budget"));
    }
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(io_error)?;
        file.write_all(&bytes).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        fs::rename(&temporary, path).map_err(io_error)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
fn io_error(_: std::io::Error) -> crate::Error {
    error(503, "projection_storage_failed")
}
