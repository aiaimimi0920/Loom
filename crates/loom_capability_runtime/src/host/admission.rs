use std::collections::HashMap;
use std::sync::Mutex;

use super::{lock, CapabilityHostError, HostResult, RuntimeHostLimits};

#[derive(Default)]
pub(super) struct InvocationAdmission {
    total: usize,
    by_plugin: HashMap<String, usize>,
}

pub(super) struct InvocationPermit<'a> {
    state: &'a Mutex<InvocationAdmission>,
    plugin_id: String,
}

impl InvocationAdmission {
    pub(super) fn acquire<'a>(
        state: &'a Mutex<Self>,
        limits: &RuntimeHostLimits,
        plugin_id: &str,
    ) -> HostResult<InvocationPermit<'a>> {
        let mut admission = lock(state)?;
        let plugin_count = admission.by_plugin.get(plugin_id).copied().unwrap_or(0);
        if admission.total >= limits.max_global_inflight
            || plugin_count >= limits.max_plugin_inflight
        {
            return Err(CapabilityHostError::Busy);
        }
        admission.total = admission.total.saturating_add(1);
        admission
            .by_plugin
            .insert(plugin_id.to_owned(), plugin_count.saturating_add(1));
        Ok(InvocationPermit {
            state,
            plugin_id: plugin_id.to_owned(),
        })
    }
}

impl Drop for InvocationPermit<'_> {
    fn drop(&mut self) {
        let Ok(mut admission) = self.state.lock() else {
            return;
        };
        admission.total = admission.total.saturating_sub(1);
        let remove = if let Some(count) = admission.by_plugin.get_mut(&self.plugin_id) {
            *count = count.saturating_sub(1);
            *count == 0
        } else {
            false
        };
        if remove {
            admission.by_plugin.remove(&self.plugin_id);
        }
    }
}
