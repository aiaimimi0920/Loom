#[test]
fn health_remains_available_while_periodic_cleanup_waits_for_a_store_lock() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let mut daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    let observer = DaemonShutdownObserver::new();
    Arc::get_mut(&mut daemon.runtime)
        .unwrap()
        .maintenance_observer = Some(Arc::clone(&observer));
    let walls = Arc::clone(&daemon.runtime.walls);
    let port = daemon.local_addr().unwrap().port();
    let (locked, lock_observed) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let blocker = thread::spawn(move || {
        walls
            .with_surface_links(|_| {
                locked.send(()).unwrap();
                released.recv_timeout(Duration::from_secs(10)).unwrap();
                Ok(())
            })
            .unwrap();
    });
    lock_observed.recv_timeout(Duration::from_secs(2)).unwrap();
    let (shutdown, receiver) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(receiver));
    let maintenance_started = observer.wait_until_observed(Duration::from_secs(3));
    let mut client = TcpStream::connect(("127.0.0.1", port)).unwrap();
    client
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    client
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    let result = client.read_to_string(&mut response);
    // Always release the store and join workers before asserting, including on regression.
    release.send(()).unwrap();
    blocker.join().unwrap();
    shutdown.send(()).unwrap();
    server.join().unwrap().unwrap();
    assert!(maintenance_started);
    result.expect("health must not wait for maintenance");
    assert!(
        response.contains("200 OK"),
        "response did not satisfy the expected contract (raw output withheld)"
    );
}
