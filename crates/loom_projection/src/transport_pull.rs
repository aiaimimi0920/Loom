//! Pull streams keep preview and accepted transfers behind the same TLS peer check.
use super::*;
use async_trait::async_trait;
use tokio::task::JoinSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PullRequest {
    pub projection_id: String,
    pub device_id: String,
    pub envelope: Option<crate::Envelope>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransferPath {
    Direct,
    Relay,
}

pub struct SnapshotGrant {
    pub snapshot: Snapshot,
    pub expires_at_ms: u64,
    pub cancelled: tokio::sync::watch::Receiver<bool>,
}

#[async_trait]
pub trait SnapshotProvider: Send + Sync {
    // The key comes from QUIC TLS, never from the untrusted request body.
    async fn snapshot(&self, public_key: &str, request: PullRequest) -> Result<SnapshotGrant>;
    async fn transferred(&self, _projection_id: &str, _path: TransferPath) {}
}

struct ConnectionGuard(iroh::endpoint::Connection);
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.close(0u32.into(), b"projection complete");
    }
}

impl Transport {
    pub async fn pull_snapshot(
        &self,
        peer: &Peer,
        request: &PullRequest,
    ) -> Result<(Snapshot, TransferPath)> {
        let _gate = self.send_gate.lock().await;
        tokio::time::timeout(Duration::from_secs(15), async {
            let connection = self
                .endpoint
                .connect(endpoint_address(peer)?, ALPN)
                .await
                .map_err(|_| error(503, "projection_transport_unavailable"))?;
            let guard = ConnectionGuard(connection);
            verify_remote(&guard.0, peer)?;
            let (mut send, mut recv) = guard
                .0
                .open_bi()
                .await
                .map_err(|_| error(503, "projection_transport_unavailable"))?;
            let bytes = serde_json::to_vec(request)
                .map_err(|_| error(400, "projection_invalid_request"))?;
            if bytes.len() > MAX_HEADER_BYTES {
                return Err(error(413, "projection_request_budget"));
            }
            send.write_all(&bytes)
                .await
                .map_err(|_| error(503, "projection_transport_unavailable"))?;
            send.finish()
                .map_err(|_| error(503, "projection_transport_unavailable"))?;
            let snapshot = read_frame(&mut recv).await?;
            let snapshot = tokio::task::spawn_blocking(move || {
                snapshot.validate()?;
                Ok(snapshot)
            })
            .await
            .map_err(|_| error(503, "projection_transport_unavailable"))??;
            if snapshot.projection_id != request.projection_id {
                return Err(error(403, "projection_access_denied"));
            }
            let path = guard
                .0
                .paths()
                .iter()
                .find(|p| p.is_selected())
                .map(|p| {
                    if p.is_ip() {
                        TransferPath::Direct
                    } else {
                        TransferPath::Relay
                    }
                })
                .ok_or(error(503, "projection_transport_unavailable"))?;
            Ok((snapshot, path))
        })
        .await
        .map_err(|_| error(504, "projection_transport_unavailable"))?
    }

    pub async fn serve(&self, provider: Arc<dyn SnapshotProvider>) {
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! {
                incoming = self.endpoint.accept() => {
                    let Some(incoming) = incoming else { break };
                    if tasks.len() >= 4 { incoming.refuse(); continue; }
                    let provider = provider.clone();
                    tasks.spawn(async move {
                        let _ = tokio::time::timeout(Duration::from_secs(15), async {
                            let connection = incoming.await.map_err(|_| error(403, "projection_access_denied"))?;
                            let guard = ConnectionGuard(connection);
                            let key = STANDARD.encode(guard.0.remote_id().as_bytes());
                            let (mut send, mut recv) = guard.0.accept_bi().await
                                .map_err(|_| error(400, "projection_invalid_request"))?;
                            let bytes = recv.read_to_end(MAX_HEADER_BYTES).await
                                .map_err(|_| error(413, "projection_request_budget"))?;
                            let request: PullRequest = serde_json::from_slice(&bytes)
                                .map_err(|_| error(400, "projection_invalid_request"))?;
                            if !validation::projection_id(&request.projection_id)
                                || uuid::Uuid::parse_str(&request.device_id).is_err() {
                                return Err(error(400, "projection_invalid_request"));
                            }
                            let mut grant = provider.snapshot(&key, request).await?;
                            if *grant.cancelled.borrow() || grant.expires_at_ms <= crate::now_ms() {
                                return Err(error(403, "projection_access_denied"));
                            }
                            let lease = Duration::from_millis(grant.expires_at_ms.saturating_sub(crate::now_ms()));
                            tokio::select! {
                                _ = grant.cancelled.changed() => return Err(error(410, "projection_unlinked")),
                                result = tokio::time::timeout(lease, async {
                                    write_frame(&mut send, &grant.snapshot).await?;
                                    send.finish().map_err(|_| error(503, "projection_transport_unavailable"))?;
                                    if let Some(path) = guard.0.paths().iter().find(|p| p.is_selected()) {
                                        let path = if path.is_ip() { TransferPath::Direct } else { TransferPath::Relay };
                                        provider.transferred(&grant.snapshot.projection_id, path).await;
                                    }
                                    let _ = send.stopped().await;
                                    Ok::<(), crate::Error>(())
                                }) => { result.map_err(|_| error(403, "projection_access_denied"))??; }
                            }
                            Ok::<(), crate::Error>(())
                        }).await;
                    });
                }
                _ = tasks.join_next(), if !tasks.is_empty() => {}
            }
        }
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    }
}
