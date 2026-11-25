//! Offline LAN exposure preserves exact routes, original budgets and independent authentication.
use super::*;

#[test]
fn offline_lan_routes_reject_management_aliases_and_oversized_bodies() {
    for name in [
        "edit", "create", "inbox", "inspect", "accept", "read", "update", "receipt", "unlink",
    ] {
        let path = format!("/v1/offline-projections/{name}");
        boundary(
            &path,
            if matches!(name, "create" | "update") {
                loom_protocol::projection::MAX_PROJECTION_HTTP_BYTES
            } else if name == "edit" {
                256 * 1024
            } else {
                MAX_BODY
            },
        );
    }
    for name in ["handshake", "catalog", "raster-check", "transfer"] {
        let path = format!("/v1/projection-peer/{name}");
        boundary(
            &path,
            if matches!(name, "handshake" | "catalog") {
                16 * 1024
            } else {
                loom_protocol::projection::MAX_PROJECTION_HTTP_BYTES
            },
        );
    }
    for path in [
        "/v1/projection-peers",
        "/v1/projection-peers/probe",
        "/v1/projection-peers/raster-check",
        "/v1/projection-peer",
        "/v1/offline-projections/admin",
        "/v1/offline-projections/targets",
        "/v1/offline-projections/create/../inbox",
    ] {
        for method in ["GET", "POST", "PUT", "DELETE"] {
            assert!(!allowed(method, path));
        }
    }
}

#[test]
fn account_projection_lan_routes_match_local_operations_and_raster_budgets() {
    for name in [
        "context", "create", "inspect", "accept", "update", "read", "unlink",
    ] {
        boundary(
            &format!("/v1/projections/v2/{name}"),
            if matches!(name, "create" | "update") {
                loom_protocol::projection::MAX_PROJECTION_HTTP_BYTES
            } else {
                MAX_BODY
            },
        );
    }
    for name in [
        "configuration",
        "publish",
        "sync",
        "peer",
        "admin",
        "create/../read",
    ] {
        assert!(!allowed("POST", &format!("/v1/projections/v2/{name}")));
    }
}

fn boundary(path: &str, limit: usize) {
    assert!(allowed("POST", path));
    for method in ["GET", "PUT", "DELETE", "OPTIONS"] {
        assert!(!allowed(method, path));
    }
    for suffix in ["?admin=true", "/", "%2fadmin"] {
        assert!(!allowed("POST", &format!("{path}{suffix}")));
    }
    assert_eq!(body_limit(path), limit);
    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:8766\r\nContent-Type: application/json\r\n"
    );
    for extra in [
        "Authorization: Bearer admin\r\n".into(),
        "Origin: https://evil\r\n".into(),
        "Transfer-Encoding: chunked\r\n".into(),
        format!("Content-Length: {}\r\n", limit + 1),
    ] {
        assert!(read_request(
            &mut format!("{head}{extra}\r\n").as_bytes(),
            "127.0.0.1:8766"
        )
        .is_err());
    }
    // Missing credentials may pass the adapter, but must be rejected by device or signed-peer owners.
    assert!(read_request(
        &mut format!("{head}Content-Length: 2\r\n\r\n{{}}").as_bytes(),
        "127.0.0.1:8766"
    )
    .is_ok());
}
