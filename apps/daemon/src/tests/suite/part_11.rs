// Loom daemon tests fragment 11; included into the shared crate test module.
#[test]
fn daemon_serves_probes_while_brain_plan_is_blocked() {
    let entered = Arc::new((Mutex::new(false), Condvar::new()));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let mut daemon =
        LoomDaemon::bind(DaemonConfig::localhost(0).with_bounded_request_executor(2, 4))
            .expect("bind daemon");
    Arc::get_mut(&mut daemon.runtime)
        .expect("exclusive daemon runtime")
        .brain_planner = Arc::new(BlockingBrainPlanner {
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
    });
    let port = daemon.local_addr().expect("address").port();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx));
    let mut fixture = ConcurrencyTestFixture::new(shutdown_tx, server);
    fixture.add_release_gate(Arc::clone(&release));

    let invoke_rx = fixture.spawn_client(move || {
            http_request(
                port,
                "POST",
                "/v1/invoke",
                Some(
                    r#"{"requestId":"concurrent-plan","caller":"test","capability":"brain.plan","input":{"goal":"block planner"}}"#,
                ),
            )
        });
    assert!(
        wait_for_test_gate(&entered, Duration::from_millis(750)),
        "planner did not enter before the deadline"
    );

    let probes_rx = fixture.spawn_client(move || {
        let health = http_get(port, "/health");
        let status = http_get(port, "/status");
        (health, status)
    });
    let probes_before_release = probes_rx.recv_timeout(Duration::from_millis(750));
    let probes_returned_while_blocked = probes_before_release.is_ok();

    fixture.release_gates();
    let invoke = invoke_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("invoke response after release");
    let (health, status) = probes_before_release.unwrap_or_else(|_| {
        probes_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("probe responses after release")
    });
    fixture.finish().expect("serve");
    assert!(
        probes_returned_while_blocked,
        "health and status did not return while planning was blocked"
    );
    assert!(health.starts_with("HTTP/1.1 200 OK"));
    assert!(status.starts_with("HTTP/1.1 200 OK"));
    assert!(invoke.starts_with("HTTP/1.1 200 OK"));
}

#[test]
fn daemon_runs_approved_capabilities_concurrently() {
    let entered = Arc::new((Mutex::new(0_usize), Condvar::new()));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let mut daemon =
        LoomDaemon::bind(DaemonConfig::localhost(0).with_bounded_request_executor(2, 2))
            .expect("bind daemon");
    Arc::get_mut(&mut daemon.runtime)
        .expect("exclusive daemon runtime")
        .brain_planner = Arc::new(CountingBlockingBrainPlanner {
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
    });
    let port = daemon.local_addr().expect("address").port();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx));
    let mut fixture = ConcurrencyTestFixture::new(shutdown_tx, server);
    fixture.add_release_gate(Arc::clone(&release));

    let first_rx = fixture.spawn_client(move || {
            http_request(
                port,
                "POST",
                "/v1/invoke",
                Some(
                    r#"{"requestId":"overlap-first","caller":"test","capability":"brain.plan","input":{"goal":"overlap first"}}"#,
                ),
            )
        });
    let second_rx = fixture.spawn_client(move || {
            http_request(
                port,
                "POST",
                "/v1/invoke",
                Some(
                    r#"{"requestId":"overlap-second","caller":"test","capability":"brain.plan","input":{"goal":"overlap second"}}"#,
                ),
            )
        });

    let (entered_lock, entered_signal) = &*entered;
    let entered_count = entered_lock.lock().expect("read planner entries");
    let (entered_count, _) = entered_signal
        .wait_timeout_while(entered_count, Duration::from_millis(750), |count| {
            *count < 2
        })
        .expect("wait planner entries");
    let overlapped = *entered_count >= 2;
    drop(entered_count);

    fixture.release_gates();
    let first_response = first_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("first capability response");
    let second_response = second_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("second capability response");
    fixture.finish().expect("serve");
    assert!(overlapped, "approved capabilities did not overlap");
    assert!(first_response.starts_with("HTTP/1.1 200 OK"));
    assert!(second_response.starts_with("HTTP/1.1 200 OK"));
}

