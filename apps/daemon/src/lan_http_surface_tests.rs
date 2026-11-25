//! 原始请求构造的独立LAN边界oracle；真实Device/附件/生命周期另由双原生联合验证。
use super::*;
const DEVICE: &str =
    "Authorization: Device fixture-session\r\nX-Loom-Device-Nonce: unique-surface\r\n";
fn parse(method: &str, path: &str, extra: &str) -> Result<Request> {
    let bytes = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:8766\r\n{extra}\r\n");
    read_request(&mut bytes.as_bytes(), "127.0.0.1:8766")
}
#[test]
fn surface_lan_routes_allow_only_attach_lifecycle_and_device_stream() {
    for path in [
        "/v1/surfaces/attach",
        "/v1/surfaces/instances/surface:fixture/lifecycle",
    ] {
        assert!(parse(
            "POST",
            path,
            &format!("{DEVICE}Content-Type: application/json\r\n")
        )
        .is_ok());
        assert!(parse("GET", path, DEVICE).is_err());
        assert!(parse("POST", &(path.to_owned() + "?admin=1"), DEVICE).is_err());
        assert_eq!(body_limit(path), 16 * 1024);
    }
    for path in [
        "/v1/surfaces/stream",
        "/v1/surfaces/stream?after=0&timeoutMs=0",
        "/v1/surfaces/stream?timeoutMs=5000&after=17",
    ] {
        assert!(parse("GET", path, DEVICE).is_ok(), "{path}");
    }
    for path in [
        "/v1/surfaces",
        "/v1/surfaces/instances",
        "/v1/surfaces/instances/surface:x",
        "/v1/surfaces/instances/surface:x/mount",
        "/v1/surfaces/instances/surface:x/attachments",
        "/v1/surfaces/instances/surface:x/events",
        "/v1/surfaces/actions/cancel",
        "/v1/surfaces/resources/digest?leaseId=x",
        "/v1/surfaces/stream?deviceId=other",
        "/v1/surfaces/stream?after=1&after=2",
        "/v1/surfaces/stream?%61fter=1",
        "/v1/surfaces/stream?after=-1",
        "/v1/surfaces/stream?after=18446744073709551616",
        "/v1/surfaces/stream?timeoutMs=5001",
        "/v1/surfaces/stream?after=%30",
        "/v1/surfaces/stream?",
        "/v1/surfaces/stream?after=0&",
    ] {
        assert!(parse("GET", path, DEVICE).is_err(), "{path}");
        assert!(
            parse(
                "POST",
                path,
                &format!("{DEVICE}Content-Type: application/json\r\n")
            )
            .is_err(),
            "{path}"
        );
    }
    for id in [
        "",
        ".",
        "..",
        "%2e%2e",
        "surface:x/part",
        "surface:x%2Fpart",
        "surface:x\\part",
        "surface:x#bad",
    ] {
        assert!(parse(
            "POST",
            &format!("/v1/surfaces/instances/{id}/lifecycle"),
            &format!("{DEVICE}Content-Type: application/json\r\n")
        )
        .is_err());
    }
}
#[test]
fn surface_lan_requires_device_nonce_no_browser_and_original_bounds() {
    let path = "/v1/surfaces/attach";
    for auth in [
        "",
        "Authorization: Device \r\n",
        "Authorization: Bearer admin\r\n",
        "Authorization: Device fixture\r\n",
        "X-Loom-Device-Nonce: unique\r\n",
    ] {
        assert!(parse(
            "POST",
            path,
            &format!("{auth}Content-Type: application/json\r\n")
        )
        .is_err());
    }
    for extra in [
        "Cookie: private=1\r\n",
        "Origin: https://127.0.0.1:8766\r\n",
        "Upgrade: websocket\r\n",
        "Transfer-Encoding: chunked\r\n",
        "Expect: 100-continue\r\n",
        "Host: evil\r\n",
        "Content-Length: 16385\r\n",
    ] {
        assert!(parse(
            "POST",
            path,
            &format!("{DEVICE}Content-Type: application/json\r\n{extra}")
        )
        .is_err());
    }
    assert!(parse(
        "GET",
        "/v1/surfaces/stream",
        &format!("{DEVICE}Content-Length: 1\r\n")
    )
    .is_err());
}
