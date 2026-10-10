//! Byte- and count-bounded broadcasts; overloaded subscribers fail closed without blocking producers.
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc, Arc,
};
#[cfg(test)]
use std::time::Duration;

pub(crate) const MAX_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_MESSAGES: usize = 128;

struct Packet {
    text: Option<String>,
    bytes: usize,
    budget: Arc<AtomicUsize>,
}

impl Packet {
    fn into_text(mut self) -> String {
        self.text.take().unwrap_or_default()
    }
}

impl Drop for Packet {
    fn drop(&mut self) {
        self.budget.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

#[derive(Clone)]
pub(crate) struct BroadcastSender {
    sender: mpsc::SyncSender<Packet>,
    bytes: Arc<AtomicUsize>,
    failed: Arc<AtomicBool>,
}

pub(crate) struct BroadcastReceiver {
    receiver: mpsc::Receiver<Packet>,
    failed: Arc<AtomicBool>,
}

pub(crate) fn channel() -> (BroadcastSender, BroadcastReceiver) {
    let (sender, receiver) = mpsc::sync_channel(MAX_MESSAGES);
    let failed = Arc::new(AtomicBool::new(false));
    (
        BroadcastSender {
            sender,
            bytes: Arc::new(AtomicUsize::new(0)),
            failed: failed.clone(),
        },
        BroadcastReceiver { receiver, failed },
    )
}

impl BroadcastSender {
    pub(crate) fn try_send(&self, text: &str) -> bool {
        if self.failed.load(Ordering::Acquire) {
            return false;
        }
        if self
            .bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |bytes| {
                bytes
                    .checked_add(text.len())
                    .filter(|total| *total <= MAX_BYTES)
            })
            .is_err()
        {
            self.failed.store(true, Ordering::Release);
            return false;
        }
        let packet = Packet {
            text: Some(text.to_owned()),
            bytes: text.len(),
            budget: self.bytes.clone(),
        };
        if self.sender.try_send(packet).is_err() {
            self.failed.store(true, Ordering::Release);
            return false;
        }
        true
    }
}

impl BroadcastReceiver {
    pub(crate) fn try_recv(&self) -> Result<String, mpsc::TryRecvError> {
        if self.failed.load(Ordering::Acquire) {
            return Err(mpsc::TryRecvError::Disconnected);
        }
        self.receiver.try_recv().map(Packet::into_text)
    }

    #[cfg(test)]
    pub(crate) fn recv_timeout(&self, timeout: Duration) -> Result<String, mpsc::RecvTimeoutError> {
        if self.failed.load(Ordering::Acquire) {
            return Err(mpsc::RecvTimeoutError::Disconnected);
        }
        self.receiver.recv_timeout(timeout).map(Packet::into_text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_consumer_is_disconnected_at_count_limit_without_blocking() {
        let (tx, rx) = channel();
        for _ in 0..MAX_MESSAGES {
            assert!(tx.try_send("event"));
        }
        assert!(!tx.try_send("overflow"));
        assert!(matches!(
            rx.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
        drop(rx);
        assert_eq!(tx.bytes.load(Ordering::Acquire), 0);
    }

    #[test]
    fn byte_budget_is_released_on_receive_and_failed_send() {
        let (tx, rx) = channel();
        let text = "x".repeat(MAX_BYTES);
        assert!(tx.try_send(&text));
        assert_eq!(rx.try_recv().unwrap().len(), MAX_BYTES);
        assert_eq!(tx.bytes.load(Ordering::Acquire), 0);
        assert!(tx.try_send(&text));
        assert!(!tx.try_send("overflow"));
        drop(rx);
        assert_eq!(tx.bytes.load(Ordering::Acquire), 0);
        let (tx, rx) = channel();
        drop(rx);
        assert!(!tx.try_send("gone"));
        assert_eq!(tx.bytes.load(Ordering::Acquire), 0);
    }
}