#[test]
fn daemon_returns_busy_when_request_queue_is_full() {
    let entered = Arc::new((Mutex::new(false), Condvar::new()));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let submissions = RequestSubmissionObserver::new();
    let mut daemon =
        LoomDaemon::bind(DaemonConfig::localhost(0).with_bounded_request_executor(1, 1))
            .expect("bind daemon");
    let inserted_runs = Arc::new(AtomicUsize::new(0));
    let runtime = Arc::get_mut(&mut daemon.runtime).expect("exclusive daemon runtime");
    runtime.brain_planner = Arc::new(BlockingBrainPlanner {
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
    });
    runtime.run_store = Arc::new(Mutex::new(Box::new(CountingRunEvidenceStore::new(
        Arc::clone(&inserted_runs),
    ))));
    runtime.request_submission_observer = Some(Arc::clone(&submissions));
    let port = daemon.local_addr().expect("address").port();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx));
    let mut fixture = ConcurrencyTestFixture::new(shutdown_tx, server);
    fixture.add_release_gate(Arc::clone(&release));

    let first_body =
            r#"{"requestId":"queue-first","caller":"test","capability":"brain.plan","input":{"goal":"queue first"}}"#
                .to_owned();
    let first_rx =
        fixture.spawn_client(move || http_request(port, "POST", "/v1/invoke", Some(&first_body)));
    assert!(
        wait_for_test_gate(&entered, Duration::from_millis(750)),
        "planner did not enter before the deadline"
    );

    let second_body =
            r#"{"requestId":"queue-second","caller":"test","capability":"brain.plan","input":{"goal":"queue second"}}"#
                .to_owned();
    let second_rx =
        fixture.spawn_client(move || http_request(port, "POST", "/v1/invoke", Some(&second_body)));
    let second_submitted = submissions.wait_for_count(2);

    let third_rx = fixture.spawn_client(move || {
            http_request(
                port,
                "POST",
                "/v1/invoke",
                Some(
                    r#"{"requestId":"queue-third","caller":"test","capability":"brain.plan","input":{"goal":"queue third"}}"#,
                ),
            )
        });
    let third_response_before_release = third_rx.recv_timeout(Duration::from_millis(750)).ok();
    let third_returned_before_release = third_response_before_release.is_some();

    fixture.release_gates();
    let first_response = first_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("first client response");
    let second_response = second_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("second client response");
    let third_response = third_response_before_release.unwrap_or_else(|| {
        third_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("third response after release")
    });

    let health = http_get(port, "/health");
    fixture.finish().expect("serve");

    assert!(
        third_returned_before_release,
        "third request did not receive an overload response before release"
    );
    assert!(
        second_submitted,
        "second request was not submitted to the queue"
    );
    assert!(third_response.starts_with("HTTP/1.1 503 Service Unavailable"));
    let third_body = response_json_body(&third_response);
    assert_eq!(third_body["error"]["code"], "daemon_busy");
    assert_eq!(third_body["error"]["retryable"], true);
    assert!(!third_body.to_string().contains("queue-third"));
    assert!(first_response.starts_with("HTTP/1.1 200 OK"));
    assert!(second_response.starts_with("HTTP/1.1 200 OK"));
    assert!(!first_response.contains("queue-third"));
    assert!(!second_response.contains("queue-third"));
    assert_eq!(
        inserted_runs.load(Ordering::SeqCst),
        2,
        "overloaded request created run evidence"
    );
    assert!(health.starts_with("HTTP/1.1 200 OK"));
}

#[test]
fn daemon_shutdown_drains_active_and_queued_requests() {
    let entered = Arc::new((Mutex::new(false), Condvar::new()));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let submissions = RequestSubmissionObserver::new();
    let shutdown_observer = DaemonShutdownObserver::new();
    let mut daemon =
        LoomDaemon::bind(DaemonConfig::localhost(0).with_bounded_request_executor(1, 1))
            .expect("bind daemon");
    let runtime = Arc::get_mut(&mut daemon.runtime).expect("exclusive daemon runtime");
    runtime.brain_planner = Arc::new(BlockingBrainPlanner {
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
    });
    runtime.request_submission_observer = Some(Arc::clone(&submissions));
    runtime.shutdown_observer = Some(Arc::clone(&shutdown_observer));
    let port = daemon.local_addr().expect("address").port();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let (server_done_tx, server_done_rx) = mpsc::channel();
    let server = thread::spawn(move || {
        let result = daemon.serve_until(shutdown_rx);
        server_done_tx.send(()).expect("report server completion");
        result
    });
    let mut fixture = ConcurrencyTestFixture::new(shutdown_tx, server);
    fixture.add_release_gate(Arc::clone(&release));

    let first_rx = fixture.spawn_client(move || {
            http_request(
                port,
                "POST",
                "/v1/invoke",
                Some(
                    r#"{"requestId":"drain-first","caller":"test","capability":"brain.plan","input":{"goal":"drain first"}}"#,
                ),
            )
        });
    assert!(
        wait_for_test_gate(&entered, Duration::from_millis(750)),
        "planner did not enter before the deadline"
    );

    let second_rx = fixture.spawn_client(move || {
            http_request(
                port,
                "POST",
                "/v1/invoke",
                Some(
                    r#"{"requestId":"drain-second","caller":"test","capability":"brain.plan","input":{"goal":"drain second"}}"#,
                ),
            )
        });
    let second_submitted = submissions.wait_for_count(2);
    fixture.request_shutdown();
    assert!(
        shutdown_observer.wait_until_observed(Duration::from_secs(3)),
        "serve loop did not observe shutdown before the deadline"
    );
    let stopped_before_release = server_done_rx
        .recv_timeout(Duration::from_millis(250))
        .is_ok();

    fixture.release_gates();
    let first_response = first_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("first drained request");
    let second_response = second_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("second drained request");
    fixture.finish().expect("serve");
    assert!(
        !stopped_before_release,
        "daemon returned before active work was released"
    );
    assert!(
        second_submitted,
        "second request was not queued before shutdown"
    );
    assert!(first_response.starts_with("HTTP/1.1 200 OK"));
    assert!(second_response.starts_with("HTTP/1.1 200 OK"));
}

