//! Core runtime identifiers, version metadata, and run lifecycle primitives shared by Loom crates.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Version of the Loom runtime crates.
pub const LOOM_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Result alias used by Loom crates.
pub type LoomResult<T> = Result<T, LoomError>;

/// Shared Loom identifier validation errors.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LoomError {
    #[error("invalid id `{value}`: {source}")]
    InvalidId { value: String, source: uuid::Error },
    #[error("invalid workflow id `{0}`")]
    InvalidWorkflowId(String),
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
    fn workflow_id_rejects_blank_strings() {
        assert_eq!(
            WorkflowId::try_new("   "),
            Err(LoomError::InvalidWorkflowId("   ".to_owned()))
        );
        let id = WorkflowId::try_new("image-blend-workflow").expect("valid workflow id");
        assert_eq!(id.as_str(), "image-blend-workflow");
    }
}
