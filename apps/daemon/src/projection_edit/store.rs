use super::*;
use std::io::Read;

pub(crate) struct Store {
    path: PathBuf,
    sessions: BTreeMap<String, Session>,
    failed: bool,
}
const MAX_SESSIONS: usize = 64;
const MAX_STORE_BYTES: usize = 8 * 1024 * 1024;

impl Store {
    pub(super) fn available(&self) -> EditResult<()> {
        if self.failed {
            return Err(error(503, "projection_edit_storage_failed"));
        }
        Ok(())
    }
    pub(crate) fn has_binding(&self, projection_id: &str) -> EditResult<bool> {
        self.available()?;
        Ok(self.find(projection_id).is_some())
    }
    pub(crate) fn document(&self, projection_id: &str) -> EditResult<Option<Value>> {
        self.available()?;
        Ok(self.find(projection_id).map(Session::response))
    }
    pub(crate) fn release(&mut self, projection_id: &str) -> EditResult<()> {
        self.available()?;
        let Some(mut session) = self.find(projection_id).cloned() else {
            return Ok(());
        };
        session.bindings.remove(projection_id);
        session.offline_bindings.remove(projection_id);
        if session.bindings.is_empty() {
            match fs::remove_file(self.path.join(format!("{}.json", &session.session_id[5..]))) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(_) => return Err(error(503, "projection_edit_storage_failed")),
            }
            self.sessions.remove(&session.session_id);
        } else {
            self.commit(session)?;
        }
        Ok(())
    }
    pub fn open(path: PathBuf) -> Result<Self> {
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            anyhow::ensure!(
                !metadata.file_type().is_symlink(),
                "edit storage cannot be a link"
            );
        }
        let entries = match fs::read_dir(&path) {
            Ok(entries) => Some(entries),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let mut sessions = BTreeMap::new();
        let mut bindings = std::collections::BTreeSet::new();
        let mut total = 0;
        for (index, entry) in entries.into_iter().flatten().enumerate() {
            anyhow::ensure!(index < MAX_SESSIONS * 2, "too many edit storage entries");
            let entry = entry?;
            if entry.path().extension().and_then(|name| name.to_str()) != Some("json") {
                continue;
            }
            anyhow::ensure!(
                entry.file_type()?.is_file() && !entry.file_type()?.is_symlink(),
                "invalid edit storage entry"
            );
            let mut bytes = Vec::new();
            fs::File::open(entry.path())?
                .take((MAX_SESSION_BYTES + 1) as u64)
                .read_to_end(&mut bytes)?;
            total += bytes.len();
            anyhow::ensure!(
                bytes.len() <= MAX_SESSION_BYTES && total <= MAX_STORE_BYTES,
                "edit storage budget exceeded"
            );
            let session: Session = serde_json::from_slice(&bytes)?;
            anyhow::ensure!(session.valid(), "invalid edit session");
            anyhow::ensure!(
                entry.file_name().to_str() == Some(&format!("{}.json", &session.session_id[5..])),
                "invalid edit filename"
            );
            anyhow::ensure!(
                session
                    .bindings
                    .iter()
                    .all(|id| bindings.insert(id.clone())),
                "duplicate edit binding"
            );
            sessions.insert(session.session_id.clone(), session);
            anyhow::ensure!(sessions.len() <= MAX_SESSIONS, "too many edit sessions");
        }
        Ok(Self {
            path,
            sessions,
            failed: false,
        })
    }
    pub(super) fn find(&self, projection_id: &str) -> Option<&Session> {
        self.sessions
            .values()
            .find(|session| session.bindings.contains(projection_id))
    }
    pub(super) fn get(&self, session_id: &str) -> EditResult<&Session> {
        self.available()?;
        self.sessions
            .get(session_id)
            .ok_or_else(|| error(404, "projection_edit_not_found"))
    }
    pub(super) fn commit(&mut self, session: Session) -> EditResult<Value> {
        self.available()?;
        if !session.valid() {
            return Err(error(400, "projection_edit_invalid_document"));
        }
        // Account for the exact pretty-JSON encoding and trailing newline written below.
        let bytes = serde_json::to_vec_pretty(&session)
            .map_err(|_| error(400, "projection_edit_invalid_document"))?;
        let total = self
            .sessions
            .values()
            .filter(|item| item.session_id != session.session_id)
            .try_fold(bytes.len() + 1, |sum, item| {
                serde_json::to_vec_pretty(item).map(|bytes| sum + bytes.len() + 1)
            })
            .map_err(|_| error(503, "projection_edit_storage_failed"))?;
        if bytes.len() + 1 > MAX_SESSION_BYTES
            || total > MAX_STORE_BYTES
            || (!self.sessions.contains_key(&session.session_id)
                && self.sessions.len() >= MAX_SESSIONS)
        {
            return Err(error(413, "projection_edit_store_full"));
        }
        let response = session.response();
        if crate::write_json_atomically(
            &self.path.join(format!("{}.json", &session.session_id[5..])),
            &session,
        )
        .is_err()
        {
            // An error after replacement may have committed. Reopen before trusting memory.
            self.failed = true;
            return Err(error(503, "projection_edit_storage_failed"));
        }
        self.sessions.insert(session.session_id.clone(), session);
        Ok(response)
    }
    pub(super) fn prune(&mut self, active: &std::collections::BTreeSet<String>) -> EditResult<()> {
        self.prune_bindings(active, false)
    }
    pub(crate) fn prune_offline(
        &mut self,
        active: &std::collections::BTreeSet<String>,
    ) -> EditResult<()> {
        self.prune_bindings(active, true)
    }
    fn prune_bindings(
        &mut self,
        active: &std::collections::BTreeSet<String>,
        offline: bool,
    ) -> EditResult<()> {
        self.available()?;
        let stale: Vec<_> = self
            .sessions
            .values()
            .filter(|session| {
                session.bindings.iter().any(|id| {
                    session.offline_bindings.contains(id) == offline && !active.contains(id)
                })
            })
            .cloned()
            .collect();
        for mut session in stale {
            session.bindings.retain(|id| {
                session.offline_bindings.contains(id) != offline || active.contains(id)
            });
            session
                .offline_bindings
                .retain(|id| session.bindings.contains(id));
            if !session.bindings.is_empty() {
                self.commit(session)?;
                continue;
            }
            match fs::remove_file(self.path.join(format!("{}.json", &session.session_id[5..]))) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(_) => return Err(error(503, "projection_edit_storage_failed")),
            }
            self.sessions.remove(&session.session_id);
        }
        Ok(())
    }
}