#[test]
fn daemon_returns_shutting_down_for_request_accepted_before_shutdown() {
    let accepted = ConnectionAcceptObserver::new();
    let mut daemon =
        LoomDaemon::bind(DaemonConfig::localhost(0).with_bounded_request_executor(1, 1))
            .expect("bind daemon");
    let runtime = Arc::get_mut(&mut daemon.runtime).expect("exclusive daemon runtime");
    runtime.connection_accept_observer = Some(Arc::clone(&accepted));
    let port = daemon.local_addr().expect("address").port();
    let mut client = TcpStream::connect(("127.0.0.1", port)).expect("connect client");
    client
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("set client timeout");
    let body = r#"{"requestId":"shutdown-race","caller":"test","capability":"brain.plan","input":{"goal":"shutdown race"}}"#;
    write!(
            client,
            "POST /v1/invoke HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {TEST_DAEMON_AUTH_TOKEN}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .expect("write partial request");

    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx));
    // The connection has to be accepted before shutdown is asked for, otherwise this exercises the
    // backlog drain rather than the accepted-then-shutdown path the test is named for. Waiting for
    // the accept itself says so; the fixed 100ms sleep this replaces only guessed at it, and on a
    // loaded machine the guess was wrong in the direction that made the response read time out.
    assert!(
        accepted.wait_for_count(1, Duration::from_secs(3)),
        "serve loop did not accept the connection before the deadline"
    );
    shutdown_tx.send(()).expect("request shutdown");
    client
        .write_all(body.as_bytes())
        .expect("complete request body");
    client
        .shutdown(Shutdown::Write)
        .expect("close client write side");

    let mut response = String::new();
    client
        .read_to_string(&mut response)
        .expect("read shutdown response");
    server.join().expect("server thread").expect("serve daemon");

    assert!(response.starts_with("HTTP/1.1 503 Service Unavailable"));
    let body = response_json_body(&response);
    assert_eq!(body["error"]["code"], "daemon_shutting_down");
    assert_eq!(body["error"]["retryable"], true);
}

#[test]
fn daemon_shutting_down_response_is_retryable_service_unavailable() {
    let (status, body) = daemon_shutting_down_response();
    assert_eq!(status, 503);
    let body: Value = serde_json::from_str(&body).expect("shutdown response json");
    assert_eq!(
        body,
        serde_json::json!({
            "error": {
                "code": "daemon_shutting_down",
                "message": "Loom daemon is shutting down",
                "retryable": true,
            }
        })
    );

    let mut response = Vec::new();
    write_response(&mut response, status, &body.to_string()).expect("write shutdown response");
    let response = String::from_utf8(response).expect("utf8 response");
    assert!(response.starts_with("HTTP/1.1 503 Service Unavailable"));
}

#[test]
fn serialized_routes_do_not_overlap_while_probes_remain_available() {
    let _guard = lock_ignoring_poison(&ENV_LOCK);
    let root = unique_temp_dir("serialized-routes");

    let observer = SerializedRouteObserver::new();
    let mut daemon = LoomDaemon::bind(
        DaemonConfig::localhost(0)
            .with_bounded_request_executor(3, 4)
            .with_control_plane_root(&root),
    )
    .expect("bind daemon");
    Arc::get_mut(&mut daemon.runtime)
        .expect("exclusive daemon runtime")
        .serialized_route_observer = Some(Arc::clone(&observer));
    let port = daemon.local_addr().expect("address").port();
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx));
    let mut fixture = ConcurrencyTestFixture::new(shutdown_tx, server);
    let observer_for_cleanup = Arc::clone(&observer);
    fixture.add_release_action(move || observer_for_cleanup.release());

    let first_rx = fixture.spawn_client(move || http_get(port, "/v1/workflows"));
    assert!(
        observer.wait_until_entered(Duration::from_millis(750)),
        "serialized route did not enter before the deadline"
    );
    let second_rx = fixture.spawn_client(move || http_get(port, "/v1/workflows"));

    let health = http_get(port, "/health");
    assert!(health.starts_with("HTTP/1.1 200 OK"));

    fixture.release_gates();
    let first_response = first_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("first serialized route");
    let second_response = second_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("second serialized route");
    fixture.finish().expect("serve");
    assert!(first_response.starts_with("HTTP/1.1 200 OK"));
    assert!(second_response.starts_with("HTTP/1.1 200 OK"));
    assert_eq!(observer.max_active(), 1);
    fs::remove_dir_all(root).expect("cleanup serialized routes root");
}

#[test]
fn serialized_route_observer_wait_is_bounded() {
    let observer = SerializedRouteObserver::new();
    assert!(!observer.wait_until_entered(Duration::from_millis(25)));
}
