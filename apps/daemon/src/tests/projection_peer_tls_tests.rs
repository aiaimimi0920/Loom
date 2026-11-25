// Approved roots are peer-scoped and persistent; neither TLS nor signed identity is bypassed.
use super::*;

fn save(port: u16, peer: &Value) -> (u16, Value) {
    peer_admin(
        port,
        "PUT",
        "/v1/projection-peers",
        json!({
            "expectedRevision":peer_view(port)["revision"], "peer":peer,
        }),
    )
}

#[test]
fn offline_peer_https_requires_approved_certificate_and_survives_restart() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let ra = ProjectionRoot::new();
    let rb = ProjectionRoot::new();
    let (a, mut sa) = start(&ra.0);
    let (b, mut sb) = start(&rb.0);
    let ha = Https::enable(a);
    let hb = Https::enable(b);
    let va = peer_view(a);
    let vb = peer_view(b);
    assert_eq!(va["localConnection"]["origin"], ha.origin);
    assert_eq!(vb["localConnection"]["certificate"], hb.certificate);
    assert!(!va.to_string().contains("privateKey"));
    trust_peer(b, &va, a);
    let mut peer = json!({"peerId":vb["identity"]["peerId"], "publicKey":vb["identity"]["publicKey"],
        "name":"TLS B", "origin":hb.origin, "enabled":true});
    let probe = || {
        peer_admin(
            a,
            "POST",
            "/v1/projection-peers/probe",
            json!({"peerId":peer["peerId"]}),
        )
    };
    assert_eq!(save(a, &peer).0, 200);
    assert_eq!(probe().0, 502, "unknown self-signed root must fail");
    for encoded in [
        "".to_owned(),
        "?".to_owned(),
        BASE64.encode(b"not X.509"),
        "x".repeat(8193),
    ] {
        peer["certificate"] = json!(encoded);
        let rejected = save(a, &peer);
        assert_eq!(rejected.0, 400, "{rejected:?}");
        assert_eq!(error_code(&rejected), "peer_invalid_certificate");
    }
    peer["certificate"] = ha.certificate.clone();
    assert_eq!(save(a, &peer).0, 200);
    assert_eq!(
        peer_admin(
            a,
            "POST",
            "/v1/projection-peers/probe",
            json!({"peerId":peer["peerId"]})
        )
        .0,
        502,
        "a different peer's root must not authorize B"
    );
    peer["certificate"] = hb.certificate.clone();
    peer["origin"] = json!(hb.origin.replace("127.0.0.1", "localhost"));
    assert_eq!(save(a, &peer).0, 200);
    assert_eq!(
        peer_admin(
            a,
            "POST",
            "/v1/projection-peers/probe",
            json!({"peerId":peer["peerId"]})
        )
        .0,
        502,
        "approved root must not disable hostname validation"
    );
    peer["origin"] = json!(hb.origin);
    assert_eq!(save(a, &peer).0, 200);
    let probe = || {
        peer_admin(
            a,
            "POST",
            "/v1/projection-peers/probe",
            json!({"peerId":peer["peerId"]}),
        )
    };
    assert_eq!(probe().0, 200);
    sa.finish().unwrap();
    let (a, mut sa) = start(&ra.0);
    assert_eq!(peer_view(a)["peers"][0]["certificate"], hb.certificate);
    assert_eq!(
        peer_admin(
            a,
            "POST",
            "/v1/projection-peers/probe",
            json!({"peerId":peer["peerId"]})
        )
        .0,
        200
    );
    assert_eq!(
        peer_admin(
            b,
            "DELETE",
            "/v1/projection-peers",
            json!({
        "expectedRevision":peer_view(b)["revision"],"peerId":va["identity"]["peerId"]})
        )
        .0,
        200
    );
    assert_eq!(
        peer_admin(
            a,
            "POST",
            "/v1/projection-peers/probe",
            json!({"peerId":peer["peerId"]})
        )
        .0,
        502,
        "TLS trust must not replace mutual signed identity trust"
    );
    sa.finish().unwrap();
    sb.finish().unwrap();
}
