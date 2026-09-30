use super::*;
static PERSISTENT_HOST_GENERATION: AtomicU64 = AtomicU64::new(0);
static PERSISTENT_HOST_LIFECYCLE: OnceLock<(Mutex<PersistentHostLifecycleState>, Condvar)> =
    OnceLock::new();

#[derive(Default)]
struct PersistentHostLifecycleState {
    total: usize,
    generations: BTreeMap<u64, usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PersistentMcpHostInvalidationOutcome {
    pub closed_idle_hosts: usize,
    pub drained: bool,
}

pub(super) struct PersistentHostSlot {
    generation: u64,
}

fn persistent_host_lifecycle() -> &'static (Mutex<PersistentHostLifecycleState>, Condvar) {
    PERSISTENT_HOST_LIFECYCLE.get_or_init(|| {
        (
            Mutex::new(PersistentHostLifecycleState::default()),
            Condvar::new(),
        )
    })
}

pub(super) fn persistent_host_generation() -> u64 {
    PERSISTENT_HOST_GENERATION.load(Ordering::Acquire)
}

/// Advance the process-wide generation while the caller holds the idle-host pool lock.
///
/// Slot acquisition serializes on the lifecycle lock, so requests either join the old generation
/// and are drained or observe the new generation and reject stale resolved server state.
pub(super) fn advance_persistent_host_generation() -> u64 {
    let (lifecycle, _) = persistent_host_lifecycle();
    let _state = lifecycle
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    PERSISTENT_HOST_GENERATION.fetch_add(1, Ordering::AcqRel)
}

pub(super) fn wait_for_invalidated_persistent_hosts(
    invalidated_generation: u64,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    let (lifecycle, changed) = persistent_host_lifecycle();
    let mut state = lifecycle
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    loop {
        let old_hosts_remain = state
            .generations
            .range(..=invalidated_generation)
            .any(|(_, count)| *count > 0);
        if !old_hosts_remain {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        let (next_state, wait) = changed
            .wait_timeout(state, remaining)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state = next_state;
        if wait.timed_out() {
            return false;
        }
    }
}

impl PersistentHostSlot {
    pub(super) fn acquire(generation: u64) -> Result<Self, PersistentHostError> {
        let (lifecycle, _) = persistent_host_lifecycle();
        let mut state = lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if generation != PERSISTENT_HOST_GENERATION.load(Ordering::Acquire) {
            return Err(PersistentHostError::Invalidated);
        }
        if state.total >= MAX_PERSISTENT_MCP_HOSTS {
            return Err(PersistentHostError::PoolExhausted);
        }
        state.total += 1;
        *state.generations.entry(generation).or_default() += 1;
        Ok(Self { generation })
    }
}

impl Drop for PersistentHostSlot {
    fn drop(&mut self) {
        let (lifecycle, changed) = persistent_host_lifecycle();
        let mut state = lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        debug_assert!(state.total > 0, "persistent host count underflow");
        state.total = state.total.saturating_sub(1);
        let remove_generation = state
            .generations
            .get_mut(&self.generation)
            .is_some_and(|count| {
                *count = count.saturating_sub(1);
                *count == 0
            });
        if remove_generation {
            state.generations.remove(&self.generation);
        }
        changed.notify_all();
    }
}

#[cfg(test)]
pub(super) fn persistent_host_count() -> usize {
    persistent_host_lifecycle()
        .0
        .lock()
        .map(|state| state.total)
        .unwrap_or_else(|poisoned| poisoned.into_inner().total)
}
