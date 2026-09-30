//! One owned maintenance worker; slow cleanup never occupies the listener thread.
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub(crate) struct PeriodicMaintenance {
    stop: Sender<()>,
    worker: Option<JoinHandle<()>>,
}

impl PeriodicMaintenance {
    pub(crate) fn start(
        interval: Duration,
        mut operation: impl FnMut() + Send + 'static,
    ) -> std::io::Result<Self> {
        let (stop, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("loom-maintenance".to_owned())
            .spawn(move || loop {
                match receiver.recv_timeout(interval) {
                    Err(RecvTimeoutError::Timeout) => operation(),
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                }
            })?;
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }

    pub(crate) fn shutdown(&mut self) -> std::io::Result<()> {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| std::io::Error::other("maintenance worker panicked"))?;
        }
        Ok(())
    }
}

impl Drop for PeriodicMaintenance {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_interrupts_idle_wait_and_is_idempotent() {
        let mut worker = PeriodicMaintenance::start(Duration::from_secs(60), || {}).unwrap();
        let started = std::time::Instant::now();
        worker.shutdown().unwrap();
        worker.shutdown().unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn slow_work_runs_once_at_a_time_and_is_joined_on_shutdown() {
        let (entered, observed) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let mut worker = PeriodicMaintenance::start(Duration::from_millis(1), move || {
            entered
                .send(thread::current().name().unwrap().to_owned())
                .unwrap();
            released.recv_timeout(Duration::from_secs(5)).unwrap();
        })
        .unwrap();
        assert_eq!(
            observed.recv_timeout(Duration::from_secs(2)).unwrap(),
            "loom-maintenance"
        );
        let (finished, completion) = mpsc::channel();
        let joining = thread::spawn(move || {
            worker.shutdown().unwrap();
            finished.send(()).unwrap();
        });
        assert!(completion.recv_timeout(Duration::from_millis(50)).is_err());
        release.send(()).unwrap();
        completion.recv_timeout(Duration::from_secs(2)).unwrap();
        joining.join().unwrap();
        assert!(observed.try_recv().is_err());
    }
}
