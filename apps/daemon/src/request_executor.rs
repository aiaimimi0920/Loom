use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use serde::Serialize;
use thiserror::Error;

pub(crate) const DEFAULT_REQUEST_WORKERS: usize = 4;
pub(crate) const DEFAULT_REQUEST_QUEUE_CAPACITY: usize = 32;
const MAX_REQUEST_WORKERS: usize = 32;
const MAX_REQUEST_QUEUE_CAPACITY: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RequestExecutorConfig {
    Inline,
    Bounded {
        workers: usize,
        queue_capacity: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RequestExecutorStatus {
    pub mode: &'static str,
    pub workers: usize,
    pub queue_capacity: usize,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error)]
pub(crate) enum RequestExecutorConfigError {
    #[error("LOOM_DAEMON_WORKERS must be between 1 and 32, got {0}")]
    InvalidWorkers(usize),
    #[error("LOOM_DAEMON_QUEUE_CAPACITY must be between 1 and 1024, got {0}")]
    InvalidQueueCapacity(usize),
    #[error("{name} must be an unsigned integer, got `{value}`")]
    InvalidEnvironment { name: &'static str, value: String },
}

impl RequestExecutorConfig {
    #[must_use]
    pub(crate) fn production_default() -> Self {
        Self::Bounded {
            workers: DEFAULT_REQUEST_WORKERS,
            queue_capacity: DEFAULT_REQUEST_QUEUE_CAPACITY,
        }
    }

    pub(crate) fn bounded(
        workers: usize,
        queue_capacity: usize,
    ) -> Result<Self, RequestExecutorConfigError> {
        if !(1..=MAX_REQUEST_WORKERS).contains(&workers) {
            return Err(RequestExecutorConfigError::InvalidWorkers(workers));
        }
        if !(1..=MAX_REQUEST_QUEUE_CAPACITY).contains(&queue_capacity) {
            return Err(RequestExecutorConfigError::InvalidQueueCapacity(
                queue_capacity,
            ));
        }
        Ok(Self::Bounded {
            workers,
            queue_capacity,
        })
    }

    pub(crate) fn from_env() -> Result<Self, RequestExecutorConfigError> {
        let defaults = Self::production_default().status();
        let workers = request_executor_environment_value("LOOM_DAEMON_WORKERS", defaults.workers)?;
        let queue_capacity = request_executor_environment_value(
            "LOOM_DAEMON_QUEUE_CAPACITY",
            defaults.queue_capacity,
        )?;
        Self::bounded(workers, queue_capacity)
    }

    #[must_use]
    pub(crate) fn status(self) -> RequestExecutorStatus {
        match self {
            Self::Inline => RequestExecutorStatus {
                mode: "inline",
                workers: 1,
                queue_capacity: 0,
            },
            Self::Bounded {
                workers,
                queue_capacity,
            } => RequestExecutorStatus {
                mode: "bounded_workers",
                workers,
                queue_capacity,
            },
        }
    }
}

fn request_executor_environment_value(
    name: &'static str,
    default: usize,
) -> Result<usize, RequestExecutorConfigError> {
    let value = match std::env::var(name) {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => return Ok(default),
        Err(std::env::VarError::NotUnicode(value)) => {
            return Err(RequestExecutorConfigError::InvalidEnvironment {
                name,
                value: value.to_string_lossy().into_owned(),
            });
        }
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(default);
    }
    value
        .parse::<usize>()
        .map_err(|_| RequestExecutorConfigError::InvalidEnvironment {
            name,
            value: value.to_owned(),
        })
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum SubmitError<T> {
    Full(T),
    Closed(T),
}

pub(crate) struct BoundedRequestExecutor<T: Send + 'static> {
    sender: Option<SyncSender<T>>,
    workers: Vec<JoinHandle<()>>,
}

impl<T: Send + 'static> BoundedRequestExecutor<T> {
    pub(crate) fn new<F>(
        thread_prefix: &str,
        workers: usize,
        queue_capacity: usize,
        handler: F,
    ) -> std::io::Result<Self>
    where
        F: Fn(T) + Send + Sync + 'static,
    {
        Self::new_with_spawner(
            thread_prefix,
            workers,
            queue_capacity,
            handler,
            |name, worker| thread::Builder::new().name(name).spawn(worker),
        )
    }

    fn new_with_spawner<F, S>(
        thread_prefix: &str,
        workers: usize,
        queue_capacity: usize,
        handler: F,
        mut spawn_worker: S,
    ) -> std::io::Result<Self>
    where
        F: Fn(T) + Send + Sync + 'static,
        S: FnMut(String, Box<dyn FnOnce() + Send + 'static>) -> std::io::Result<JoinHandle<()>>,
    {
        let (sender, receiver) = mpsc::sync_channel(queue_capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        let handler = Arc::new(handler);
        let mut worker_handles = Vec::with_capacity(workers);
        for index in 0..workers {
            let receiver = Arc::clone(&receiver);
            let handler = Arc::clone(&handler);
            let worker = Box::new(move || worker_loop(receiver, handler));
            match spawn_worker(format!("{thread_prefix}-{index}"), worker) {
                Ok(worker) => worker_handles.push(worker),
                Err(error) => {
                    drop(sender);
                    for worker in worker_handles {
                        let _ = worker.join();
                    }
                    return Err(error);
                }
            }
        }
        Ok(Self {
            sender: Some(sender),
            workers: worker_handles,
        })
    }

    pub(crate) fn try_submit(&self, job: T) -> Result<(), SubmitError<T>> {
        let Some(sender) = self.sender.as_ref() else {
            return Err(SubmitError::Closed(job));
        };
        sender.try_send(job).map_err(|error| match error {
            TrySendError::Full(job) => SubmitError::Full(job),
            TrySendError::Disconnected(job) => SubmitError::Closed(job),
        })
    }

    pub(crate) fn close(&mut self) {
        self.sender.take();
    }

    pub(crate) fn shutdown(&mut self) -> std::io::Result<()> {
        self.close();
        let mut first_join_error = None;
        for worker in self.workers.drain(..) {
            if worker.join().is_err() && first_join_error.is_none() {
                first_join_error = Some(std::io::Error::other(
                    "Loom request worker terminated unexpectedly",
                ));
            }
        }
        if let Some(error) = first_join_error {
            return Err(error);
        }
        Ok(())
    }
}

impl<T: Send + 'static> Drop for BoundedRequestExecutor<T> {
    fn drop(&mut self) {
        self.close();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn worker_loop<T, F>(receiver: Arc<Mutex<Receiver<T>>>, handler: Arc<F>)
where
    T: Send + 'static,
    F: Fn(T) + Send + Sync + 'static,
{
    loop {
        let job = {
            let receiver = match receiver.lock() {
                Ok(receiver) => receiver,
                Err(_) => return,
            };
            match receiver.recv() {
                Ok(job) => job,
                Err(_) => return,
            }
        };
        let _ = catch_unwind(AssertUnwindSafe(|| handler(job)));
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{mpsc, Arc, Barrier, Condvar, Mutex};
    use std::thread;
    use std::time::Duration;

    use super::{BoundedRequestExecutor, RequestExecutorConfig, SubmitError};

    #[test]
    fn production_defaults_are_bounded_and_stable() {
        assert_eq!(
            RequestExecutorConfig::production_default(),
            RequestExecutorConfig::Bounded {
                workers: 4,
                queue_capacity: 32,
            }
        );
    }

    #[test]
    fn executor_config_rejects_invalid_ranges() {
        assert!(RequestExecutorConfig::bounded(0, 32).is_err());
        assert!(RequestExecutorConfig::bounded(33, 32).is_err());
        assert!(RequestExecutorConfig::bounded(4, 0).is_err());
        assert!(RequestExecutorConfig::bounded(4, 1025).is_err());
    }

    #[test]
    fn constructor_joins_started_workers_when_later_spawn_fails() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let exited = Arc::new(AtomicBool::new(false));
        let spawn_attempts = Arc::clone(&attempts);
        let worker_exited = Arc::clone(&exited);

        let result = BoundedRequestExecutor::<usize>::new_with_spawner(
            "loom-request",
            2,
            1,
            |_value| {},
            move |name, worker| {
                if spawn_attempts.fetch_add(1, Ordering::SeqCst) == 1 {
                    return Err(io::Error::other("fixture spawn failure"));
                }
                let worker_exited = Arc::clone(&worker_exited);
                thread::Builder::new().name(name).spawn(move || {
                    worker();
                    worker_exited.store(true, Ordering::SeqCst);
                })
            },
        );

        let error = match result {
            Ok(_) => panic!("second worker spawn should fail"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert!(exited.load(Ordering::SeqCst));
    }

    #[test]
    fn shutdown_joins_remaining_workers_after_first_worker_panics() {
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let second_exited = Arc::new(AtomicBool::new(false));
        let spawn_index = Arc::new(AtomicUsize::new(0));
        let release_for_spawner = Arc::clone(&release);
        let exited_for_spawner = Arc::clone(&second_exited);
        let spawn_index_for_spawner = Arc::clone(&spawn_index);

        let executor = BoundedRequestExecutor::<usize>::new_with_spawner(
            "loom-request",
            2,
            1,
            |_value| {},
            move |name, worker| {
                let index = spawn_index_for_spawner.fetch_add(1, Ordering::SeqCst);
                if index == 0 {
                    return thread::Builder::new()
                        .name(name)
                        .spawn(|| panic!("fixture worker panic"));
                }

                let release = Arc::clone(&release_for_spawner);
                let exited = Arc::clone(&exited_for_spawner);
                thread::Builder::new().name(name).spawn(move || {
                    drop(worker);
                    let (release_lock, release_signal) = &*release;
                    let released = release_lock.lock().expect("lock release");
                    let (released, _) = release_signal
                        .wait_timeout_while(released, Duration::from_secs(3), |released| !*released)
                        .expect("wait release");
                    if *released {
                        exited.store(true, Ordering::SeqCst);
                    }
                })
            },
        )
        .expect("create executor");

        let (result_tx, result_rx) = mpsc::channel();
        let shutdown_barrier = Arc::new(Barrier::new(2));
        let worker_shutdown_barrier = Arc::clone(&shutdown_barrier);
        let shutdown_thread = thread::spawn(move || {
            let mut executor = executor;
            worker_shutdown_barrier.wait();
            result_tx
                .send(executor.shutdown())
                .expect("report shutdown result");
        });

        shutdown_barrier.wait();
        let early_result = result_rx.recv_timeout(Duration::from_millis(250));
        let returned_early = early_result.is_ok();
        let (release_lock, release_signal) = &*release;
        *release_lock.lock().expect("release worker") = true;
        release_signal.notify_all();

        let shutdown_result = early_result.unwrap_or_else(|_| {
            result_rx
                .recv_timeout(Duration::from_secs(3))
                .expect("shutdown result after releasing worker")
        });
        shutdown_thread.join().expect("shutdown thread");

        assert!(
            !returned_early,
            "shutdown returned before joining the remaining worker"
        );
        assert!(
            shutdown_result.is_err(),
            "first worker panic must be reported"
        );
        assert!(second_exited.load(Ordering::SeqCst));
    }

    #[test]
    fn bounded_executor_runs_jobs_on_named_workers() {
        let names = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&names);
        let mut executor =
            BoundedRequestExecutor::new("loom-request", 2, 4, move |value: usize| {
                captured.lock().expect("lock names").push((
                    value,
                    std::thread::current()
                        .name()
                        .expect("worker name")
                        .to_owned(),
                ));
            })
            .expect("create executor");

        executor.try_submit(1).expect("submit first");
        executor.try_submit(2).expect("submit second");
        executor.shutdown().expect("shutdown executor");

        let names = names.lock().expect("read names");
        assert_eq!(names.len(), 2);
        assert!(names
            .iter()
            .all(|(_, name)| name.starts_with("loom-request-")));
    }

    #[test]
    fn full_queue_returns_the_original_job() {
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let entered = Arc::new((Mutex::new(false), Condvar::new()));
        let worker_gate = Arc::clone(&gate);
        let worker_entered = Arc::clone(&entered);
        let mut executor =
            BoundedRequestExecutor::new("loom-request", 1, 1, move |_value: usize| {
                let (entered_lock, entered_signal) = &*worker_entered;
                *entered_lock.lock().expect("lock entered") = true;
                entered_signal.notify_all();
                let (gate_lock, gate_signal) = &*worker_gate;
                let released = gate_lock.lock().expect("lock gate");
                let _ = gate_signal
                    .wait_timeout_while(released, Duration::from_secs(3), |released| !*released)
                    .expect("wait gate");
            })
            .expect("create executor");

        executor.try_submit(1).expect("submit active job");
        let (entered_lock, entered_signal) = &*entered;
        let did_enter = entered_lock.lock().expect("read entered");
        let (did_enter, _) = entered_signal
            .wait_timeout_while(did_enter, Duration::from_secs(3), |entered| !*entered)
            .expect("wait entered");
        assert!(*did_enter, "worker did not enter before the deadline");
        drop(did_enter);
        executor.try_submit(2).expect("submit queued job");
        assert!(matches!(executor.try_submit(3), Err(SubmitError::Full(3))));

        let (gate_lock, gate_signal) = &*gate;
        *gate_lock.lock().expect("release gate") = true;
        gate_signal.notify_all();
        executor.shutdown().expect("shutdown executor");
    }

    #[test]
    fn panicking_job_does_not_kill_the_worker() {
        let completed = Arc::new(Mutex::new(Vec::new()));
        let worker_completed = Arc::clone(&completed);
        let mut executor =
            BoundedRequestExecutor::new("loom-request", 1, 2, move |value: usize| {
                if value == 1 {
                    panic!("fixture panic");
                }
                worker_completed.lock().expect("lock completed").push(value);
            })
            .expect("create executor");

        executor.try_submit(1).expect("submit panic");
        executor.try_submit(2).expect("submit recovery");
        executor.shutdown().expect("shutdown executor");
        assert_eq!(*completed.lock().expect("read completed"), vec![2]);
    }

    #[test]
    fn closed_executor_returns_the_original_job() {
        let mut executor = BoundedRequestExecutor::new("loom-request", 1, 1, |_value: usize| {})
            .expect("create executor");
        executor.close();
        assert!(matches!(
            executor.try_submit(7),
            Err(SubmitError::Closed(7))
        ));
        executor.shutdown().expect("join executor");
    }
}
