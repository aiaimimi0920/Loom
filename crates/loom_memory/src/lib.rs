//! Memory and retrieval contracts for Loom.

use std::collections::BTreeMap;

use loom_core::{MessageId, RunId, SessionId};
use serde::{Deserialize, Serialize};

/// Version of the memory crate.
pub const LOOM_MEMORY_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Result alias for memory operations.
pub type MemoryResult<T> = Result<T, MemoryError>;

/// Memory contract errors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryError {
    EmptyContent,
    EmptyQuery,
}

impl std::fmt::Display for MemoryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyContent => formatter.write_str("memory content must not be empty"),
            Self::EmptyQuery => formatter.write_str("memory query must not be empty"),
        }
    }
}

impl std::error::Error for MemoryError {}

/// Stable memory record retained by Loom for later retrieval.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MemoryRecord {
    pub id: MessageId,
    pub session_id: SessionId,
    pub run_id: Option<RunId>,
    pub message_id: Option<MessageId>,
    pub content: String,
    pub tags: Vec<String>,
    pub metadata: BTreeMap<String, String>,
}

impl MemoryRecord {
    #[must_use]
    pub fn for_session(session_id: SessionId, content: impl Into<String>) -> Self {
        Self::new(session_id, None, None, content)
    }

    #[must_use]
    pub fn for_run(session_id: SessionId, run_id: RunId, content: impl Into<String>) -> Self {
        Self::new(session_id, Some(run_id), None, content)
    }

    #[must_use]
    pub fn for_message(
        session_id: SessionId,
        message_id: MessageId,
        content: impl Into<String>,
    ) -> Self {
        Self::new(session_id, None, Some(message_id), content)
    }

    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        let tag = tag.into();
        if !tag.trim().is_empty() && !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
        self
    }

    #[must_use]
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    fn new(
        session_id: SessionId,
        run_id: Option<RunId>,
        message_id: Option<MessageId>,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: MessageId::new(),
            session_id,
            run_id,
            message_id,
            content: content.into(),
            tags: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    fn validate(&self) -> MemoryResult<()> {
        if self.content.trim().is_empty() {
            return Err(MemoryError::EmptyContent);
        }
        Ok(())
    }

    fn matches_query(&self, query: &str) -> bool {
        let query = query.to_ascii_lowercase();
        self.content.to_ascii_lowercase().contains(&query)
            || self
                .tags
                .iter()
                .any(|tag| tag.to_ascii_lowercase().contains(&query))
            || self.metadata.iter().any(|(key, value)| {
                key.to_ascii_lowercase().contains(&query)
                    || value.to_ascii_lowercase().contains(&query)
            })
    }
}

/// Session-scoped memory search request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryQuery {
    pub session_id: SessionId,
    pub text: String,
}

impl MemoryQuery {
    #[must_use]
    pub fn new(session_id: SessionId, text: impl Into<String>) -> Self {
        Self {
            session_id,
            text: text.into(),
        }
    }

    fn validate(&self) -> MemoryResult<()> {
        if self.text.trim().is_empty() {
            return Err(MemoryError::EmptyQuery);
        }
        Ok(())
    }
}

/// Memory storage abstraction for Loom's retrieval layer.
pub trait MemoryStore {
    fn append(&mut self, record: MemoryRecord) -> MemoryResult<()>;
    fn records_for_session(&self, session_id: SessionId) -> MemoryResult<Vec<MemoryRecord>>;
    fn records_for_run(&self, run_id: RunId) -> MemoryResult<Vec<MemoryRecord>>;
    fn search(&self, query: MemoryQuery) -> MemoryResult<Vec<MemoryRecord>>;
}

/// In-memory memory store used by daemon/CLI smoke paths and tests.
#[derive(Clone, Debug, Default)]
pub struct InMemoryMemoryStore {
    records: Vec<MemoryRecord>,
}

impl MemoryStore for InMemoryMemoryStore {
    fn append(&mut self, record: MemoryRecord) -> MemoryResult<()> {
        record.validate()?;
        self.records.push(record);
        Ok(())
    }

    fn records_for_session(&self, session_id: SessionId) -> MemoryResult<Vec<MemoryRecord>> {
        Ok(self
            .records
            .iter()
            .filter(|record| record.session_id == session_id)
            .cloned()
            .collect())
    }

    fn records_for_run(&self, run_id: RunId) -> MemoryResult<Vec<MemoryRecord>> {
        Ok(self
            .records
            .iter()
            .filter(|record| record.run_id == Some(run_id))
            .cloned()
            .collect())
    }

    fn search(&self, query: MemoryQuery) -> MemoryResult<Vec<MemoryRecord>> {
        query.validate()?;
        Ok(self
            .records
            .iter()
            .filter(|record| record.session_id == query.session_id)
            .filter(|record| record.matches_query(&query.text))
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_core::{MessageId, RunId, SessionId};

    #[test]
    fn in_memory_store_records_and_queries_memories_by_session_and_run() {
        let mut store = InMemoryMemoryStore::default();
        let session_id = SessionId::new();
        let run_id = RunId::new();
        let record = MemoryRecord::for_run(session_id, run_id, "draft accepted")
            .with_tag("workflow")
            .with_metadata("node", "review");

        let record_id = record.id;
        store.append(record).expect("append memory");

        let session_memories = store
            .records_for_session(session_id)
            .expect("query session memories");
        assert_eq!(session_memories.len(), 1);
        assert_eq!(session_memories[0].id, record_id);
        assert_eq!(session_memories[0].content, "draft accepted");
        assert_eq!(session_memories[0].metadata["node"], "review");

        let run_memories = store.records_for_run(run_id).expect("query run memories");
        assert_eq!(run_memories.len(), 1);
        assert_eq!(run_memories[0].id, record_id);
    }

    #[test]
    fn search_matches_content_tags_and_metadata_without_cross_session_leakage() {
        let mut store = InMemoryMemoryStore::default();
        let session_id = SessionId::new();
        let other_session_id = SessionId::new();
        store
            .append(MemoryRecord::for_session(
                session_id,
                "gateway returned success",
            ))
            .expect("append first memory");
        store
            .append(MemoryRecord::for_session(
                session_id,
                "reviewer requested changes",
            ))
            .expect("append second memory");
        store
            .append(MemoryRecord::for_session(
                other_session_id,
                "gateway private token",
            ))
            .expect("append other session memory");
        store
            .append(
                MemoryRecord::for_message(session_id, MessageId::new(), "model response")
                    .with_tag("gateway")
                    .with_metadata("provider", "gemini"),
            )
            .expect("append tagged memory");

        let content_matches = store
            .search(MemoryQuery::new(session_id, "success"))
            .expect("search content");
        assert_eq!(content_matches.len(), 1);
        assert_eq!(content_matches[0].content, "gateway returned success");

        let tag_matches = store
            .search(MemoryQuery::new(session_id, "gateway"))
            .expect("search tag");
        assert_eq!(tag_matches.len(), 2);
        assert!(tag_matches
            .iter()
            .all(|record| record.session_id == session_id));

        let metadata_matches = store
            .search(MemoryQuery::new(session_id, "gemini"))
            .expect("search metadata");
        assert_eq!(metadata_matches.len(), 1);
        assert_eq!(metadata_matches[0].metadata["provider"], "gemini");
    }
}
