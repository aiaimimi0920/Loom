// Owns the bounded durable latest snapshot, invitation consumption, and source/receiver bindings.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectionRecord {
    envelope: ProjectionEnvelope,
    snapshot: ProjectionSnapshot,
    revision: u64,
    digest: String,
    source_epoch: u64,
    receiver: Option<String>,
    receiver_epoch: Option<u64>,
    receiver_unit_id: Option<String>,
    unlinked: bool,
    updated_at_ms: u64,
    #[serde(default)]
    delivery: Option<ProjectionDelivery>,
}

struct ProjectionStore {
    path: PathBuf,
    records: BTreeMap<String, ProjectionRecord>,
    presence: BTreeMap<String, ProjectionPresence>,
}

const MAX_PROJECTIONS: usize = 64;
const MAX_PROJECTION_STORE_BYTES: usize = 48 * 1024 * 1024;

impl ProjectionStore {
    fn open(path: PathBuf) -> Result<Self> {
        use std::io::Read;
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            anyhow::ensure!(
                !metadata.file_type().is_symlink(),
                "projection storage cannot be a link"
            );
        }
        let entries = match fs::read_dir(&path) {
            Ok(entries) => Some(entries),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let mut records = BTreeMap::new();
        let mut total = 0;
        for (index, entry) in entries.into_iter().flatten().enumerate() {
            anyhow::ensure!(
                index < MAX_PROJECTIONS * 2,
                "too many projection storage entries"
            );
            let entry = entry?;
            if entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                != Some("json")
            {
                continue;
            }
            anyhow::ensure!(
                !entry.file_type()?.is_symlink() && entry.file_type()?.is_file(),
                "invalid projection storage entry"
            );
            let mut bytes = Vec::new();
            fs::File::open(entry.path())?
                .take((loom_protocol::projection::MAX_PROJECTION_HTTP_BYTES + 1) as u64)
                .read_to_end(&mut bytes)?;
            total += bytes.len();
            anyhow::ensure!(
                bytes.len() <= loom_protocol::projection::MAX_PROJECTION_HTTP_BYTES
                    && total <= MAX_PROJECTION_STORE_BYTES,
                "projection store exceeds its budget"
            );
            let record: ProjectionRecord = serde_json::from_slice(&bytes)?;
            let expected_name = format!(
                "{}.json",
                record
                    .envelope
                    .projection_id
                    .strip_prefix("projection:")
                    .unwrap_or_default()
            );
            anyhow::ensure!(
                entry.file_name().to_str() == Some(&expected_name)
                    && record.envelope.validate().is_ok()
                    && record.revision >= record.envelope.source.revision
                    && record.revision <= loom_protocol::projection::MAX_PROJECTION_REVISION
                    && record.receiver.is_some() == record.receiver_epoch.is_some()
                    && record.receiver.is_some() == record.receiver_unit_id.is_some()
                    && record
                        .receiver
                        .as_deref()
                        .is_none_or(loom_protocol::projection::projection_identifier_valid)
                    && record
                        .receiver_unit_id
                        .as_deref()
                        .is_none_or(loom_protocol::projection::projection_identifier_valid)
                    && projection_delivery_record_valid(&record)
                    && record.receiver.as_deref()
                        != Some(record.envelope.source.device_id.as_str()),
                "invalid stored projection"
            );
            validate_projection_snapshot(&record.snapshot, &record.digest)
                .map_err(|error| anyhow::anyhow!(error.code))?;
            records.insert(record.envelope.projection_id.clone(), record);
            anyhow::ensure!(
                records.len() <= MAX_PROJECTIONS,
                "too many stored projections"
            );
        }
        Ok(Self {
            path,
            records,
            presence: BTreeMap::new(),
        })
    }

    fn get(&self, id: &str) -> std::result::Result<&ProjectionRecord, ProjectionError> {
        self.records
            .get(id)
            .ok_or_else(|| ProjectionError::new(404, "projection_not_found"))
    }

    fn record_path(&self, id: &str) -> PathBuf {
        // IDs were validated as a fixed prefix plus exactly 32 hex digits.
        self.path
            .join(format!("{}.json", &id["projection:".len()..]))
    }

    fn commit(&mut self, record: ProjectionRecord) -> std::result::Result<(), ProjectionError> {
        let id = &record.envelope.projection_id;
        if !loom_protocol::projection::projection_id_valid(id) {
            return Err(ProjectionError::new(400, "projection_invalid"));
        }
        let total = self
            .records
            .values()
            .filter(|item| item.envelope.projection_id != *id)
            .map(|item| item.snapshot.image_base64.len() + 8192)
            .sum::<usize>()
            + record.snapshot.image_base64.len()
            + 8192;
        if (!self.records.contains_key(id) && self.records.len() >= MAX_PROJECTIONS)
            || total > MAX_PROJECTION_STORE_BYTES
        {
            return Err(ProjectionError::new(413, "projection_store_full"));
        }
        // Persist before publishing the mutation, including the single-use acceptance bit.
        write_json_atomically(&self.record_path(id), &record)
            .map_err(|_| ProjectionError::new(503, "projection_storage_failed"))?;
        self.records.insert(id.clone(), record);
        Ok(())
    }

    fn prune_expired(&mut self, now: u64) -> std::result::Result<(), ProjectionError> {
        let expired: Vec<_> = self
            .records
            .iter()
            .filter(|(_, record)| {
                (record.receiver.is_none() || record.unlinked)
                    && record.envelope.expires_at_ms <= now
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            match fs::remove_file(self.record_path(&id)) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(_) => return Err(ProjectionError::new(503, "projection_storage_failed")),
            }
            self.records.remove(&id);
        }
        Ok(())
    }

