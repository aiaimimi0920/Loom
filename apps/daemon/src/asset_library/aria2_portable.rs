//! Cross-platform pure-Rust HTTP Range download engine for Linux/Docker and portable AssetLibrary hosts.
use super::{aria2_native::State, aria2_native::Task, error, Result};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

struct Job {
    url: String,
    directory: PathBuf,
    expected: u64,
    state: Arc<State>,
    result: mpsc::Sender<i32>,
}

struct Worker {
    sender: mpsc::SyncSender<Job>,
    thread: std::thread::JoinHandle<()>,
}

static WORKER: OnceLock<Mutex<Option<Worker>>> = OnceLock::new();
static STOPPING: AtomicBool = AtomicBool::new(false);

fn execute_portable(client: &reqwest::blocking::Client, job: &Job) -> i32 {
    if STOPPING.load(Ordering::Acquire) || job.state.control.load(Ordering::Acquire) == 1 {
        return 1;
    }
    job.state.total.store(job.expected, Ordering::Release);
    let started = Instant::now();
    let target_path = job.directory.join("payload.zip");
    let mut file = match fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&target_path)
    {
        Ok(file) => file,
        Err(_) => return -2,
    };
    let mut completed = 0_u64;
    let mut chunk = [0_u8; 32 * 1024];

    while completed < job.expected {
        if STOPPING.load(Ordering::Acquire)
            || started.elapsed() > Duration::from_secs(75)
            || job.state.control.load(Ordering::Acquire) == 1
        {
            return 1;
        }
        if job.state.control.load(Ordering::Acquire) == 2 {
            job.state.speed.store(0, Ordering::Release);
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }

        let mut request = client.get(&job.url).timeout(Duration::from_secs(30));
        if completed > 0 {
            request = request.header("Range", format!("bytes={completed}-"));
        }
        let mut response = match request.send() {
            Ok(response) => response,
            Err(_) => return -3,
        };
        let status = response.status().as_u16();
        if completed > 0 && status == 200 {
            if file.set_len(0).is_err() || file.seek(SeekFrom::Start(0)).is_err() {
                return -4;
            }
            completed = 0;
        } else if !matches!(status, 200 | 206) {
            return -5;
        }

        let window_start = Instant::now();
        let mut window_bytes = 0_u64;
        loop {
            if STOPPING.load(Ordering::Acquire)
                || started.elapsed() > Duration::from_secs(75)
                || job.state.control.load(Ordering::Acquire) == 1
            {
                return 1;
            }
            if job.state.control.load(Ordering::Acquire) == 2 {
                break;
            }
            let count = match response.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => count,
                Err(_) => return -6,
            };
            completed = completed.saturating_add(count as u64);
            if completed > job.expected {
                return 1;
            }
            if file.write_all(&chunk[..count]).is_err() {
                return -7;
            }
            window_bytes = window_bytes.saturating_add(count as u64);
            let elapsed_secs = window_start.elapsed().as_secs_f64().max(0.001);
            let speed = (window_bytes as f64 / elapsed_secs) as u64;
            job.state.completed.store(completed, Ordering::Release);
            job.state.speed.store(speed, Ordering::Release);
        }
    }
    if completed != job.expected || file.sync_all().is_err() {
        return -8;
    }
    0
}

