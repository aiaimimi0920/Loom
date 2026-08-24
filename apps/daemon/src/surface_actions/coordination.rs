// Concurrency reservations, cancellation, and serial execution coordination.
fn action_key(instance_id: &str, action_id: &str) -> String {
    format!("{instance_id}:{action_id}")
}

fn coordinator_state(
    coordinator: &Arc<Mutex<SurfaceActionCoordinator>>,
) -> std::sync::MutexGuard<'_, SurfaceActionCoordinator> {
    // Coordinator operations only mutate standard collections and atomics. Recovering a poisoned
    // guard keeps cancellation and reservation cleanup available after a worker panic instead of
    // silently disabling Serial execution or leaking a RejectWhileRunning reservation.
    coordinator
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn is_recoverable_action_status(status: &SurfaceActionStatus) -> bool {
    matches!(
        status,
        SurfaceActionStatus::Queued
            | SurfaceActionStatus::Running
            | SurfaceActionStatus::Interrupted
            | SurfaceActionStatus::CancelRequested
    )
}

/// Returns the ack a submit should hand straight back, if there is one.
///
/// Submitting an event is idempotent: an event that already has an ack does not run a second time.
/// Recovery is the exception — it deliberately re-submits the acks that never reached a terminal
/// state, which is what the four statuses below are.
///
/// A submit calls this twice, once on each side of the package resolve, because a concurrent submit
/// of the same event may have been accepted while this one held no lock.
fn settled_ack(previous: Option<&SurfaceActionAck>, recovering: bool) -> Option<SurfaceActionAck> {
    let existing = previous?;
    if recovering && is_recoverable_action_status(&existing.status) {
        return None;
    }
    Some(existing.clone())
}

/// Whether two readings of one instance's descriptor still name the same locked Art package.
///
/// Only the three fields that pin package content are compared. The rest of the descriptor carries
/// counters — `generation`, `surface_revision`, `preview_revision`, `result_revision` — that move on
/// ordinary traffic such as a snapshot, so comparing whole descriptors would report a package change
/// on nearly every concurrent event and turn the retry into a spin.
fn same_locked_package(
    left: &loom_protocol::SurfaceInstanceDescriptor,
    right: &loom_protocol::SurfaceInstanceDescriptor,
) -> bool {
    left.art_id == right.art_id
        && left.art_version == right.art_version
        && left.package_digest == right.package_digest
}

fn reserve_action(
    coordinator: &Arc<Mutex<SurfaceActionCoordinator>>,
    instance_id: &str,
    action: &SurfaceActionDefinition,
    event: &SurfaceEvent,
) -> Result<Arc<AtomicBool>, SurfaceStoreError> {
    let key = action_key(instance_id, &action.id);
    let request_id = request_id_for_event(&event.event_id);
    let mut state = coordinator_state(coordinator);
    match &action.concurrency {
        SurfaceActionConcurrency::ReplaceLatest | SurfaceActionConcurrency::Coalesce => {
            if let Some(previous) = state.latest_requests.insert(key, request_id.clone()) {
                if let Some(token) = state.cancellation_tokens.get(&previous) {
                    token.store(true, Ordering::Release);
                }
            }
        }
        SurfaceActionConcurrency::RejectWhileRunning => {
            if !state.reject_reservations.insert(key) {
                return Err(SurfaceStoreError::Conflict(format!(
                    "Surface action {} is already running",
                    action.id
                )));
            }
        }
        SurfaceActionConcurrency::Serial | SurfaceActionConcurrency::Parallel => {}
    }
    let cancellation = Arc::new(AtomicBool::new(false));
    state
        .cancellation_tokens
        .insert(request_id, Arc::clone(&cancellation));
    Ok(cancellation)
}

fn release_reservation(
    coordinator: &Arc<Mutex<SurfaceActionCoordinator>>,
    instance_id: &str,
    action: &SurfaceActionDefinition,
    request_id: Option<&str>,
) {
    let mut state = coordinator_state(coordinator);
    let key = action_key(instance_id, &action.id);
    if action.concurrency == SurfaceActionConcurrency::RejectWhileRunning {
        state.reject_reservations.remove(&key);
    }
    if matches!(
        &action.concurrency,
        SurfaceActionConcurrency::ReplaceLatest | SurfaceActionConcurrency::Coalesce
    ) && request_id.is_some_and(|request_id| {
        state
            .latest_requests
            .get(&key)
            .is_some_and(|latest| latest == request_id)
    }) {
        state.latest_requests.remove(&key);
    }
    if let Some(request_id) = request_id {
        state.cancellation_tokens.remove(request_id);
    }
}

fn serial_lock(
    coordinator: &Arc<Mutex<SurfaceActionCoordinator>>,
    job: &SurfaceActionJob,
) -> Option<Arc<Mutex<()>>> {
    if job.action.concurrency != SurfaceActionConcurrency::Serial {
        return None;
    }
    let mut state = coordinator_state(coordinator);
    state
        .serial_locks
        .retain(|_, serial_lock| serial_lock.strong_count() > 0);
    let key = action_key(&job.event.instance_id, &job.action.id);
    if let Some(serial_lock) = state.serial_locks.get(&key).and_then(Weak::upgrade) {
        return Some(serial_lock);
    }
    let serial_lock = Arc::new(Mutex::new(()));
    state.serial_locks.insert(key, Arc::downgrade(&serial_lock));
    Some(serial_lock)
}

fn acquire_serial_guard(
    serial_lock: Option<&Arc<Mutex<()>>>,
) -> Option<std::sync::MutexGuard<'_, ()>> {
    serial_lock.map(|serial_lock| {
        serial_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    })
}

fn is_latest(coordinator: &Arc<Mutex<SurfaceActionCoordinator>>, job: &SurfaceActionJob) -> bool {
    if !matches!(
        &job.action.concurrency,
        SurfaceActionConcurrency::ReplaceLatest | SurfaceActionConcurrency::Coalesce
    ) {
        return true;
    }
    coordinator_state(coordinator)
        .latest_requests
        .get(&action_key(&job.event.instance_id, &job.action.id))
        .cloned()
        .is_some_and(|latest| latest == job.ack.request_id)
}
