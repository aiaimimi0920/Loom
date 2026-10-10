// Owns Hook WebSocket connection threads so bridge stop/restart is deterministic.
#[derive(Clone)]
struct HookBridgeConnections {
    cancelled: Arc<AtomicBool>,
    workers: Arc<Mutex<Vec<(JoinHandle<()>, TcpStream)>>>,
    #[cfg(test)]
    idle_timeout: Duration,
}

impl HookBridgeConnections {
    fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(true)),
            workers: Arc::new(Mutex::new(Vec::new())),
            #[cfg(test)]
            idle_timeout: HOOK_BRIDGE_IDLE_TIMEOUT,
        }
    }

    fn prepare_start(&self) {
        self.cancel_and_join();
        self.cancelled.store(false, Ordering::SeqCst);
    }

    fn cancellation(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }

    fn idle_timeout(&self) -> Duration {
        // Tests shorten one bridge instance only; production has no override.
        #[cfg(test)]
        {
            self.idle_timeout
        }
        #[cfg(not(test))]
        {
            HOOK_BRIDGE_IDLE_TIMEOUT
        }
    }

    fn track(&self, worker: JoinHandle<()>, interrupt: TcpStream) {
        match self.workers.lock() {
            Ok(mut workers) => workers.push((worker, interrupt)),
            Err(poisoned) => poisoned.into_inner().push((worker, interrupt)),
        }
    }

    fn at_capacity(&self) -> bool {
        // One accept owner checks and inserts; bound unauthenticated handshake workers too.
        self.workers
            .lock()
            .map(|workers| workers.len() >= 32)
            .unwrap_or(true)
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
                if workers[index].0.is_finished() {
                    finished.push(workers.swap_remove(index));
                } else {
                    index += 1;
                }
            }
            finished
        };
        for (worker, _) in finished {
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
        // Interrupt all sockets before joining any worker: per-read timeouts cannot
        // interrupt a peer that keeps an incomplete message alive with fragments.
        for (_, interrupt) in &workers {
            let _ = interrupt.shutdown(std::net::Shutdown::Both);
        }
        for (worker, _) in workers {
            let _ = worker.join();
        }
    }
}

fn decrement_client_count(counter: &AtomicUsize) {
    let _ = counter.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
        value.checked_sub(1)
    });
}
