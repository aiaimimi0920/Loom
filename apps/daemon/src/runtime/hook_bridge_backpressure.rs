// Queue limits apply to both item count and retained payload bytes. Overflow
// disconnects the slow consumer instead of blocking publishers or losing events silently.
const HOOK_BRIDGE_SUBSCRIPTION_CAPACITY: usize = 32;
const HOOK_BRIDGE_SUBSCRIPTION_BYTES: usize = 16 * 1024 * 1024;
const HOOK_BRIDGE_DRAIN_BATCH: usize = 16;

#[derive(Clone)]
struct HookBridgeBroadcastSender {
    tx: mpsc::SyncSender<String>,
    queued_bytes: Arc<AtomicUsize>,
    overflowed: Arc<AtomicBool>,
    byte_limit: usize,
}

struct HookBridgeBroadcastReceiver {
    rx: Receiver<String>,
    queued_bytes: Arc<AtomicUsize>,
    overflowed: Arc<AtomicBool>,
}

fn hook_bridge_broadcast_channel(
    capacity: usize,
    byte_limit: usize,
) -> (HookBridgeBroadcastSender, HookBridgeBroadcastReceiver) {
    let (tx, rx) = mpsc::sync_channel(capacity);
    let queued_bytes = Arc::new(AtomicUsize::new(0));
    let overflowed = Arc::new(AtomicBool::new(false));
    (
        HookBridgeBroadcastSender {
            tx,
            queued_bytes: Arc::clone(&queued_bytes),
            overflowed: Arc::clone(&overflowed),
            byte_limit,
        },
        HookBridgeBroadcastReceiver {
            rx,
            queued_bytes,
            overflowed,
        },
    )
}

impl HookBridgeBroadcastSender {
    fn try_send(&self, message: &str) -> bool {
        if self.overflowed.load(Ordering::SeqCst) {
            return false;
        }
        let reserved = self
            .queued_bytes
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |bytes| {
                bytes
                    .checked_add(message.len())
                    .filter(|next| *next <= self.byte_limit)
            })
            .is_ok();
        if !reserved {
            self.overflowed.store(true, Ordering::SeqCst);
            return false;
        }
        if self.tx.try_send(message.to_owned()).is_err() {
            self.queued_bytes.fetch_sub(message.len(), Ordering::SeqCst);
            self.overflowed.store(true, Ordering::SeqCst);
            return false;
        }
        true
    }
}

impl HookBridgeBroadcastReceiver {
    fn try_recv(&self) -> std::result::Result<String, mpsc::TryRecvError> {
        if self.overflowed.load(Ordering::SeqCst) {
            return Err(mpsc::TryRecvError::Disconnected);
        }
        let message = self.rx.try_recv()?;
        self.queued_bytes.fetch_sub(message.len(), Ordering::SeqCst);
        Ok(message)
    }

    #[cfg(test)]
    fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> std::result::Result<String, mpsc::RecvTimeoutError> {
        if self.overflowed.load(Ordering::SeqCst) {
            return Err(mpsc::RecvTimeoutError::Disconnected);
        }
        let message = self.rx.recv_timeout(timeout)?;
        self.queued_bytes.fetch_sub(message.len(), Ordering::SeqCst);
        Ok(message)
    }
}
