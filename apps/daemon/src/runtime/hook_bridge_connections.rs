// Owns Hook WebSocket connection threads so bridge stop/restart is deterministic.
#[derive(Clone)]
struct HookBridgeConnections {
    cancelled: Arc<AtomicBool>,
    workers: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl HookBridgeConnections {
    fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(true)),
            workers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn prepare_start(&self) {
        self.cancel_and_join();
        self.cancelled.store(false, Ordering::SeqCst);
    }

    fn cancellation(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }

    fn track(&self, worker: JoinHandle<()>) {
        match self.workers.lock() {
            Ok(mut workers) => workers.push(worker),
            Err(poisoned) => poisoned.into_inner().push(worker),
        }
    }

    fn reap_finished(&self) {
        let finished = {
            let mut workers = match self.workers.lock() {
                Ok(workers) => workers,
                Err(poisoned) => poisoned.into_inner(),
            };
            let mut finished = Vec::new();
            let mut index = 0;
            while index < workers.len() {
                if workers[index].is_finished() {
                    finished.push(workers.swap_remove(index));
                } else {
                    index += 1;
                }
            }
            finished
        };
        for worker in finished {
            let _ = worker.join();
        }
    }

    fn cancel_and_join(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        let workers = {
            let mut workers = match self.workers.lock() {
                Ok(workers) => workers,
                Err(poisoned) => poisoned.into_inner(),
            };
            std::mem::take(&mut *workers)
        };
        for worker in workers {
            let _ = worker.join();
        }
    }
}

fn decrement_client_count(counter: &AtomicUsize) {
    let _ = counter.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
        value.checked_sub(1)
    });
}
