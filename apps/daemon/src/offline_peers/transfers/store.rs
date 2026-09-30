use super::*;
const LIMIT: usize = 64;
pub(crate) struct Store {
    path: PathBuf,
    pub(super) records: BTreeMap<String, Record>,
}
impl Store {
    pub fn open(root: &Path) -> anyhow::Result<Self> {
        let path = root.join("offline-projections");
        fs::create_dir_all(&path)?;
        let mut store = Self {
            path,
            records: BTreeMap::new(),
        };
        for entry in fs::read_dir(&store.path)? {
            let entry = entry?;
            if entry.path().extension().and_then(|v| v.to_str()) != Some("json") {
                continue;
            }
            anyhow::ensure!(
                store.records.len() < LIMIT,
                "offline transfer store exceeds budget"
            );
            anyhow::ensure!(
                entry.file_type()?.is_file(),
                "offline transfer record must be a file"
            );
            let mut bytes = Vec::new();
            fs::File::open(entry.path())?
                .take(MAX_BODY as u64 + 1)
                .read_to_end(&mut bytes)?;
            anyhow::ensure!(
                bytes.len() <= MAX_BODY,
                "offline transfer record exceeds budget"
            );
            let record: Record = serde_json::from_slice(&bytes)?;
            record.validate().map_err(|e| anyhow::anyhow!(e.code))?;
            anyhow::ensure!(
                entry.path() == store.file(&record.envelope.projection_id),
                "offline transfer filename mismatch"
            );
            anyhow::ensure!(
                !store.records.contains_key(&record.envelope.projection_id),
                "duplicate offline transfer"
            );
            store
                .records
                .insert(record.envelope.projection_id.clone(), record);
        }
        Ok(store)
    }
    fn file(&self, id: &str) -> PathBuf {
        self.path
            .join(format!("{}.json", sha256_bytes(id.as_bytes())))
    }
    pub(super) fn get(&self, id: &str) -> PeerResult<Record> {
        self.records
            .get(id)
            .cloned()
            .ok_or_else(|| failure(404, "projection_not_found"))
    }
    pub(super) fn commit(&mut self, record: Record) -> PeerResult<()> {
        let id = &record.envelope.projection_id;
        if !self.records.contains_key(id) {
            let expired: Vec<_> = self
                .records
                .iter()
                .filter(|(_, r)| {
                    r.envelope.expires_at_ms <= unix_time_millis()
                        && (r.unlinked || r.receiver_unit.is_none())
                })
                .map(|(id, _)| id.clone())
                .collect();
            for old in expired {
                fs::remove_file(self.file(&old))
                    .map_err(|_| failure(503, "projection_storage_unavailable"))?;
                self.records.remove(&old);
            }
            if self.records.len() >= LIMIT {
                return Err(failure(429, "projection_store_full"));
            }
            if self
                .records
                .values()
                .filter(|r| {
                    r.peer_id == record.peer_id
                        && r.envelope.source.device_id == record.envelope.source.device_id
                        && r.incoming == record.incoming
                })
                .count()
                >= 8
            {
                return Err(failure(429, "projection_source_limit"));
            }
        }
        write_json_atomically(&self.file(id), &record)
            .map_err(|_| failure(503, "projection_storage_unavailable"))?;
        self.records.insert(id.clone(), record);
        Ok(())
    }
}
