#[test]
fn offline_raster_http_accepts_png_above_default_json_budget() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let ra = ProjectionRoot::new(); let rb = ProjectionRoot::new();
    let (a, mut sa) = start(&ra.0); let (b, mut sb) = start(&rb.0);
    let va = peer_view(a); let vb = peer_view(b); trust_peer(a, &vb, b); trust_peer(b, &va, a);
    let mut state = 17_u32;
    let image = image::RgbaImage::from_fn(768, 512, |_, _| {
        state ^= state << 13; state ^= state >> 17; state ^= state << 5;
        image::Rgba(state.to_le_bytes())
    });
    let mut output = std::io::Cursor::new(Vec::new());
    image.write_to(&mut output, image::ImageFormat::Png).unwrap();
    let bytes = output.into_inner(); assert!(bytes.len() > 1024 * 1024);
    let reply = peer_admin(a, "POST", "/v1/projection-peers/raster-check", json!({
        "peerId": vb["identity"]["peerId"], "snapshot": {"imageBase64": BASE64.encode(&bytes), "width": 768, "height": 512}}));
    assert_eq!(reply.0, 200, "{}", reply.1);
    assert_eq!(reply.1["raster"]["digest"], sha256_bytes(&bytes));
    sa.finish().unwrap(); sb.finish().unwrap();
}

#[test]
fn offline_raster_http_transfers_real_png_and_rejects_revoked_peer() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let ra = ProjectionRoot::new(); let rb = ProjectionRoot::new();
    let (a, mut sa) = start(&ra.0); let (b, mut sb) = start(&rb.0);
    let va = peer_view(a); let vb = peer_view(b);
    trust_peer(a, &vb, b); trust_peer(b, &va, a);
    for (source, target) in [(a, &vb), (b, &va)] {
        let snapshot = png(43);
        let bytes = BASE64.decode(&snapshot.image_base64).unwrap();
        let reply = peer_admin(source, "POST", "/v1/projection-peers/raster-check",
            json!({"peerId": target["identity"]["peerId"], "snapshot": snapshot}));
        assert_eq!(reply.0, 200, "{}", reply.1);
        assert_eq!(reply.1["rasterVerified"], true);
        assert_eq!(reply.1["raster"]["digest"], sha256_bytes(&bytes));
        assert_eq!(reply.1["raster"]["byteLength"], bytes.len());
        assert_eq!(reply.1["deliveryAvailable"], false);
        assert_eq!(reply.1["retained"], false);
    }
    assert_eq!(public(a, "/v1/projection-peers/raster-check", json!({})).0, 403);
    assert_eq!(public(b, "/v1/projection-peer/raster-check", json!({})).0, 400);
    let hook = Identity::pair(a, "Not a peer administrator");
    let headers = format!("Authorization: Device {}\r\nX-Loom-Device-Nonce: {}\r\n", hook.token, Uuid::new_v4());
    assert_eq!(response(http_request_with_extra_headers(a, "POST", "/v1/projection-peers/raster-check", Some("{}"), &headers)).0, 403);
    assert_eq!(peer_admin(b, "DELETE", "/v1/projection-peers", json!({"expectedRevision": peer_view(b)["revision"],
        "peerId": va["identity"]["peerId"]})).0, 200);
    assert_eq!(peer_admin(a, "POST", "/v1/projection-peers/raster-check", json!({
        "peerId": vb["identity"]["peerId"], "snapshot": png(43)})).0, 502);
    sa.finish().unwrap(); sb.finish().unwrap();
}

#[test]
fn offline_raster_http_authenticates_content_and_rejects_replay() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let ra = ProjectionRoot::new(); let rb = ProjectionRoot::new();
    let (a, mut sa) = start(&ra.0); let (b, mut sb) = start(&rb.0);
    let va = peer_view(a); let vb = peer_view(b); trust_peer(b, &va, a);
    let stored: Value = serde_json::from_slice(&fs::read(ra.0.join("settings/offline-projection-peers.json")).unwrap()).unwrap();
    let key: SigningKeyDocument = serde_json::from_value(stored["identity"].clone()).unwrap();
    let snapshot = png(19); let bytes = BASE64.decode(&snapshot.image_base64).unwrap();
    let digest = sha256_bytes(&bytes); let nonce = Uuid::new_v4().simple().to_string(); let now = unix_time_millis();
    let purpose = format!("raster-request\n{digest}\n{}\n{}\n{}\nnot-delivered\nnot-retained", snapshot.width, snapshot.height, bytes.len());
    let message = format!("loom.offline-peer.v1\n{purpose}\n{}\n{}\n{nonce}\n{now}",
        va["identity"]["peerId"].as_str().unwrap(), vb["identity"]["peerId"].as_str().unwrap());
    let request = json!({"challenge": {"sourceId": va["identity"]["peerId"], "targetId": vb["identity"]["peerId"],
        "nonce": nonce, "timestampMs": now, "signature": sign_message(&key, message.as_bytes()).unwrap()},
        "raster": {"digest": digest, "width": snapshot.width, "height": snapshot.height, "byteLength": bytes.len()}, "snapshot": snapshot});
    let mut metadata = request.clone(); metadata["raster"]["width"] = json!(8);
    assert_eq!(public(b, "/v1/projection-peer/raster-check", metadata).0, 403);
    let mut corrupt = request.clone(); corrupt["snapshot"] = json!(png(20));
    assert_eq!(public(b, "/v1/projection-peer/raster-check", corrupt).0, 400);
    // Even a failed authenticated request consumes the nonce; no decoder replay loop.
    assert_eq!(public(b, "/v1/projection-peer/raster-check", request).0, 409);
    sa.finish().unwrap(); sb.finish().unwrap();
}
