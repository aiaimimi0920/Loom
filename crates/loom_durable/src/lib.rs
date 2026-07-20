//! Durable event and actor runtime contracts for Loom.

pub mod run_store;
pub use run_store::{
    InMemoryRunEvidenceStore, RunEventDraft, RunEvidenceStore, RunStoreError, RunStoreResult,
    RunStoreStatus, SqliteRunEvidenceStore,
};

use std::collections::HashMap;

use async_trait::async_trait;
use loom_core::{ActorId, LoomError, LoomEvent, LoomResult, RunId, SessionId};
use serde_json::Value;
use tokio::sync::RwLock;

/// Version of the durable runtime crate.
pub const LOOM_DURABLE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Durable event store contract.
#[async_trait]
pub trait EventStore: Send + Sync {
    async fn append(&self, event: LoomEvent) -> LoomResult<()>;
    async fn events_for_session(&self, session_id: SessionId) -> LoomResult<Vec<LoomEvent>>;
    async fn events_for_run(&self, run_id: RunId) -> LoomResult<Vec<LoomEvent>>;
}

/// In-memory event store used for deterministic runtime tests and local smoke runs.
#[derive(Debug, Default)]
pub struct InMemoryEventStore {
    events: RwLock<Vec<LoomEvent>>,
}

impl InMemoryEventStore {
    fn sort_events(events: &mut [LoomEvent]) {
        events.sort_by_key(|event| event.sequence);
    }
}

#[async_trait]
impl EventStore for InMemoryEventStore {
    async fn append(&self, event: LoomEvent) -> LoomResult<()> {
        self.events.write().await.push(event);
        Ok(())
    }

    async fn events_for_session(&self, session_id: SessionId) -> LoomResult<Vec<LoomEvent>> {
        let mut events: Vec<_> = self
            .events
            .read()
            .await
            .iter()
            .filter(|event| event.session_id == session_id)
            .cloned()
            .collect();
        Self::sort_events(&mut events);
        Ok(events)
    }

    async fn events_for_run(&self, run_id: RunId) -> LoomResult<Vec<LoomEvent>> {
        let mut events: Vec<_> = self
            .events
            .read()
            .await
            .iter()
            .filter(|event| event.run_id() == Some(run_id))
            .cloned()
            .collect();
        Self::sort_events(&mut events);
        Ok(events)
    }
}

/// Runtime state tracked for a registered actor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActorState {
    Idle,
    Busy,
    Terminated,
}

/// Message retained in an actor mailbox until an executor consumes it.
#[derive(Clone, Debug, PartialEq)]
pub struct ActorEnvelope {
    pub run_id: RunId,
    pub payload: Value,
}

impl ActorEnvelope {
    #[must_use]
    pub fn new(run_id: RunId, payload: Value) -> Self {
        Self { run_id, payload }
    }
}

#[derive(Clone, Debug)]
struct ActorSlot {
    state: ActorState,
    mailbox: Vec<ActorEnvelope>,
}

impl ActorSlot {
    fn new() -> Self {
        Self {
            state: ActorState::Idle,
            mailbox: Vec::new(),
        }
    }
}

/// In-memory actor mesh used by the durable runtime until a persistent mesh exists.
#[derive(Debug, Default)]
pub struct ActorMesh {
    actors: RwLock<HashMap<ActorId, ActorSlot>>,
}

impl ActorMesh {
    pub async fn register(&self, actor_id: ActorId) -> LoomResult<()> {
        let mut actors = self.actors.write().await;
        if actors.contains_key(&actor_id) {
            return Err(LoomError::ActorAlreadyRegistered(actor_id.to_string()));
        }

        actors.insert(actor_id, ActorSlot::new());
        Ok(())
    }

    pub async fn dispatch(&self, actor_id: &ActorId, envelope: ActorEnvelope) -> LoomResult<()> {
        let mut actors = self.actors.write().await;
        let actor = actors
            .get_mut(actor_id)
            .ok_or_else(|| LoomError::ActorNotFound(actor_id.to_string()))?;

        if actor.state == ActorState::Terminated {
            return Err(LoomError::ActorTerminated(actor_id.to_string()));
        }

        actor.mailbox.push(envelope);
        actor.state = ActorState::Busy;
        Ok(())
    }

