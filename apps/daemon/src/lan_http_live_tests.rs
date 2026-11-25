//! LAN ingress boundary tests use raw requests, independently of the HTTP client.
use super::*;

fn parse(method: &str, path: &str, extra: &str) -> Result<Request> {
    let bytes = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:8766\r\n{extra}\r\n");
    read_request(&mut bytes.as_bytes(), "127.0.0.1:8766")
}

const MEDIA: &str = "/v1/live/media?sessionId=live%3AlAN%2Fpart&role=viewer";
const UPGRADE: &str = "Authorization: Device test-session\r\nX-Loom-Device-Nonce: unique-1\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Protocol: loom.live.v1\r\n";

#[test]
fn live_lan_routes_are_exact_bounded_and_keep_encoded_session_identity() {
    for route in [
        "/v1/live/status",
        "/v1/live/sessions",
        "/v1/live/sessions/live:test%2Fpart",
        "/v1/live/sessions/live:test%2Fpart/events?after=0&timeoutMs=750",
    ] {
        assert!(parse("GET", route, "").is_ok(), "{route}");
    }
    for suffix in [
        "viewers",
        "resume",
        "control",
        "input",
        "observations",
        "triggers",
        "close",
    ] {
        let path = format!("/v1/live/sessions/live:test%2Fpart/{suffix}");
        assert!(parse("POST", &path, "Content-Type: application/json\r\n").is_ok());
        assert!(parse("GET", &path, "").is_err());
        assert!(parse(
            "POST",
            &(path + "?admin=true"),
            "Content-Type: application/json\r\n"
        )
        .is_err());
    }
    assert_eq!(
        crate::live_session_path_id("/v1/live/sessions/live:test%2Fpart").as_deref(),
        Some("live:test/part")
    );
    assert!(crate::live_session_suffix_id(
        "/v1/live/sessions/live:test%252Fpart/events",
        "/events"
    )
    .is_none());
    for path in [
        "/v1/live/status?x=1",
        "/v1/live/sessions/../devices",
        "/v1/live/sessions/%2e%2e",
        "/v1/live/sessions/live:x/unknown",
        "/v1/live/sessions/live:x/events?after=0&after=1",
        "/v1/live/sessions/live:x/events?timeoutMs=-1",
        "/v1/live/sessions/live:x/events?after=18446744073709551616",
        "/v1/live/sessions/live:x/events?%61fter=1",
        "/v1/live/sessions/live:x/events?admin=1",
        "/v1/live/sessions/live:x%ZZ",
    ] {
        assert!(parse("GET", path, "").is_err(), "{path}");
    }
    assert!(parse("GET", "/v1/live/status", "Content-Length: 1\r\n").is_err());
    assert!(parse(
        "POST",
        "/v1/live/sessions",
        "Content-Type: application/json\r\nContent-Length: 1048577\r\n"
    )
    .is_err());
}

#[test]
fn live_lan_upgrade_requires_device_nonce_and_no_browser_or_body() {
    let request = parse("GET", MEDIA, UPGRADE).unwrap();
    let request =
        super::super::lan_http_live::upgrade_request(request, "127.0.0.1:8765".parse().unwrap());
    assert_eq!(request.header("host"), Some("127.0.0.1:8765"));
    assert_eq!(request.header("authorization"), Some("Device test-session"));
    assert!(crate::validate_live_websocket_headers(&request, "loom.live.v1").is_ok());
    for extra in [
        "Cookie: loom_admin=secret\r\n",
        "Origin: https://127.0.0.1:8766\r\n",
        "Transfer-Encoding: chunked\r\n",
        "Expect: 100-continue\r\n",
        "Content-Length: 1\r\n",
        "Host: evil\r\n",
    ] {
        assert!(parse("GET", MEDIA, &(UPGRADE.to_owned() + extra)).is_err());
    }
    for bad in [
        UPGRADE.replace("Device test-session", "Bearer admin"),
        UPGRADE.replace("Authorization: Device test-session\r\n", ""),
        UPGRADE.replace("X-Loom-Device-Nonce: unique-1\r\n", ""),
        UPGRADE.replace("dGhlIHNhbXBsZSBub25jZQ==", "AA=="),
    ] {
        assert!(parse("GET", MEDIA, &bad).is_err());
    }
    for path in [
        "/v1/live/media",
        "/v1/walls/live/media?sessionId=x&role=viewer",
        "/v1/live/media?sessionId=x&role=admin",
        "/v1/live/media?sessionId=x&role=viewer&deviceId=admin",
        "/v1/live/media?sessionId=x&role=viewer&role=source",
        "/v1/live/media?sessionId=x&role=viewer&afterEpoch=+1",
    ] {
        assert!(parse("GET", path, UPGRADE).is_err(), "{path}");
    }
    assert!(parse("GET", "/v1/otter/identity", UPGRADE).is_err());
}