    fn create(
        &mut self,
        envelope: ProjectionEnvelope,
        snapshot: ProjectionSnapshot,
        source_epoch: u64,
        now: u64,
        delivery: Option<ProjectionDelivery>,
    ) -> std::result::Result<(), ProjectionError> {
        envelope
            .validate()
            .map_err(|_| ProjectionError::new(400, "projection_invalid"))?;
        if envelope.expires_at_ms <= now
            || envelope.expires_at_ms
                > now.saturating_add(loom_protocol::projection::PROJECTION_INVITE_TTL_MS)
        {
            return Err(ProjectionError::new(410, "projection_invitation_expired"));
        }
        self.prune_expired(now)?;
        if self.records.contains_key(&envelope.projection_id)
            || self
                .records
                .values()
                .any(|record| record.envelope.nonce == envelope.nonce)
        {
            return Err(ProjectionError::new(409, "projection_invitation_replayed"));
        }
        if self
            .records
            .values()
            .filter(|record| record.envelope.source.device_id == envelope.source.device_id)
            .count()
            >= 8
        {
            return Err(ProjectionError::new(429, "projection_source_limit"));
        }
        self.commit(ProjectionRecord {
            revision: envelope.source.revision,
            digest: envelope.content.digest.clone(),
            envelope,
            snapshot,
            source_epoch,
            receiver: None,
            receiver_epoch: None,
            receiver_unit_id: None,
            unlinked: false,
            updated_at_ms: now,
            delivery,
        })
    }

    fn invitation(
        &self,
        envelope: &ProjectionEnvelope,
        now: u64,
    ) -> std::result::Result<&ProjectionRecord, ProjectionError> {
        let record = self.get(&envelope.projection_id)?;
        if &record.envelope != envelope {
            return Err(ProjectionError::new(403, "projection_invitation_mismatch"));
        }
        if envelope.expires_at_ms <= now {
            return Err(ProjectionError::new(410, "projection_invitation_expired"));
        }
        if record.receiver.is_some() || record.unlinked {
            return Err(ProjectionError::new(409, "projection_invitation_consumed"));
        }
        Ok(record)
    }

    fn accept(
        &mut self,
        envelope: &ProjectionEnvelope,
        receiver: &str,
        epoch: u64,
        unit_id: &str,
        revision: u64,
        digest: &str,
        now: u64,
    ) -> std::result::Result<ProjectionRecord, ProjectionError> {
        let previous = self.get(&envelope.projection_id)?;
        projection_target_authorized(previous, receiver, epoch)?;
        // The same receiver can recover a lost acceptance response without consuming a second invitation.
        if &previous.envelope == envelope
            && previous.receiver.as_deref() == Some(receiver)
            && previous.receiver_epoch == Some(epoch)
            && previous.receiver_unit_id.as_deref() == Some(unit_id)
            && !previous.unlinked
        {
            return Ok(previous.clone());
        }
        let record = self.invitation(envelope, now)?;
        if receiver == envelope.source.device_id {
            return Err(ProjectionError::new(409, "projection_same_device"));
        }
        if record.revision != revision || record.digest != digest {
            return Err(ProjectionError::new(409, "projection_content_changed"));
        }
        let mut record = record.clone();
        record.receiver = Some(receiver.to_owned());
        record.receiver_epoch = Some(epoch);
        record.receiver_unit_id = Some(unit_id.to_owned());
        if let Some(delivery) = &mut record.delivery {
            delivery.status = DeliveryStatus::Accepted;
        }
        let result = record.clone();
        self.commit(record)?;
        Ok(result)
    }

    fn update(
        &mut self,
        id: &str,
        actor: &str,
        session_id: &str,
        prior: u64,
        revision: u64,
        digest: String,
        snapshot: ProjectionSnapshot,
        now: u64,
    ) -> std::result::Result<(), ProjectionError> {
        let record = self.get(id)?;
        if record.envelope.source.device_id != actor
            || record.envelope.source.session_id != session_id
        {
            return Err(ProjectionError::new(403, "projection_source_mismatch"));
        }
        if record.unlinked {
            return Err(ProjectionError::new(410, "projection_unlinked"));
        }
        if record.receiver.is_none() && record.envelope.expires_at_ms <= now {
            return Err(ProjectionError::new(410, "projection_invitation_expired"));
        }
        // A lost success response must not make an identical retry advance the revision again.
        if record.revision == revision
            && revision == prior.saturating_add(1)
            && record.digest == digest
        {
            return Ok(());
        }
        if record.revision != prior
            || revision != prior.saturating_add(1)
            || revision > loom_protocol::projection::MAX_PROJECTION_REVISION
        {
            return Err(ProjectionError::new(409, "projection_revision_conflict"));
        }
        if now.saturating_sub(record.updated_at_ms) < 500 {
            return Err(ProjectionError::new(429, "projection_update_rate"));
        }
        let mut record = record.clone();
        record.revision = revision;
        record.digest = digest;
        record.snapshot = snapshot;
        record.updated_at_ms = now;
        self.commit(record)
    }

    fn unlink(
        &mut self,
        id: &str,
        actor: &str,
        epoch: u64,
    ) -> std::result::Result<(), ProjectionError> {
        let record = self.get(id)?;
        let source = record.envelope.source.device_id == actor && record.source_epoch == epoch;
        let receiver =
            record.receiver.as_deref() == Some(actor) && record.receiver_epoch == Some(epoch);
        if !source && !receiver {
            return Err(ProjectionError::new(403, "projection_access_denied"));
        }
        if record.unlinked {
            return Ok(());
        }
        let mut record = record.clone();
        record.unlinked = true;
        self.commit(record)
    }
}
