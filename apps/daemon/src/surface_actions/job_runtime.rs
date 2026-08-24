// Worker guard, action runner lifecycle, timeout, and thread reaping.
/// Owns the release of an action's reservation for as long as the job body runs, and reports a
/// panic in that body as a failure the caller can see.
///
/// `request_executor::worker_loop` catches the unwind from a worker handler, so a panic inside a
/// Surface action job neither crashes the daemon nor reaches any of the bookkeeping below the panic
/// site. Without this guard a `RejectWhileRunning` action stayed reserved for the rest of the
/// daemon's life — every later invocation of that `instance:action` pair answered "is already
/// running" — and the last persisted ack was the `Running` one, so Hook waited on a request that
/// could never resolve.
struct SurfaceActionJobGuard<'a> {
    job: &'a SurfaceActionJob,
    surface_instances: &'a SharedSurfaceInstanceStore,
    hook_bridge: &'a SharedHookBridgeRuntime,
    coordinator: &'a Arc<Mutex<SurfaceActionCoordinator>>,
    /// Set once the body has persisted a terminal ack of its own. A guard that drops while this is
    /// still false was dropped by an unwind.
    settled: bool,
}

impl<'a> SurfaceActionJobGuard<'a> {
    fn new(
        job: &'a SurfaceActionJob,
        surface_instances: &'a SharedSurfaceInstanceStore,
        hook_bridge: &'a SharedHookBridgeRuntime,
        coordinator: &'a Arc<Mutex<SurfaceActionCoordinator>>,
    ) -> Self {
        Self {
            job,
            surface_instances,
            hook_bridge,
            coordinator,
            settled: false,
        }
    }

    fn settle(&mut self) {
        self.settled = true;
    }
}

impl Drop for SurfaceActionJobGuard<'_> {
    fn drop(&mut self) {
        if !self.settled {
            eprintln!(
                "loom surface action {} panicked while handling request {}",
                self.job.action.id, self.job.ack.request_id
            );
            // Every store access on this path tolerates a poisoned mutex, because the panic may
            // well have happened while this thread was holding one. A destructor that panicked
            // during an unwind would abort the process.
            finish_failed(
                self.job,
                execution_error(
                    "surface_action_panicked",
                    "Surface action handling panicked before it reached a terminal state",
                ),
                self.surface_instances,
                self.hook_bridge,
            );
        }
        release_reservation(
            self.coordinator,
            &self.job.event.instance_id,
            &self.job.action,
            Some(&self.job.ack.request_id),
        );
    }
}