    pub async fn terminate(&self, actor_id: &ActorId) -> LoomResult<()> {
        let mut actors = self.actors.write().await;
        let actor = actors
            .get_mut(actor_id)
            .ok_or_else(|| LoomError::ActorNotFound(actor_id.to_string()))?;
        actor.state = ActorState::Terminated;
        Ok(())
    }

    pub async fn actor_state(&self, actor_id: &ActorId) -> LoomResult<ActorState> {
        self.actors
            .read()
            .await
            .get(actor_id)
            .map(|actor| actor.state)
            .ok_or_else(|| LoomError::ActorNotFound(actor_id.to_string()))
    }

    pub async fn mailbox(&self, actor_id: &ActorId) -> LoomResult<Vec<ActorEnvelope>> {
        self.actors
            .read()
            .await
            .get(actor_id)
            .map(|actor| actor.mailbox.clone())
            .ok_or_else(|| LoomError::ActorNotFound(actor_id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_core::{ActorId, LoomEvent, RunId, RunStatus, SessionId, WorkflowId};

    #[tokio::test]
    async fn in_memory_store_appends_and_queries_events_in_sequence_order() {
        let store = InMemoryEventStore::default();
        let session_id = SessionId::new();
        let run_id = RunId::new();
        let workflow_id = WorkflowId::new("sample.workflow");

        let event_2 = LoomEvent::run_finished(session_id, run_id, RunStatus::Succeeded, 2);
        let event_1 = LoomEvent::run_started(session_id, run_id, workflow_id, 1);

        store.append(event_2.clone()).await.expect("append event 2");
        store.append(event_1.clone()).await.expect("append event 1");

        let by_session = store
            .events_for_session(session_id)
            .await
            .expect("query by session");
        assert_eq!(by_session, vec![event_1.clone(), event_2.clone()]);

        let by_run = store.events_for_run(run_id).await.expect("query by run");
        assert_eq!(by_run, vec![event_1, event_2]);
    }

    #[tokio::test]
    async fn in_memory_store_returns_empty_vectors_for_unknown_ids() {
        let store = InMemoryEventStore::default();

        assert!(store
            .events_for_session(SessionId::new())
            .await
            .expect("query unknown session")
            .is_empty());
        assert!(store
            .events_for_run(RunId::new())
            .await
            .expect("query unknown run")
            .is_empty());
    }

    #[tokio::test]
    async fn actor_mesh_registers_dispatches_and_tracks_state() {
        let mesh = ActorMesh::default();
        let actor_id = ActorId::new("planner");

        mesh.register(actor_id.clone())
            .await
            .expect("register actor");
        assert_eq!(
            mesh.actor_state(&actor_id).await.expect("actor state"),
            ActorState::Idle
        );

        mesh.dispatch(
            &actor_id,
            ActorEnvelope::new(RunId::new(), serde_json::json!({"task": "plan"})),
        )
        .await
        .expect("dispatch message");

        assert_eq!(
            mesh.actor_state(&actor_id).await.expect("actor state"),
            ActorState::Busy
        );
        assert_eq!(
            mesh.mailbox(&actor_id).await.expect("mailbox").len(),
            1,
            "dispatch should retain the envelope for later actor processing"
        );

        mesh.terminate(&actor_id).await.expect("terminate actor");
        assert_eq!(
            mesh.actor_state(&actor_id).await.expect("actor state"),
            ActorState::Terminated
        );
        assert!(mesh
            .dispatch(
                &actor_id,
                ActorEnvelope::new(RunId::new(), serde_json::json!({"task": "late"})),
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn actor_mesh_rejects_duplicate_and_missing_actor_dispatch() {
        let mesh = ActorMesh::default();
        let actor_id = ActorId::new("planner");

        mesh.register(actor_id.clone())
            .await
            .expect("register actor once");
        assert!(mesh.register(actor_id.clone()).await.is_err());
        assert!(mesh
            .dispatch(
                &ActorId::new("missing"),
                ActorEnvelope::new(RunId::new(), serde_json::json!({})),
            )
            .await
            .is_err());
    }
}
