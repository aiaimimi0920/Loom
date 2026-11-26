// Image-input requests traverse the real Device, issuer, task table, and Capability process against an independent byte oracle.
fn image_bytes() -> Vec<u8> {
    BASE64.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a6x0AAAAASUVORK5CYII=").unwrap()
}
fn image_request(f: &Fixture, id: &str, input: Value) -> Value {
    let call = f.service_submit(id, input, 5000);
    json!({"protocolVersion":"loom.art.local.service.images.v1","call":call,"images":[{"png":BASE64.encode(image_bytes())}]})
}
fn image_fixture() -> Fixture {
    let package = package("local.js", &sha256_bytes(SOURCE), &[SERVICE_ID], SOURCE);
    let f = Fixture::build_package_images(&package, true, true);
    assert_eq!(f.approve(0, true).0, 200);
    f
}
fn wait_image_cleanup(f: &Fixture) {
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        if fs::read_dir(f.root.join("capability-resources/active"))
            .unwrap()
            .count()
            == 0
            && f.resources.lock().unwrap().active_leases_for_test() == 0
        {
            break;
        }
        assert!(Instant::now() < end, "staged resources not reclaimed");
        thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn local_art_service_images_http_reads_exact_staged_bytes_once_and_releases_leases() {
    let f = image_fixture();
    let submit = image_request(&f, "call:image", json!({"readImage":true}));
    assert_eq!(f.service("submit-images", submit.clone()).0, 200);
    let reply = f.service_final(&submit["call"]);
    assert_eq!(reply["status"], "succeeded", "{reply}");
    let hex = image_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(reply["output"]["pngHex"], hex);
    assert_eq!(reply["output"]["calls"], 1);
    assert_eq!(f.service("submit-images", submit.clone()).1, reply);
    let mut changed = submit.clone();
    changed["images"][0]["png"] = json!(BASE64.encode(b"different"));
    assert_eq!(f.service("submit-images", changed).0, 409);
    assert_eq!(f.service("submit", submit["call"].clone()).0, 409);
    wait_image_cleanup(&f);
    let mut invalid = submit;
    invalid["call"]["owner"]["callId"] = json!("call:bad-png");
    invalid["images"][0]["png"] = json!(BASE64.encode(b"not a PNG"));
    assert_eq!(f.service("submit-images", invalid.clone()).0, 200);
    assert_eq!(f.service_final(&invalid["call"])["status"], "failed");
    wait_image_cleanup(&f);
}
#[test]
fn local_art_service_images_http_permission_wire_and_owner_cannot_be_bypassed() {
    let f = Fixture::with_service();
    assert_eq!(f.approve(0, true).0, 200);
    let submit = image_request(&f, "call:no-image-permission", json!({"readImage":true}));
    assert_eq!(f.service("submit-images", submit.clone()).0, 200);
    assert_eq!(f.service_final(&submit["call"])["status"], "failed");
    wait_image_cleanup(&f);
    assert_eq!(f.service("submit", submit.clone()).0, 400);
    let mut invalid = submit.clone();
    invalid["call"]["grant"]["signature"] = json!("invalid");
    assert_eq!(f.service("submit-images", invalid).0, 403);
    let mut invalid = submit.clone();
    invalid["images"][0]["path"] = json!("c:/secret");
    assert_eq!(f.service("submit-images", invalid).0, 400);
    let mut invalid = submit.clone();
    invalid["images"] = json!([]);
    assert_eq!(f.service("submit-images", invalid).0, 413);
    let mut invalid = submit;
    invalid["call"]["owner"]["binding"]["generation"] = json!(2);
    assert_eq!(f.service("submit-images", invalid).0, 403);
}
#[test]
fn local_art_service_images_http_cancel_and_revoke_are_terminal_and_cleanup_staging() {
    let f = image_fixture();
    let submit = image_request(&f, "call:image-cancel", json!({"hang":true}));
    assert_eq!(f.service("submit-images", submit.clone()).0, 200);
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        if f.service("poll", Fixture::service_control(&submit["call"]))
            .1["progress"]["step"]
            == 1
        {
            break;
        }
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        f.service("cancel", Fixture::service_control(&submit["call"]))
            .1["status"],
        "cancelled"
    );
    wait_image_cleanup(&f);
    assert_eq!(f.service_final(&submit["call"])["status"], "cancelled");
    assert_eq!(f.approve(1, false).0, 200);
    assert_eq!(f.service("submit-images", submit).0, 403);
}
#[test]
fn local_art_service_images_transport_limits_are_exact_and_large_input_reaches_original_task() {
    let path = "/v1/art/local/service/submit-images";
    let maximum = loom_protocol::local_art::service::images::MAX_REQUEST;
    let header = |route: &str, length: usize| {
        format!("POST {route} HTTP/1.1\r\nContent-Length: {length}\r\n\r\n").into_bytes()
    };
    assert!(!request_exceeds_size_limit(&header(path, maximum)));
    assert!(request_exceeds_size_limit(&header(path, maximum + 1)));
    assert!(request_exceeds_size_limit(&header(
        &format!("{path}?admin=true"),
        MAX_HTTP_BODY_BYTES + 1
    )));
    let f = image_fixture();
    let mut submit = image_request(&f, "call:large-image", json!({}));
    let mut bytes = image_bytes();
    bytes.resize(1024 * 1024, 0);
    submit["images"][0]["png"] = json!(BASE64.encode(bytes));
    assert_eq!(f.service("submit-images", submit.clone()).0, 200);
    assert_eq!(f.service_final(&submit["call"])["status"], "succeeded");
    wait_image_cleanup(&f);
}
