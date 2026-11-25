//! 独立原始 HTTP 请求 oracle，不用路由实现生成允许列表。
use super::*;
const DEVICE: &str = "Authorization: Device fixture\r\nX-Loom-Device-Nonce: wall-unique\r\n";
fn parse(method: &str, path: &str, headers: &str) -> Result<Request> {
    let bytes = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:8766\r\n{headers}\r\n");
    read_request(&mut bytes.as_bytes(), "127.0.0.1:8766")
}
#[test]
fn wall_lan_exact_device_routes_do_not_grant_management_or_other_tile_capabilities() {
    assert!(parse("GET", "/v1/walls/state", DEVICE).is_ok());
    for path in [
        "/v1/walls/endpoints/register",
        "/v1/walls/endpoints/identify/report",
        "/v1/walls/connect",
        "/v1/walls/heartbeat",
        "/v1/walls/disconnect",
        "/v1/walls/images/read",
        "/v1/walls/surfaces/open",
        "/v1/walls/surfaces/state",
        "/v1/walls/surfaces/image",
        "/v1/walls/surfaces/close",
        "/v1/walls/surfaces/event",
        "/v1/walls/surfaces/confirmation",
        "/v1/walls/surfaces/cancel",
    ] {
        assert!(parse(
            "POST",
            path,
            &format!("{DEVICE}Content-Type: application/json\r\n")
        )
        .is_ok());
        assert_eq!(body_limit(path), 512 * 1024);
        for alias in [
            format!("{path}/"),
            format!("{path}?admin=1"),
            path.replace("walls", "%77alls"),
        ] {
            assert!(parse("POST", &alias, DEVICE).is_err());
        }
    }
    for path in [
        "/v1/walls/layouts",
        "/v1/walls/presentation",
        "/v1/walls/endpoints/remove",
        "/v1/walls/endpoints/identify",
        "/v1/walls/live/media",
        "/v1/walls/input",
        "/v1/surfaces/resources",
    ] {
        for method in ["GET", "POST", "PUT", "DELETE"] {
            assert!(parse(method, path, DEVICE).is_err(), "{method} {path}");
        }
    }
}
#[test]
fn wall_lan_requires_device_nonce_and_preserves_http_bounds() {
    for auth in [
        "",
        "Authorization: Bearer admin\r\n",
        "Authorization: Device fixture\r\n",
        "X-Loom-Device-Nonce: x\r\n",
        "Authorization: Device \r\nX-Loom-Device-Nonce: x\r\n",
    ] {
        assert!(parse("GET", "/v1/walls/state", auth).is_err());
    }
    for headers in [
        "Origin: https://localhost\r\n",
        "Cookie: x=1\r\n",
        "Upgrade: websocket\r\n",
        "Transfer-Encoding: chunked\r\n",
        "Expect: 100-continue\r\n",
        "Content-Length: 524289\r\n",
    ] {
        assert!(parse(
            "POST",
            "/v1/walls/heartbeat",
            &format!("{DEVICE}Content-Type: application/json\r\n{headers}")
        )
        .is_err());
    }
    assert!(parse(
        "GET",
        "/v1/walls/state",
        &format!("{DEVICE}Content-Length: 1\r\n")
    )
    .is_err());
}
#[test]
fn wall_media_lan_query_and_auth_are_exact_and_do_not_grant_surface_viewer() {
    let path="/v1/walls/live/media?endpointId=tile&leaseId=lease&revision=2&sessionId=live%3Asource&format=raw_bgra";
    let upgrade=format!("{DEVICE}Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Protocol: loom.wall.media.v1\r\n");
    assert!(parse("GET", path, &upgrade).is_ok());
    assert!(parse("GET", &path.replace("raw_bgra", "png"), &upgrade).is_ok());
    for bad in [
        format!("{path}&role=viewer"),
        format!("{path}&deviceId=other"),
        format!("{path}&revision=2"),
        format!("{path}&maxWidth=10"),
        path.replace("revision=2", "revision=+2"),
        path.replace("endpointId", "%65ndpointId"),
        path.replace("format=raw_bgra", "format=png&maxHeight=721"),
        path.replace("format=raw_bgra", "format=png&maxFps=11"),
        path.replace("sessionId=live%3Asource", "sessionId=live%253Asource"),
        path.replace("&leaseId=lease", ""),
    ] {
        assert!(parse("GET", &bad, &upgrade).is_err(), "{bad}");
    }
    for headers in [
        upgrade.replace("Device fixture", "Bearer fixture"),
        upgrade.replace("X-Loom-Device-Nonce: wall-unique\r\n", ""),
        upgrade.replace("loom.wall.media.v1", "loom.live.v1"),
        upgrade.replace("loom.wall.media.v1", "loom.wall.media.v1, another-protocol"),
        upgrade.replace("dGhlIHNhbXBsZSBub25jZQ==", "bad"),
        format!("{upgrade}Origin: https://localhost\r\n"),
        format!("{upgrade}Content-Length: 1\r\n"),
    ] {
        assert!(parse("GET", path, &headers).is_err());
    }
    assert!(parse(
        "POST",
        path,
        &format!("{upgrade}Content-Type: application/json\r\n")
    )
    .is_err());
}
