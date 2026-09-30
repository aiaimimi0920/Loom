use super::*;
use crate::runtime_store::Entry;
use crate::{Envelope, LocalImage, Status};

impl ProjectionRuntime {
    pub(super) async fn operate(&self, owner: &str, operation: LocalOperation) -> Result<Value> {
        match operation {
            LocalOperation::Context {} => Ok(self.context()),
            LocalOperation::Create {
                unit_id,
                content_kind,
                snapshot,
            } => self.create(owner, unit_id, content_kind, snapshot).await,
            LocalOperation::Inspect { envelope } => {
                let view = self
                    .central_view(&CentralOperation::Inspect {
                        envelope: envelope.clone(),
                    })
                    .await?;
                let (snapshot, transfer) = self.fetch(&view, true).await?;
                Ok(response(
                    &Entry {
                        owner: owner.to_owned(),
                        unit_id: String::new(),
                        source: false,
                        envelope,
                        snapshot,
                        pending_snapshot: None,
                        pending: None,
                        view: Some(view),
                        stopped: false,
                        transfer: Some(transfer),
                        last_error: None,
                    },
                    true,
                ))
            }
            LocalOperation::Accept {
                envelope,
                receiver_unit_id,
                expected_revision,
                expected_digest,
                confirmed,
            } => {
                self.accept(
                    owner,
                    envelope,
                    receiver_unit_id,
                    expected_revision,
                    expected_digest,
                    confirmed,
                )
                .await
            }
            LocalOperation::Read {
                projection_id,
                known_revision,
            } => {
                let store = self.store.lock().await;
                let entry = store.owned(&projection_id, owner)?;
                if entry.stopped {
                    let expired = entry.envelope.expires_at_ms <= now_ms()
                        && entry
                            .view
                            .as_ref()
                            .is_none_or(|v| v.record.receiver.is_none());
                    return Err(error(
                        410,
                        if expired {
                            "projection_invitation_expired"
                        } else {
                            "projection_unlinked"
                        },
                    ));
                }
                if known_revision > entry.snapshot.revision {
                    return Err(error(409, "projection_revision_conflict"));
                }
                Ok(response(
                    entry,
                    !entry.source && known_revision < entry.snapshot.revision,
                ))
            }
            LocalOperation::Update {
                projection_id,
                source_session_id,
                prior_revision,
                revision,
                snapshot,
            } => {
                let mut entry = self
                    .store
                    .lock()
                    .await
                    .owned(&projection_id, owner)?
                    .clone();
                if !entry.source || entry.envelope.source.session_id != source_session_id {
                    return Err(error(403, "projection_access_denied"));
                }
                if entry.stopped {
                    return Err(error(410, "projection_unlinked"));
                }
                entry = self.flush(entry).await?;
                if entry.stopped {
                    return Err(error(410, "projection_unlinked"));
                }
                let next = self
                    .decode(snapshot, entry.envelope.clone(), revision)
                    .await?;
                if next.digest == entry.snapshot.digest {
                    return Ok(response(&entry, false));
                }
                if entry.snapshot.revision != prior_revision {
                    return Err(error(409, "projection_revision_conflict"));
                }
                entry.pending = Some(CentralOperation::Publish {
                    projection_id,
                    source_session_id,
                    prior_revision,
                    revision,
                    digest: next.digest.clone(),
                    width: next.width,
                    height: next.height,
                    byte_length: next.png.len(),
                });
                entry.pending_snapshot = Some(next);
                self.store.lock().await.save(entry.clone()).await?;
                let entry = self.flush(entry).await?;
                Ok(response(&entry, false))
            }
            LocalOperation::Unlink { projection_id } => {
                let mut entry = self
                    .store
                    .lock()
                    .await
                    .owned(&projection_id, owner)?
                    .clone();
                entry.stopped = true;
                self.cancel_transfer(&projection_id);
                entry.pending_snapshot = None;
                entry.pending = Some(CentralOperation::Unlink { projection_id });
                // The local tombstone is durable before contacting an offline central service.
                self.store.lock().await.save(entry.clone()).await?;
                self.wake.notify_one();
                self.flush(entry).await?;
                Ok(json!({"unlinked":true}))
            }
        }
    }

