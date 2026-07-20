//! Core primitives shared by Loom crates.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;
use uuid::Uuid;

/// Version of the Loom runtime crates.
pub const LOOM_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Result alias used by Loom crates.
pub type LoomResult<T> = Result<T, LoomError>;

/// Shared Loom runtime errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LoomError {
    #[error("invalid id `{value}`: {source}")]
    InvalidId { value: String, source: uuid::Error },
    #[error("invalid workflow id `{0}`")]
    InvalidWorkflowId(String),
    #[error("invalid actor id `{0}`")]
    InvalidActorId(String),
    #[error("actor `{0}` is already registered")]
    ActorAlreadyRegistered(String),
    #[error("actor `{0}` was not found")]
    ActorNotFound(String),
    #[error("actor `{0}` has terminated")]
    ActorTerminated(String),
}

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            #[must_use]
            pub fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = LoomError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(value)
                    .map(Self)
                    .map_err(|source| LoomError::InvalidId {
                        value: value.to_owned(),
                        source,
                    })
            }
        }
    };
}

uuid_id!(SessionId);
uuid_id!(RunId);
uuid_id!(MessageId);
uuid_id!(EventId);

/// Stable workflow identifier.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct WorkflowId(String);

impl WorkflowId {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        assert!(
            Self::is_valid(&value),
            "workflow id must contain at least one non-whitespace character"
        );
        Self(value)
    }

    #[must_use]
    pub fn try_new(value: impl Into<String>) -> LoomResult<Self> {
        let value = value.into();
        if Self::is_valid(&value) {
            Ok(Self(value))
        } else {
            Err(LoomError::InvalidWorkflowId(value))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn is_valid(value: &str) -> bool {
        !value.trim().is_empty()
    }
}

impl fmt::Display for WorkflowId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for WorkflowId {
    type Err = LoomError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_new(value)
    }
}

/// Stable actor identifier used by the durable actor mesh.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct ActorId(String);

impl ActorId {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        assert!(
            Self::is_valid(&value),
            "actor id must contain at least one non-whitespace character"
        );
        Self(value)
    }

    #[must_use]
    pub fn try_new(value: impl Into<String>) -> LoomResult<Self> {
        let value = value.into();
        if Self::is_valid(&value) {
            Ok(Self(value))
        } else {
            Err(LoomError::InvalidActorId(value))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn is_valid(value: &str) -> bool {
        !value.trim().is_empty()
    }
}

impl fmt::Display for ActorId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for ActorId {
    type Err = LoomError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_new(value)
    }
}

/// Chat/message role inside a Loom session.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Message DTO shared by CLI, daemon, workflows, and durable events.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LoomMessage {
    pub id: MessageId,
    pub session_id: SessionId,
    pub role: MessageRole,
    pub content: String,
    pub metadata: Map<String, Value>,
    pub created_at: DateTime<Utc>,
}

impl LoomMessage {
    #[must_use]
    pub fn new(session_id: SessionId, role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            id: MessageId::new(),
            session_id,
            role,
            content: content.into(),
            metadata: Map::new(),
            created_at: Utc::now(),
        }
    }

    #[must_use]
    pub fn user(session_id: SessionId, content: impl Into<String>) -> Self {
        Self::new(session_id, MessageRole::User, content)
    }

    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

/// Lifecycle state for a Loom run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

/// Durable run record tracked by the daemon and workflow executor.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RunRecord {
    pub id: RunId,
    pub session_id: SessionId,
    pub workflow_id: WorkflowId,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl RunRecord {
    #[must_use]
    pub fn new(session_id: SessionId, workflow_id: WorkflowId) -> Self {
        Self {
            id: RunId::new(),
            session_id,
            workflow_id,
            status: RunStatus::Pending,
            started_at: Utc::now(),
            finished_at: None,
        }
    }

    pub fn mark_running(&mut self) {
        self.status = RunStatus::Running;
    }

    pub fn mark_succeeded(&mut self) {
        self.status = RunStatus::Succeeded;
        self.finished_at = Some(Utc::now());
    }

    pub fn mark_failed(&mut self) {
        self.status = RunStatus::Failed;
        self.finished_at = Some(Utc::now());
    }
}

/// Serializable event envelope for durable Loom execution.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LoomEvent {
    pub id: EventId,
    pub session_id: SessionId,
    pub sequence: u64,
    pub occurred_at: DateTime<Utc>,
    pub kind: LoomEventKind,
}

impl LoomEvent {
    #[must_use]
    pub fn new(session_id: SessionId, sequence: u64, kind: LoomEventKind) -> Self {
        Self {
            id: EventId::new(),
            session_id,
            sequence,
            occurred_at: Utc::now(),
            kind,
        }
    }