fn execute_surface_action_job(
    job: SurfaceActionJob,
    surface_instances: &SharedSurfaceInstanceStore,
    surface_resources: &SharedSurfaceResourceStore,
    hook_bridge: &SharedHookBridgeRuntime,
    coordinator: &Arc<Mutex<SurfaceActionCoordinator>>,
    runner: &Arc<SurfaceActionRunner>,
) {
    let serial = serial_lock(coordinator, &job);
    let _serial_guard = acquire_serial_guard(serial.as_ref());
    // Declared after the serial lock so it drops first: the reservation is released before the
    // lock that serializes the action, never the other way around.
    let mut guard = SurfaceActionJobGuard::new(&job, surface_instances, hook_bridge, coordinator);
    if !is_latest(coordinator, &job) || job.cancellation.load(Ordering::Acquire) {
        finish_cancelled(&job, surface_instances, hook_bridge);
        guard.settle();
        return;
    }

    let mut running = job.ack.clone();
    running.status = SurfaceActionStatus::Running;
    persist_ack(surface_instances, &running, false);
    broadcast_ack(hook_bridge, &running);
    broadcast_progress(hook_bridge, &job, Some(0.0), "running");

    let timeout_millis = job
        .action
        .timeout_ms
        .unwrap_or(DEFAULT_SURFACE_ACTION_TIMEOUT_MILLIS)
        .clamp(1, MAX_SURFACE_ACTION_TIMEOUT_MILLIS);
    let timeout = Duration::from_millis(timeout_millis);
    let deadline = Instant::now() + timeout;
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let runner_job = job.clone();
    let runner = Arc::clone(runner);
    let spawn = std::thread::Builder::new()
        .name(format!("loom-surface-runner-{}", job.ack.request_id))
        .spawn(move || {
            let _ = result_tx.send(runner(&runner_job));
        });
    let mut timed_out = false;
    let mut cancelled = false;
    let mut runner_thread = None;
    let result = match spawn {
        Err(error) => Err(execution_error(
            "surface_action_runner_failed",
            format!("Surface action runner could not start: {error}"),
        )),
        Ok(thread) => {
            runner_thread = Some(thread);
            loop {
                if job.cancellation.load(Ordering::Acquire) || !is_latest(coordinator, &job) {
                    cancelled = true;
                    break Err(execution_error(
                        "surface_action_cancelled",
                        "Surface action was cancelled",
                    ));
                }
                let now = Instant::now();
                if now >= deadline {
                    timed_out = true;
                    job.cancellation.store(true, Ordering::Release);
                    break Err(execution_error(
                        "surface_action_timeout",
                        format!("Surface action exceeded its {timeout_millis} ms budget"),
                    ));
                }
                let wait = deadline
                    .saturating_duration_since(now)
                    .min(Duration::from_millis(SURFACE_ACTION_POLL_MILLIS));
                match result_rx.recv_timeout(wait) {
                    Ok(result) => break result,
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => {
                        break Err(execution_error(
                            "surface_action_runner_failed",
                            "Surface action runner stopped without a result",
                        ))
                    }
                }
            }
        }
    };
    let abandoned_runner = cancelled || timed_out;
    // The runner can observe cancellation and return before this worker's polling loop
    // observes the token. Re-check it after receiving the result so that a cooperative
    // runner cannot race a late success commit over an accepted cancellation.
    if cancelled
        || (!timed_out && job.cancellation.load(Ordering::Acquire))
        || !is_latest(coordinator, &job)
    {
        finish_cancelled(&job, surface_instances, hook_bridge);
        guard.settle();
    } else {
        let result = result.and_then(parse_surface_action_response);

        match result.and_then(|response| {
            apply_action_response(
                &job,
                response,
                surface_instances,
                surface_resources,
                hook_bridge,
            )
        }) {
            Ok(()) => {
                let mut succeeded = job.ack.clone();
                succeeded.status = SurfaceActionStatus::Succeeded;
                persist_ack(surface_instances, &succeeded, true);
                broadcast_progress(hook_bridge, &job, Some(1.0), "succeeded");
                broadcast_ack(hook_bridge, &succeeded);
            }
            Err(error) => {
                finish_failed(&job, error, surface_instances, hook_bridge);
                if timed_out {
                    broadcast_progress(hook_bridge, &job, None, "timeout");
                }
            }
        }
        guard.settle();
    }
    // The ack is out, so Hook is no longer waiting. Reclaim the runner before `guard` releases the
    // reservation and `_serial_guard` releases the serial lock, so a `Serial` follow-up cannot
    // start while the runner this job spawned is still executing.
    reap_runner_thread(
        runner_thread,
        &result_rx,
        &job.ack.request_id,
        abandoned_runner,
        Duration::from_millis(SURFACE_ACTION_RUNNER_REAP_MILLIS),
    );
}

/// Waits for a spawned runner thread to finish, then joins it.
///
/// The join handle used to be dropped as soon as the polling loop broke, which detached the thread.
/// On a timeout or a cancellation that meant the worker released its `Serial` lock and its
/// reservation while the runner was still running, so the next invocation of the same action ran
/// concurrently with the abandoned one — the opposite of what `Serial` promises — and nothing ever
/// reported a runner that ignored its budget.
///
/// The runner sends its result immediately before returning, so the result channel doubles as the
/// "thread is about to finish" signal and the join that follows it does not block. `grace` bounds
/// the wait because a thread cannot be stopped from the outside; when it runs out the handle is
/// dropped and the thread is left to finish on its own, which is reported rather than hidden.
fn reap_runner_thread(
    thread: Option<std::thread::JoinHandle<()>>,
    result_rx: &mpsc::Receiver<Result<Value, SurfaceExecutionError>>,
    request_id: &str,
    abandoned: bool,
    grace: Duration,
) {
    let Some(thread) = thread else {
        return;
    };
    if abandoned {
        let deadline = Instant::now() + grace;
        loop {
            let now = Instant::now();
            if now >= deadline {
                eprintln!(
                    "loom surface action runner {} outlived its budget by more than {} ms and was abandoned",
                    request_id,
                    grace.as_millis()
                );
                return;
            }
            let wait = deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(SURFACE_ACTION_POLL_MILLIS));
            match result_rx.recv_timeout(wait) {
                // A late result is discarded: the terminal ack for this request has already been
                // decided and sent. All that matters here is that the thread is finishing.
                Ok(_) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => continue,
            }
        }
    }
    let _ = thread.join();
}