    async fn create(
        &self,
        owner: &str,
        unit_id: String,
        kind: String,
        image: LocalImage,
    ) -> Result<Value> {
        let existing = self
            .store
            .lock()
            .await
            .entries
            .values()
            .find(|e| e.source && e.owner == owner && e.unit_id == unit_id && !e.stopped)
            .cloned();
        if let Some(entry) = existing {
            let entry = self.flush(entry).await?;
            if !entry.stopped {
                return Ok(response(&entry, false));
            }
        }
        let mut envelope =
            self.identity()
                .invitation(&unit_id, &kind, &"0".repeat(64), now_ms())?;
        let snapshot = self.decode(image, envelope.clone(), 1).await?;
        // Generate the invitation only after the formal PNG has been validated.
        let signed = self
            .identity()
            .invitation(&unit_id, &kind, &snapshot.digest, now_ms())?;
        envelope = signed;
        let snapshot = Snapshot {
            projection_id: envelope.projection_id.clone(),
            source_session_id: envelope.source.session_id.clone(),
            ..snapshot
        };
        let pending = CentralOperation::Create {
            envelope: envelope.clone(),
            width: snapshot.width,
            height: snapshot.height,
            byte_length: snapshot.png.len(),
        };
        let entry = Entry {
            owner: owner.to_owned(),
            unit_id,
            source: true,
            envelope,
            snapshot,
            pending_snapshot: None,
            pending: Some(pending),
            view: None,
            stopped: false,
            transfer: None,
            last_error: None,
        };
        self.ensure_active()?;
        self.store.lock().await.save(entry.clone()).await?;
        let entry = self.flush(entry).await?;
        self.wake.notify_one();
        Ok(response(&entry, false))
    }

    async fn accept(
        &self,
        owner: &str,
        envelope: Envelope,
        unit_id: String,
        revision: u64,
        digest: String,
        confirmed: bool,
    ) -> Result<Value> {
        envelope.validate(self.identity().origin())?;
        let existing = self
            .store
            .lock()
            .await
            .entries
            .get(&envelope.projection_id)
            .cloned();
        if let Some(entry) = existing {
            if entry.owner != owner
                || entry.unit_id != unit_id
                || entry.source
                || entry.envelope != envelope
            {
                return Err(error(403, "projection_access_denied"));
            }
            if entry.stopped {
                return Err(error(410, "projection_unlinked"));
            }
            return Ok(response(&self.flush(entry).await?, true));
        }
        let view = self
            .central_view(&CentralOperation::Inspect {
                envelope: envelope.clone(),
            })
            .await?;
        if view.record.revision != revision || view.record.digest != digest {
            return Err(error(409, "projection_content_changed"));
        }
        let (snapshot, transfer) = self.fetch(&view, true).await?;
        let pending = CentralOperation::Accept {
            envelope: envelope.clone(),
            receiver_unit_id: unit_id.clone(),
            expected_revision: revision,
            expected_digest: digest,
            confirmed,
        };
        let entry = Entry {
            owner: owner.to_owned(),
            unit_id,
            source: false,
            envelope,
            snapshot,
            pending_snapshot: None,
            pending: Some(pending),
            view: Some(view),
            stopped: false,
            transfer: Some(transfer),
            last_error: None,
        };
        self.ensure_active()?;
        self.store.lock().await.save(entry.clone()).await?;
        let entry = self.flush(entry).await?;
        self.wake.notify_one();
        Ok(response(&entry, true))
    }

