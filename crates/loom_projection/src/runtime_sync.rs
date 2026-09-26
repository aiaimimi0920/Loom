use super::*;
use crate::{PullRequest, SnapshotGrant, SnapshotProvider, Status, TransferPath};
use async_trait::async_trait;

impl ProjectionRuntime {
    pub(super) async fn synchronize(&self) -> Result<()> {
        self.ensure_active()?;
        let result = self
            .client
            .execute(&CentralOperation::Sync {
                endpoint: self.transport.endpoint_address(),
            })
            .await?;
        self.ensure_active()?;
        let CentralResponse::Sync { views } = result else {
            return Err(error(502, "projection_response_invalid"));
        };
        let mut views: std::collections::BTreeMap<_, _> = views
            .into_iter()
            .map(|view| (view.record.envelope.projection_id.clone(), view))
            .collect();
        let ids: Vec<_> = self.store.lock().await.entries.keys().cloned().collect();
        for id in ids {
            let _gate = self.gate.lock().await;
            self.ensure_active()?;
            let entry = {
                let mut store = self.store.lock().await;
                let entry = store.entries.get_mut(&id).expect("stored projection");
                if let Some(view) = views.remove(&id) {
                    entry.merge_view(view)?;
                }
                entry.clone()
            };
            let result = self.refresh_entry(entry).await;
            if let Err(failure) = result {
                if failure.status == 401 {
                    return Err(failure);
                }
                if let Some(entry) = self.store.lock().await.entries.get_mut(&id) {
                    entry.last_error = Some(failure.code.to_owned());
                    entry.transfer = None;
                }
            }
        }
        Ok(())
    }

    async fn refresh_entry(&self, mut entry: crate::runtime_store::Entry) -> Result<()> {
        if entry
            .view
            .as_ref()
            .is_some_and(|v| v.record.status == Status::Stopped)
        {
            if !entry.stopped {
                entry.stopped = true;
                entry.pending = None;
                entry.pending_snapshot = None;
                self.cancel_transfer(entry.id());
                self.store.lock().await.save(entry).await?;
            }
            return Ok(());
        }
        entry = self.flush(entry).await?;
        let Some(view) = &entry.view else {
            return Ok(());
        };
        if entry.stopped || entry.source {
            return Ok(());
        }
        if !view.available || view.authorized_until_ms <= now_ms() {
            return Err(error(409, "projection_peer_unavailable"));
        }
        if view.record.revision < entry.snapshot.revision
            || (view.record.revision == entry.snapshot.revision
                && view.record.digest != entry.snapshot.digest)
        {
            return Err(error(409, "projection_revision_conflict"));
        }
        if view.record.revision > entry.snapshot.revision {
            let (snapshot, transfer) = self.fetch(view, false).await?;
            entry.snapshot = snapshot;
            entry.transfer = Some(transfer);
            entry.last_error = None;
            self.ensure_active()?;
            self.store.lock().await.save(entry).await?;
        } else if let Some(stored) = self.store.lock().await.entries.get_mut(entry.id()) {
            stored.last_error = None;
        }
        Ok(())
    }

    pub(super) async fn fetch(
        &self,
        view: &View,
        preview: bool,
    ) -> Result<(Snapshot, TransferPath)> {
        if !view.available || view.authorized_until_ms <= now_ms() {
            return Err(error(409, "projection_peer_unavailable"));
        }
        let peer = view
            .peer
            .as_ref()
            .ok_or(error(409, "projection_peer_unavailable"))?;
        let request = PullRequest {
            projection_id: view.record.envelope.projection_id.clone(),
            device_id: self.identity().session().device_id.clone(),
            envelope: preview.then(|| view.record.envelope.clone()),
        };
        let remaining = view
            .authorized_until_ms
            .saturating_sub(now_ms())
            .min(15_000);
        let (snapshot, transfer) = tokio::time::timeout(
            Duration::from_millis(remaining),
            self.transport.pull_snapshot(peer, &request),
        )
        .await
        .map_err(|_| error(504, "projection_peer_unavailable"))??;
        self.ensure_active()?;
        if view.authorized_until_ms <= now_ms()
            || snapshot.source_session_id != view.record.envelope.source.session_id
            || snapshot.revision != view.record.revision
            || snapshot.digest != view.record.digest
            || snapshot.width != view.record.width
            || snapshot.height != view.record.height
            || snapshot.png.len() != view.record.byte_length
        {
            return Err(error(409, "projection_content_changed"));
        }
        Ok((snapshot, transfer))
    }
}

#[async_trait]
impl SnapshotProvider for ProjectionRuntime {
    async fn snapshot(&self, public_key: &str, request: PullRequest) -> Result<SnapshotGrant> {
        self.ensure_active()?;
        {
            let store = self.store.lock().await;
            let entry = store
                .entries
                .get(&request.projection_id)
                .ok_or(error(404, "projection_not_found"))?;
            if !entry.source
                || entry.stopped
                || matches!(entry.pending, Some(CentralOperation::Create { .. }))
            {
                return Err(error(403, "projection_access_denied"));
            }
        }
        let result = self
            .client
            .execute(&CentralOperation::Peer {
                projection_id: request.projection_id.clone(),
                peer_device_id: request.device_id,
                peer_public_key: public_key.to_owned(),
                envelope: request.envelope,
            })
            .await?;
        self.ensure_active()?;
        let CentralResponse::Peer {
            peer,
            authorized_until_ms,
        } = result
        else {
            return Err(error(502, "projection_response_invalid"));
        };
        if peer.public_key != public_key || authorized_until_ms <= now_ms() {
            return Err(error(403, "projection_access_denied"));
        }
        let store = self.store.lock().await;
        let entry = store
            .entries
            .get(&request.projection_id)
            .ok_or(error(404, "projection_not_found"))?;
        if entry.stopped {
            return Err(error(410, "projection_unlinked"));
        }
        let cancelled = self.transfer_cancellation(entry.id())?;
        Ok(SnapshotGrant {
            snapshot: entry.snapshot.clone(),
            expires_at_ms: authorized_until_ms,
            cancelled,
        })
    }

    async fn transferred(&self, projection_id: &str, path: TransferPath) {
        if let Some(entry) = self.store.lock().await.entries.get_mut(projection_id) {
            entry.transfer = Some(path);
            entry.last_error = None;
        }
    }
}
