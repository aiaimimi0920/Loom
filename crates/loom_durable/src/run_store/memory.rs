use std::collections::HashMap;

use serde_json::Value;

use super::{
    core::{
        event_value, interrupt_run, sequence_values, serialized_run_document, validate_event_batch,
        validate_event_draft, validated_run_identity,
    },
    RunEventDraft, RunEvidenceStore, RunStoreError, RunStoreResult, RunStoreStatus,
};

#[derive(Debug, Default)]
pub struct InMemoryRunEvidenceStore {
    runs: HashMap<String, Value>,
    events: HashMap<String, Vec<Value>>,
    next_sequence: u64,
}

impl RunEvidenceStore for InMemoryRunEvidenceStore {
    fn insert_run(&mut self, run: Value, events: Vec<RunEventDraft>) -> RunStoreResult<()> {
        let (run_id, _) = validated_run_identity(&run)?;
        let run_id = run_id.to_owned();
        serialized_run_document(&run)?;

        validate_event_batch(&events)?;
        if self.runs.contains_key(&run_id) {
            return Err(RunStoreError::DuplicateRun(run_id));
        }

        let sequences = sequence_values(self.next_sequence, events.len())?;
        let stored_events = events
            .into_iter()
            .zip(sequences)
            .map(|(event, sequence)| event_value(sequence, &run_id, &event.kind, event.fields))
            .collect::<Vec<_>>();

        if let Some(sequence) = stored_events
            .last()
            .and_then(|event| event["sequence"].as_u64())
        {
            self.next_sequence = sequence;
        }
        self.runs.insert(run_id.clone(), run);
        self.events.insert(run_id, stored_events);
        Ok(())
    }

    fn transition_run(&mut self, run: Value, event: RunEventDraft) -> RunStoreResult<()> {
        let (run_id, _) = validated_run_identity(&run)?;
        let run_id = run_id.to_owned();
        serialized_run_document(&run)?;
        validate_event_draft(&event)?;
        if !self.runs.contains_key(&run_id) {
            return Err(RunStoreError::RunNotFound(run_id));
        }

        let sequence = sequence_values(self.next_sequence, 1)?
            .into_iter()
            .next()
            .expect("one sequence value");
        let stored_event = event_value(sequence, &run_id, &event.kind, event.fields);

        self.next_sequence = sequence;
        self.runs.insert(run_id.clone(), run);
        self.events.entry(run_id).or_default().push(stored_event);
        Ok(())
    }

    fn get_run(&self, run_id: &str) -> RunStoreResult<Option<Value>> {
        Ok(self.runs.get(run_id).cloned())
    }

    fn get_events(&self, run_id: &str) -> RunStoreResult<Option<Vec<Value>>> {
        if !self.runs.contains_key(run_id) {
            return Ok(None);
        }
        Ok(Some(self.events.get(run_id).cloned().unwrap_or_default()))
    }

    fn recover_interrupted_runs(&mut self) -> RunStoreResult<usize> {
        let mut running_ids = Vec::new();
        for (key, run) in &self.runs {
            let (run_id, status) = validated_run_identity(run)?;
            if run_id != key {
                return Err(RunStoreError::Integrity(format!(
                    "run index key `{key}` does not match run id `{run_id}`"
                )));
            }
            if status == "running" {
                running_ids.push(key.clone());
            }
        }
        running_ids.sort();

        let mut recoveries = Vec::with_capacity(running_ids.len());
        for run_id in &running_ids {
            let mut run = self.runs.get(run_id).cloned().ok_or_else(|| {
                RunStoreError::Integrity("run disappeared during recovery".to_owned())
            })?;
            let event = interrupt_run(&mut run)?;
            serialized_run_document(&run)?;
            recoveries.push((run_id.clone(), run, event));
        }

        let sequences = sequence_values(self.next_sequence, recoveries.len())?;
        let stored_events = recoveries
            .iter()
            .zip(sequences)
            .map(|((run_id, _, event), sequence)| {
                event_value(sequence, run_id, &event.kind, event.fields.clone())
            })
            .collect::<Vec<_>>();

        if let Some(sequence) = stored_events
            .last()
            .and_then(|event| event["sequence"].as_u64())
        {
            self.next_sequence = sequence;
        }
        for ((run_id, run, _), event) in recoveries.into_iter().zip(stored_events) {
            self.runs.insert(run_id.clone(), run);
            self.events.entry(run_id).or_default().push(event);
        }
        Ok(running_ids.len())
    }

    fn status(&self) -> RunStoreStatus {
        RunStoreStatus {
            mode: "memory",
            persistent: false,
        }
    }
}