pub(super) fn start(
    url: String,
    directory: PathBuf,
    expected: u64,
    state: Arc<State>,
) -> Result<Task> {
    if STOPPING.load(Ordering::Acquire) {
        return Err(error(503, "asset_library_engine_stopping"));
    }
    let mut worker = WORKER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| error(503, "asset_library_engine_start_failed"))?;
    if STOPPING.load(Ordering::Acquire) {
        return Err(error(503, "asset_library_engine_stopping"));
    }
    if worker.is_none() {
        let (sender, receiver) = mpsc::sync_channel::<Job>(8);
        let thread = std::thread::Builder::new()
            .name("loom-http-download".into())
            .spawn(move || {
                let client = reqwest::blocking::Client::builder()
                    .no_proxy()
                    .redirect(reqwest::redirect::Policy::none())
                    .build();
                for job in receiver {
                    let result = match client.as_ref() {
                        Ok(client) => execute_portable(client, &job),
                        Err(_) => -100,
                    };
                    let detached = job.result.send(result).is_err();
                    if result != 0 || detached {
                        super::aria2::cleanup(&job.directory);
                    }
                }
            })
            .map_err(|_| error(503, "asset_library_engine_start_failed"))?;
        *worker = Some(Worker { sender, thread });
    }
    let (result, receiver) = mpsc::channel();
    worker
        .as_ref()
        .expect("worker initialized")
        .sender
        .try_send(Job {
            url,
            directory,
            expected,
            state: state.clone(),
            result,
        })
        .map_err(|_| error(503, "asset_library_engine_busy"))?;
    Ok(Task::from_parts(state, receiver))
}

#[cfg_attr(windows, allow(dead_code))]
pub(super) fn shutdown() {
    STOPPING.store(true, Ordering::Release);
    if let Some(worker) = WORKER.get().and_then(|worker| worker.lock().ok()?.take()) {
        drop(worker.sender);
        let _ = worker.thread.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn portable_engine_downloads_verifies_progress_and_supports_cancel() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
        let origin = format!("http://{}", listener.local_addr().expect("addr"));
        let second_entered = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let second_entered_server = Arc::clone(&second_entered);
        let server = thread::spawn(move || {
            for step in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept");
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .expect("timeout");
                let mut req = Vec::new();
                let mut one = [0_u8];
                while !req.ends_with(b"\r\n\r\n") && req.len() < 4096 {
                    if stream.read_exact(&mut one).is_err() {
                        break;
                    }
                    req.push(one[0]);
                }
                let payload = vec![b'z'; 65536];
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/zip\r\nConnection: close\r\n\r\n",
                    payload.len()
                )
                .expect("headers");
                if step == 0 {
                    let _ = stream.write_all(&payload);
                } else {
                    let _ = stream.write_all(&payload[..16384]);
                    let _ = stream.flush();
                    let (lock, cvar) = &*second_entered_server;
                    *lock.lock().expect("lock entered") = true;
                    cvar.notify_all();
                    thread::sleep(Duration::from_millis(500));
                    let _ = stream.write_all(&payload[16384..]);
                }
            }
        });

        let root = std::env::temp_dir().join(format!(
            "loom-portable-dl-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let ok_dir = root.join("ok");
        fs::create_dir_all(&ok_dir).expect("create ok dir");
        let ok_state = Arc::new(State::default());
        start(
            format!("{origin}/archive.zip"),
            ok_dir.clone(),
            65536,
            ok_state.clone(),
        )
        .expect("start ok")
        .wait()
        .expect("wait ok");
        assert_eq!(ok_state.completed.load(Ordering::Acquire), 65536);
        assert_eq!(
            fs::read(ok_dir.join("payload.zip")).expect("read").len(),
            65536
        );

        let cancel_dir = root.join("cancel");
        fs::create_dir_all(&cancel_dir).expect("create cancel dir");
        let cancel_state = Arc::new(State::default());
        let cancel_task = start(
            format!("{origin}/slow.zip"),
            cancel_dir.clone(),
            65536,
            cancel_state.clone(),
        )
        .expect("start cancel");
        let (lock, cvar) = &*second_entered;
        let guard = lock.lock().expect("lock entered");
        let (guard, _) = cvar
            .wait_timeout_while(guard, Duration::from_secs(3), |entered| !*entered)
            .expect("wait entered");
        assert!(*guard, "second download never connected");
        drop(guard);
        cancel_state.control.store(1, Ordering::Release);
        assert!(cancel_task.wait().is_err());

        server.join().expect("join server");
        let _ = fs::remove_dir_all(root);
    }
}