    #[must_use]
    pub fn run_started(
        session_id: SessionId,
        run_id: RunId,
        workflow_id: WorkflowId,
        sequence: u64,
    ) -> Self {
        Self::new(
            session_id,
            sequence,
            LoomEventKind::RunStarted {
                run_id,
                workflow_id,
            },
        )
    }

    #[must_use]
    pub fn run_finished(
        session_id: SessionId,
        run_id: RunId,
        status: RunStatus,
        sequence: u64,
    ) -> Self {
        Self::new(
            session_id,
            sequence,
            LoomEventKind::RunFinished { run_id, status },
        )
    }

    #[must_use]
    pub fn message_recorded(session_id: SessionId, message: LoomMessage, sequence: u64) -> Self {
        Self::new(
            session_id,
            sequence,
            LoomEventKind::MessageRecorded { message },
        )
    }

    #[must_use]
    pub fn actor_message(
        session_id: SessionId,
        run_id: RunId,
        actor_id: ActorId,
        payload: Value,
        sequence: u64,
    ) -> Self {
        Self::new(
            session_id,
            sequence,
            LoomEventKind::ActorMessage {
                run_id,
                actor_id,
                payload,
            },
        )
    }

    #[must_use]
    pub fn run_id(&self) -> Option<RunId> {
        match &self.kind {
            LoomEventKind::RunStarted { run_id, .. }
            | LoomEventKind::RunFinished { run_id, .. }
            | LoomEventKind::ActorMessage { run_id, .. } => Some(*run_id),
            LoomEventKind::MessageRecorded { .. } => None,
        }
    }
}

/// Event payload variants emitted by Loom runtime modules.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum LoomEventKind {
    RunStarted {
        run_id: RunId,
        workflow_id: WorkflowId,
    },
    RunFinished {
        run_id: RunId,
        status: RunStatus,
    },
    MessageRecorded {
        message: LoomMessage,
    },
    ActorMessage {
        run_id: RunId,
        actor_id: ActorId,
        payload: Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_through_json_and_strings() {
        let session_id = SessionId::new();
        let run_id = RunId::new();

        let session_json = serde_json::to_string(&session_id).expect("serialize session id");
        let decoded_session: SessionId =
            serde_json::from_str(&session_json).expect("deserialize session id");
        assert_eq!(decoded_session, session_id);

        let parsed_run: RunId = run_id.to_string().parse().expect("parse run id");
        assert_eq!(parsed_run, run_id);
    }

    #[test]
    fn messages_keep_session_role_content_and_metadata() {
        let session_id = SessionId::new();
        let message = LoomMessage::user(session_id, "build a three-node workflow")
            .with_metadata("source", serde_json::json!("unit-test"));

        assert_eq!(message.session_id, session_id);
        assert_eq!(message.role, MessageRole::User);
        assert_eq!(message.content, "build a three-node workflow");
        assert_eq!(message.metadata["source"], "unit-test");

        let encoded = serde_json::to_string(&message).expect("serialize message");
        let decoded: LoomMessage = serde_json::from_str(&encoded).expect("deserialize message");
        assert_eq!(decoded, message);
    }

    #[test]
    fn run_lifecycle_events_are_serializable_and_ordered() {
        let session_id = SessionId::new();
        let run_id = RunId::new();
        let workflow_id = WorkflowId::new("sample.workflow");

        let started = LoomEvent::run_started(session_id, run_id, workflow_id.clone(), 1);
        let finished = LoomEvent::run_finished(session_id, run_id, RunStatus::Succeeded, 2);

        assert_eq!(started.sequence, 1);
        assert_eq!(finished.sequence, 2);
        assert!(started.occurred_at <= finished.occurred_at);
        assert_eq!(started.run_id(), Some(run_id));
        assert_eq!(finished.run_id(), Some(run_id));

        let encoded = serde_json::to_string(&started).expect("serialize event");
        let decoded: LoomEvent = serde_json::from_str(&encoded).expect("deserialize event");
        assert_eq!(decoded, started);
    }

    #[test]
    fn run_records_state_transitions() {
        let session_id = SessionId::new();
        let workflow_id = WorkflowId::new("sample.workflow");
        let mut run = RunRecord::new(session_id, workflow_id);

        assert_eq!(run.status, RunStatus::Pending);
        run.mark_running();
        assert_eq!(run.status, RunStatus::Running);
        run.mark_succeeded();
        assert_eq!(run.status, RunStatus::Succeeded);
        assert!(run.finished_at.is_some());
    }
}