    pub(super) async fn flush(&self, mut entry: Entry) -> Result<Entry> {
        if !entry.stopped
            && entry.source
            && entry.envelope.expires_at_ms <= now_ms()
            && entry
                .view
                .as_ref()
                .is_none_or(|v| v.record.status == Status::Invited)
        {
            // Re-read after expiry so an accept just before the deadline is never discarded.
            match self
                .central_view(&CentralOperation::Read {
                    projection_id: entry.id().to_owned(),
                })
                .await
            {
                Ok(view) => {
                    entry.merge_view(view)?;
                    if entry
                        .view
                        .as_ref()
                        .is_some_and(|v| v.record.status != Status::Linked)
                    {
                        entry.stopped = true;
                        entry.pending_snapshot = None;
                        entry.pending = Some(CentralOperation::Unlink {
                            projection_id: entry.id().to_owned(),
                        });
                    }
                }
                Err(failure) if failure.status == 404 => {
                    entry.stopped = true;
                    entry.pending_snapshot = None;
                    entry.pending = None;
                }
                Err(failure) => return Err(failure),
            }
            if entry.stopped {
                self.cancel_transfer(entry.id());
            }
            self.store.lock().await.save(entry.clone()).await?;
        }
        let Some(operation) = entry.pending.clone() else {
            return Ok(entry);
        };
        let view = if let CentralOperation::Publish {
            revision,
            digest,
            prior_revision,
            ..
        } = &operation
        {
            let current = self
                .central_view(&CentralOperation::Read {
                    projection_id: entry.id().to_owned(),
                })
                .await?;
            if current.record.status == Status::Stopped
                || (current.record.revision == *revision && current.record.digest == *digest)
            {
                current
            } else if current.record.revision == *prior_revision {
                self.central_view(&operation).await?
            } else {
                return Err(error(409, "projection_revision_conflict"));
            }
        } else if matches!(operation, CentralOperation::Create { .. }) {
            match self
                .central_view(&CentralOperation::Read {
                    projection_id: entry.id().to_owned(),
                })
                .await
            {
                Ok(view) if view.record.envelope == entry.envelope => view,
                Ok(_) => return Err(error(403, "projection_source_mismatch")),
                Err(failure) if failure.status == 404 => self.central_view(&operation).await?,
                Err(failure) => return Err(failure),
            }
        } else {
            match self.central_view(&operation).await {
                Ok(view) => view,
                Err(failure)
                    if failure.status == 404
                        && matches!(operation, CentralOperation::Unlink { .. }) =>
                {
                    entry.pending = None;
                    self.store.lock().await.save(entry.clone()).await?;
                    return Ok(entry);
                }
                Err(failure) => return Err(failure),
            }
        };
        if view.record.status == Status::Stopped {
            entry.pending_snapshot = None;
            self.cancel_transfer(entry.id());
        } else if let Some(snapshot) = entry.pending_snapshot.take() {
            entry.snapshot = snapshot;
        }
        entry.stopped |= view.record.status == Status::Stopped;
        entry.merge_view(view)?;
        entry.pending = None;
        entry.last_error = None;
        self.ensure_active()?;
        self.store.lock().await.save(entry.clone()).await?;
        Ok(entry)
    }
}

pub(super) fn response(entry: &Entry, include_image: bool) -> Value {
    let receiver = entry.view.as_ref().and_then(|v| v.record.receiver.as_ref());
    let online = entry
        .view
        .as_ref()
        .is_some_and(|v| v.available && v.authorized_until_ms > now_ms());
    let transport = if !online || entry.last_error.is_some() {
        "offline"
    } else {
        match entry.transfer {
            Some(crate::TransferPath::Direct) => "direct",
            Some(crate::TransferPath::Relay) => "relay",
            None => "reconnecting",
        }
    };
    json!({"envelope":entry.envelope, "revision":entry.snapshot.revision, "digest":entry.snapshot.digest,
        "linked":!entry.stopped, "receiverDeviceId":receiver.map(|r| &r.device_id), "receiverUnitId":receiver.map(|r| &r.unit_id),
        "snapshot":include_image.then(|| LocalImage::from(&entry.snapshot)), "transport":transport,
        "error":entry.last_error})
}
