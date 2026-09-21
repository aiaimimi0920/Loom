// Real sockets isolate reader admission from the reserved health route's execution.
struct ReadBurstDaemon {
    port: u16,
    accepted: Arc<ConnectionAcceptObserver>,
    shutdown: mpsc::Sender<()>,
    server: Option<thread::JoinHandle<Result<()>>>,
}

impl ReadBurstDaemon {
    fn start() -> Self {
        let mut daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind burst daemon");
        let accepted = ConnectionAcceptObserver::new();
        Arc::get_mut(&mut daemon.runtime)
            .expect("exclusive burst runtime")
            .connection_accept_observer = Some(Arc::clone(&accepted));
        let port = daemon.local_addr().expect("burst daemon address").port();
        let (shutdown, receiver) = mpsc::channel();
        Self {
            port,
            accepted,
            shutdown,
            server: Some(thread::spawn(move || daemon.serve_until(receiver))),
        }
    }

    fn client(&self, complete: bool) -> TcpStream {
        let mut stream =
            TcpStream::connect(("127.0.0.1", self.port)).expect("connect burst client");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("bound burst read");
        stream
            .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n")
            .expect("write burst head");
        if complete {
            stream.write_all(b"\r\n").expect("complete burst head");
        }
        stream
    }
}

impl Drop for ReadBurstDaemon {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

#[test]
fn short_same_peer_read_burst_waits_for_existing_readers() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let daemon = ReadBurstDaemon::start();
    let mut active = (0..3).map(|_| daemon.client(false)).collect::<Vec<_>>();
    assert!(daemon.accepted.wait_for_count(3, Duration::from_secs(2)));
    let mut next = daemon.client(true);
    next.set_read_timeout(Some(Duration::from_millis(50)))
        .expect("observe burst wait");
    let mut response = String::new();
    if let Err(error) = next.read_to_string(&mut response) {
        assert!(
            matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut),
            "{error}"
        );
    }
    for client in &mut active {
        client.write_all(b"\r\n").expect("release earlier reader");
    }
    for client in &mut active {
        let mut earlier = String::new();
        client
            .read_to_string(&mut earlier)
            .expect("read earlier response");
        assert!(earlier.starts_with("HTTP/1.1 200 OK"), "{earlier}");
    }
    next.set_read_timeout(Some(Duration::from_secs(2)))
        .expect("bound final burst read");
    next.read_to_string(&mut response)
        .expect("read burst response");
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
}

#[test]
fn stalled_same_peer_read_burst_is_refused_within_its_wait_budget() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let daemon = ReadBurstDaemon::start();
    let active = (0..3).map(|_| daemon.client(false)).collect::<Vec<_>>();
    assert!(daemon.accepted.wait_for_count(3, Duration::from_secs(2)));
    let started = Instant::now();
    let mut next = daemon.client(true);
    let mut response = String::new();
    next.read_to_string(&mut response)
        .expect("bounded overload response");
    assert!(
        response.starts_with("HTTP/1.1 503 Service Unavailable"),
        "{response}"
    );
    assert!(started.elapsed() < Duration::from_millis(1500));
    drop(active);
}

#[test]
fn queued_same_peer_read_receives_shutdown_without_a_connection_reset() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let daemon = ReadBurstDaemon::start();
    let active = (0..3).map(|_| daemon.client(false)).collect::<Vec<_>>();
    assert!(daemon.accepted.wait_for_count(3, Duration::from_secs(2)));
    let mut next = daemon.client(true);
    next.set_read_timeout(Some(Duration::from_millis(50)))
        .expect("observe queued request");
    let mut response = String::new();
    let error = next
        .read_to_string(&mut response)
        .expect_err("request should wait for a reader");
    assert!(matches!(
        error.kind(),
        ErrorKind::WouldBlock | ErrorKind::TimedOut
    ));
    daemon.shutdown.send(()).expect("request shutdown");
    drop(active);
    next.set_read_timeout(Some(Duration::from_secs(2)))
        .expect("bound shutdown response");
    next.read_to_string(&mut response)
        .expect("read queued shutdown response");
    assert!(
        response.starts_with("HTTP/1.1 503 Service Unavailable"),
        "{response}"
    );
    assert_eq!(
        response_json_body(&response)["error"]["code"],
        "daemon_shutting_down"
    );
}

fn read_backlog_socket_pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind backlog socket");
    let client = TcpStream::connect(listener.local_addr().expect("backlog address"))
        .expect("connect backlog socket");
    let (server, _) = listener.accept().expect("accept backlog socket");
    (server, client)
}

#[test]
fn read_backlog_preserves_peer_fairness_and_an_absolute_deadline() {
    let mut backlog = ConnectionReadBacklog::default();
    let peers = PeerReadAdmission::new(1);
    let peer = "192.0.2.10".parse().expect("first peer");
    let other = "198.51.100.20".parse().expect("other peer");
    let held = peers.try_acquire(peer).expect("hold first reader");
    let now = Instant::now();
    let (blocked, _blocked_client) = read_backlog_socket_pair();
    let (available, _other_client) = read_backlog_socket_pair();
    let expected = available.local_addr().expect("queued other peer socket");
    backlog
        .defer(blocked, peer, now)
        .expect("queue blocked peer");
    backlog
        .defer(available, other, now)
        .expect("queue other peer");
    match backlog.poll(&peers, now) {
        Some(ConnectionReadAdmission::Ready { stream, permit }) => {
            assert_eq!(stream.local_addr().expect("admitted socket"), expected);
            drop(permit);
        }
        _ => panic!("blocked peer delayed an unrelated peer"),
    }
    assert!(backlog
        .poll(&peers, now + Duration::from_millis(200))
        .is_none());
    drop(held);
    assert!(matches!(
        backlog.poll(
            &peers,
            now + Duration::from_millis(CONNECTION_READ_WAIT_MILLIS)
        ),
        Some(ConnectionReadAdmission::Refused(_))
    ));
    assert!(backlog.pending.is_empty());
    assert!(peers.try_acquire(peer).is_some());
}

#[test]
fn read_backlog_bounds_sockets_and_releases_unprocessed_connections() {
    let mut backlog = ConnectionReadBacklog::default();
    let peer = "192.0.2.10".parse().expect("queued peer");
    let mut clients = Vec::new();
    for _ in 0..CONNECTION_READ_QUEUE_CAPACITY {
        let (server, client) = read_backlog_socket_pair();
        backlog
            .defer(server, peer, Instant::now())
            .expect("within backlog bound");
        clients.push(client);
    }
    let (server, _client) = read_backlog_socket_pair();
    assert!(backlog.defer(server, peer, Instant::now()).is_err());
    drop(backlog);
    for client in &mut clients {
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("bound closed socket read");
        assert_eq!(
            client
                .read(&mut [0; 1])
                .expect("read closed backlog socket"),
            0
        );
    }
}
