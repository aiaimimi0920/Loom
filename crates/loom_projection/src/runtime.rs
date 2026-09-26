//! One account generation owns its endpoint, durable outbox and bounded background work.
use crate::runtime_store::Store;
use crate::{
    error, now_ms, CentralClient, CentralOperation, CentralResponse, Identity, LocalOperation,
    Policy, Result, Snapshot, Transport, View,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    sync::{Mutex, Notify},
    task::JoinHandle,
};

#[path = "runtime_operations.rs"]
mod operations;
#[path = "runtime_sync.rs"]
mod sync;

pub struct ProjectionRuntime {
    pub(super) client: CentralClient,
    pub(super) policy: Policy,
    pub(super) transport: Transport,
    pub(super) store: Mutex<Store>,
    pub(super) gate: Mutex<()>,
    active: AtomicBool,
    wake: Notify,
    tasks: Mutex<Vec<JoinHandle<()>>>,
    transfers:
        std::sync::Mutex<std::collections::BTreeMap<String, tokio::sync::watch::Sender<bool>>>,
}

impl ProjectionRuntime {
    pub async fn start(identity: Identity, root: PathBuf) -> Result<Arc<Self>> {
        let client = CentralClient::new(identity.clone())?;
        let policy = client.configuration().await?;
        let stored_identity = identity.clone();
        let store = tokio::task::spawn_blocking(move || Store::load(&root, &stored_identity))
            .await
            .map_err(|_| error(503, "projection_storage_failed"))??;
        let transport = Transport::bind(&identity, &policy.relay_urls).await?;
        let owner = Arc::new(Self {
            client,
            policy,
            transport,
            store: Mutex::new(store),
            gate: Mutex::new(()),
            active: AtomicBool::new(true),
            wake: Notify::new(),
            tasks: Mutex::new(Vec::new()),
            transfers: std::sync::Mutex::new(std::collections::BTreeMap::new()),
        });
        let serving = owner.clone();
        let serving_task =
            tokio::spawn(async move { serving.transport.serve(serving.clone()).await });
        let syncing = owner.clone();
        let sync_task = tokio::spawn(async move { syncing.run_sync().await });
        owner.tasks.lock().await.extend([serving_task, sync_task]);
        Ok(owner)
    }

    pub fn identity(&self) -> &Identity {
        self.client.identity()
    }
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
    pub(super) fn ensure_active(&self) -> Result<()> {
        if !self.is_active() || self.identity().session().expires_at_ms <= now_ms() {
            Err(error(401, "device_session_unavailable"))
        } else {
            Ok(())
        }
    }

    pub async fn close(&self) {
        self.active.store(false, Ordering::Release);
        self.wake.notify_waiters();
        self.transport.close().await;
        let tasks = std::mem::take(&mut *self.tasks.lock().await);
        for task in &tasks {
            task.abort();
        }
        for task in tasks {
            let _ = task.await;
        }
    }

    pub async fn execute(&self, owner: &str, operation: LocalOperation) -> Result<Value> {
        operation.validate()?;
        if !crate::validation::identifier(owner) {
            return Err(error(403, "projection_access_denied"));
        }
        self.ensure_active()?;
        if matches!(
            operation,
            LocalOperation::Read { .. } | LocalOperation::Context {}
        ) {
            return self.operate(owner, operation).await;
        }
        let _gate = self.gate.lock().await;
        self.ensure_active()?;
        let response = self.operate(owner, operation).await?;
        self.ensure_active()?;
        Ok(response)
    }

    pub(super) fn transfer_cancellation(
        &self,
        id: &str,
    ) -> Result<tokio::sync::watch::Receiver<bool>> {
        let mut transfers = self
            .transfers
            .lock()
            .map_err(|_| error(503, "projection_transport_unavailable"))?;
        if !transfers.contains_key(id) && transfers.len() >= 64 {
            return Err(error(429, "projection_storage_budget"));
        }
        Ok(transfers
            .entry(id.to_owned())
            .or_insert_with(|| tokio::sync::watch::channel(false).0)
            .subscribe())
    }

    pub(super) fn cancel_transfer(&self, id: &str) {
        if let Ok(transfers) = self.transfers.lock() {
            if let Some(cancel) = transfers.get(id) {
                cancel.send_replace(true);
            }
        }
    }

    pub(super) async fn central_view(&self, operation: &CentralOperation) -> Result<View> {
        self.ensure_active()?;
        let result = self.client.execute(operation).await;
        self.ensure_active()?;
        match result? {
            CentralResponse::Projection { view } => Ok(view),
            _ => Err(error(502, "projection_response_invalid")),
        }
    }

    async fn run_sync(&self) {
        let mut failures = 0u32;
        loop {
            if self.ensure_active().is_err() {
                break;
            }
            match self.synchronize().await {
                Ok(()) => failures = 0,
                Err(failure) => {
                    if failure.status == 401 {
                        break;
                    }
                    failures = (failures + 1).min(5);
                    for entry in self.store.lock().await.entries.values_mut() {
                        entry.last_error = Some(failure.code.to_owned());
                        entry.transfer = None;
                    }
                }
            }
            let delay = if failures == 0 {
                self.policy.sync_interval_ms
            } else {
                (1000u64 << failures).min(30_000)
            };
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(delay)) => {}
                _ = self.wake.notified() => {}
            }
        }
        self.active.store(false, Ordering::Release);
        self.transport.close().await;
    }

    pub(super) fn context(&self) -> Value {
        let session = self.identity().session();
        json!({"status":"signed_in", "protocol":session.protocol, "projectionProtocol":crate::PROTOCOL,
            "origin":self.identity().origin(), "accountId":session.account_id, "deviceId":session.device_id,
            "deviceName":session.device_name, "expiresAtMs":session.expires_at_ms, "policy":self.policy})
    }

    pub(super) async fn decode(
        &self,
        image: crate::LocalImage,
        envelope: crate::Envelope,
        revision: u64,
    ) -> Result<Snapshot> {
        tokio::task::spawn_blocking(move || image.snapshot(&envelope, revision))
            .await
            .map_err(|_| error(503, "projection_invalid_image"))?
    }
}
