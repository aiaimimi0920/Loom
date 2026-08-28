//! Durable crash-loop accounting for Capability Plugin runtimes.

use std::time::{SystemTime, UNIX_EPOCH};

use super::types::{
    CapabilityInstallError, CapabilityLifecycleStatus, CapabilityPluginRecord, CapabilityResult,
    CapabilityRuntimeFailureState,
};
use super::CapabilityPluginRegistry;

pub const CAPABILITY_FAILURE_WINDOW_MILLIS: u64 = 5 * 60 * 1000;
pub const CAPABILITY_MAX_RUNTIME_FAILURES: u32 = 5;
const BASE_RESTART_BACKOFF_MILLIS: u64 = 1_000;
const MAX_RESTART_BACKOFF_MILLIS: u64 = 60_000;

impl CapabilityPluginRegistry {
    pub fn record_runtime_failure(
        &self,
        qualified_id: &str,
    ) -> CapabilityResult<CapabilityPluginRecord> {
        self.record_runtime_failure_at(qualified_id, unix_time_millis())
    }

    pub fn clear_runtime_failures(
        &self,
        qualified_id: &str,
    ) -> CapabilityResult<CapabilityPluginRecord> {
        let current = self
            .get(qualified_id)?
            .ok_or_else(|| CapabilityInstallError::NotFound(qualified_id.to_owned()))?;
        if current.runtime_failures == CapabilityRuntimeFailureState::default() {
            return Ok(current);
        }
        self.mutate_record(qualified_id, |record| {
            record.runtime_failures = CapabilityRuntimeFailureState::default();
            Ok(())
        })
    }

    pub fn mark_runtime_faulted(
        &self,
        qualified_id: &str,
    ) -> CapabilityResult<CapabilityPluginRecord> {
        self.mutate_record(qualified_id, |record| {
            record.status = CapabilityLifecycleStatus::Faulted;
            Ok(())
        })
    }

    pub fn runtime_restart_allowed(&self, record: &CapabilityPluginRecord) -> bool {
        let state = &record.runtime_failures;
        state.count < CAPABILITY_MAX_RUNTIME_FAILURES
            && state
                .restart_not_before_ms
                .is_none_or(|deadline| deadline <= unix_time_millis())
    }

    pub(crate) fn record_runtime_failure_at(
        &self,
        qualified_id: &str,
        now_ms: u64,
    ) -> CapabilityResult<CapabilityPluginRecord> {
        self.mutate_record(qualified_id, |record| {
            let state = &mut record.runtime_failures;
            let outside_window = state.window_started_at_ms.is_none_or(|start| {
                now_ms.saturating_sub(start) >= CAPABILITY_FAILURE_WINDOW_MILLIS
            });
            if outside_window {
                *state = CapabilityRuntimeFailureState {
                    window_started_at_ms: Some(now_ms),
                    ..CapabilityRuntimeFailureState::default()
                };
            }
            state.count = state
                .count
                .saturating_add(1)
                .min(CAPABILITY_MAX_RUNTIME_FAILURES);
            state.last_failure_at_ms = Some(now_ms);
            let shift = state.count.saturating_sub(1).min(6);
            let backoff = BASE_RESTART_BACKOFF_MILLIS
                .saturating_mul(1u64 << shift)
                .min(MAX_RESTART_BACKOFF_MILLIS);
            state.restart_not_before_ms = Some(now_ms.saturating_add(backoff));
            if state.count >= CAPABILITY_MAX_RUNTIME_FAILURES {
                record.status = CapabilityLifecycleStatus::Faulted;
            }
            Ok(())
        })
    }
}

pub(super) fn validate_failure_state(record: &CapabilityPluginRecord) -> CapabilityResult<()> {
    let state = &record.runtime_failures;
    let timestamps_consistent = match (
        state.window_started_at_ms,
        state.last_failure_at_ms,
        state.restart_not_before_ms,
    ) {
        (None, None, None) => state.count == 0,
        (Some(window), Some(last), Some(restart)) => {
            state.count > 0 && window <= last && last <= restart
        }
        _ => false,
    };
    if state.count > CAPABILITY_MAX_RUNTIME_FAILURES || !timestamps_consistent {
        return Err(CapabilityInstallError::InvalidRegistry(format!(
            "invalid runtime failure state for {}",
            record.qualified_id
        )));
    }
    Ok(())
}

fn unix_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
