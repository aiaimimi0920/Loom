use super::*;
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::atomic::AtomicBool,
    thread,
};

fn respond(mut stream: TcpStream, ranges: Arc<AtomicU64>) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut request = Vec::new();
    while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
        let mut byte = [0];
        if stream.read_exact(&mut byte).is_err() {
            return;
        }
        request.push(byte[0]);
    }
    let request = String::from_utf8_lossy(&request).to_lowercase();
    let slow = request.starts_with("get /slow ");
    let size = if slow {
        6 * 1024 * 1024
    } else if request.starts_with("get /large ") {
        80_000_000
    } else {
        65536
    };
    let range = request
        .lines()
        .find_map(|line| line.strip_prefix("range: bytes="));
    let (start, end) = range
        .map(|range| {
            ranges.fetch_add(1, Ordering::Relaxed);
            let (start, end) = range.split_once('-').unwrap();
            (
                start.parse::<usize>().unwrap(),
                if end.is_empty() {
                    size - 1
                } else {
                    end.parse::<usize>().unwrap().min(size - 1)
                },
            )
        })
        .unwrap_or((0, size - 1));
    if start > end || start >= size {
        return;
    }
    let length = end - start + 1;
    let mut headers = format!("HTTP/1.1 {} OK\r\nContent-Length: {length}\r\nContent-Type: application/zip\r\nAccept-Ranges: bytes\r\nConnection: close\r\n", if range.is_some() { 206 } else { 200 });
    if range.is_some() {
        headers.push_str(&format!("Content-Range: bytes {start}-{end}/{size}\r\n"));
    }
    headers.push_str("\r\n");
    if stream.write_all(headers.as_bytes()).is_err() {
        return;
    }
    let bytes = vec![b'x'; 16384];
    let mut remaining = length;
    while remaining > 0 {
        let n = remaining.min(bytes.len());
        if stream.write_all(&bytes[..n]).is_err() {
            return;
        }
        remaining -= n;
        if slow {
            thread::sleep(Duration::from_millis(100));
        }
    }
}

#[test]
#[ignore = "requires pinned Windows libaria2 DLL; run in an isolated test process"]
fn libaria2_real_library_progress_pause_resume_cancel_and_shutdown() {
    let library =
        PathBuf::from(std::env::var_os("LOOM_TEST_LIBARIA2").expect("pinned library path"));
    let root =
        std::env::temp_dir().join(format!("loom-libaria2-{}", uuid::Uuid::new_v4().simple()));
    fs::create_dir(&root).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = stop.clone();
    let ranges = Arc::new(AtomicU64::new(0));
    let server_ranges = ranges.clone();
    let server = thread::spawn(move || {
        let mut workers = Vec::new();
        while !server_stop.load(Ordering::Acquire) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let ranges = server_ranges.clone();
                    workers.push(thread::spawn(move || respond(stream, ranges)));
                }
                Err(_) => thread::sleep(Duration::from_millis(10)),
            }
        }
        for worker in workers {
            worker.join().unwrap();
        }
    });
    let task_id = uuid::Uuid::new_v4().to_string();
    let registration =
        super::super::download_control::register(&root, &task_id, "fixture-owner").unwrap();
    let command = |action: &str| {
        super::super::download_control::control(
            &root,
            &serde_json::json!({"taskId":task_id,"action":action}).to_string(),
            "fixture-owner",
        )
        .unwrap()
    };
    let slow_directory = root.join("cancelled");
    fs::create_dir(&slow_directory).unwrap();
    let task = start_controlled(
        format!("{origin}/slow"),
        slow_directory.clone(),
        6 * 1024 * 1024,
        registration.state.clone(),
    )
    .unwrap();
    let deadline = Instant::now();
    while (task.progress().1 == 0 || ranges.load(Ordering::Acquire) == 0)
        && deadline.elapsed() < Duration::from_secs(10)
    {
        thread::sleep(Duration::from_millis(50));
    }
    assert!(
        task.progress().1 > 0,
        "native callback reported no progress"
    );
    assert!(
        ranges.load(Ordering::Acquire) > 0,
        "engine did not issue Range requests"
    );
    command("pause");
    thread::sleep(Duration::from_millis(1800));
    let paused = task.progress().1;
    thread::sleep(Duration::from_millis(1300));
    assert_eq!(task.progress().1, paused, "paused download advanced");
    command("resume");
    let deadline = Instant::now();
    while task.progress().1 <= paused && deadline.elapsed() < Duration::from_secs(8) {
        thread::sleep(Duration::from_millis(50));
    }
    assert!(task.progress().1 > paused, "download did not resume");
    command("cancel");
    assert_eq!(command("resume")["control"], 1);
    assert!(task.wait().is_err());
    let deadline = Instant::now();
    while slow_directory.exists() && deadline.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !slow_directory.exists(),
        "cancelled task files were not reclaimed"
    );
    for index in 0..2 {
        let directory = root.join(format!("颜色迁移-{index}"));
        fs::create_dir(&directory).unwrap();
        let state = Arc::new(State::default());
        start_controlled(
            format!("{origin}/fast"),
            directory.clone(),
            65536,
            state.clone(),
        )
        .unwrap()
        .wait()
        .unwrap();
        assert_eq!(
            state.control.load(Ordering::Acquire),
            0,
            "successful completion is not cancellation"
        );
        assert_eq!(
            fs::read(directory.join("payload.zip")).unwrap(),
            vec![b'x'; 65536]
        );
    }
    let large_directory = root.join("large-application");
    fs::create_dir(&large_directory).unwrap();
    let large_state = Arc::new(State::default());
    let large_result = start_controlled(
        format!("{origin}/large"),
        large_directory.clone(),
        80_000_000,
        large_state.clone(),
    )
    .unwrap()
    .wait();
    assert!(
        large_result.is_ok(),
        "large result={large_result:?}, total={}, completed={}",
        large_state.total.load(Ordering::Acquire),
        large_state.completed.load(Ordering::Acquire)
    );
    let mut large_file = fs::File::open(large_directory.join("payload.zip")).unwrap();
    assert_eq!(large_file.metadata().unwrap().len(), 80_000_000);
    let mut buffer = [0u8; 65536];
    loop {
        let count = large_file.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        assert!(buffer[..count].iter().all(|byte| *byte == b'x'));
    }
    drop(large_file);
    assert!(
        fs::OpenOptions::new().write(true).open(&library).is_err(),
        "loaded engine must remain locked"
    );
    shutdown();
    assert!(
        fs::OpenOptions::new().write(true).open(&library).is_ok(),
        "engine was not unloaded"
    );
    assert!(start(format!("{origin}/fast"), root.clone(), 65536).is_err());
    stop.store(true, Ordering::Release);
    server.join().unwrap();
    fs::remove_dir_all(root).unwrap();
}
