// Independent wire oracle for the four pinned-peer routes through the real HTTPS adapter.
use super::*;

fn message(challenge: &Value, purpose: &str) -> Vec<u8> {
    format!(
        "loom.offline-peer.v1\n{purpose}\n{}\n{}\n{}\n{}",
        challenge["sourceId"].as_str().unwrap(),
        challenge["targetId"].as_str().unwrap(),
        challenge["nonce"].as_str().unwrap(),
        challenge["timestampMs"].as_u64().unwrap()
    )
    .into_bytes()
}
fn challenge(key: &SigningKeyDocument, a: &Value, b: &Value, purpose: &str) -> Value {
    let mut value = json!({"sourceId":a["identity"]["peerId"],"targetId":b["identity"]["peerId"],
        "nonce":Uuid::new_v4().simple().to_string(),"timestampMs":unix_time_millis(),"signature":""});
    value["signature"] = json!(sign_message(key, &message(&value, purpose)).unwrap());
    value
}
fn check(reply: &Value, input: &Value, peer: &Value, purpose: &str, suffix: &str) {
    assert_eq!(reply["challenge"], *input);
    let mut bytes = message(input, purpose);
    bytes.extend_from_slice(suffix.as_bytes());
    loom_plugin_security::verify_message(
        peer["identity"]["publicKey"].as_str().unwrap(),
        &bytes,
        reply["signature"].as_str().unwrap(),
    )
    .unwrap_or_else(|error| panic!("HTTPS peer response purpose {purpose}: {error:?}"));
}
fn post(tls: &Https, operation: &str, input: Value) -> Value {
    let reply = tls.post(None, &format!("/v1/projection-peer/{operation}"), input);
    assert_eq!(reply.0, 200, "{operation}: {reply:?}");
    reply.1
}
pub(super) fn verify(
    root: &Path,
    source_root: &Path,
    tls: &Https,
    a: &Value,
    b: &Value,
    target: &str,
    envelope: &ProjectionEnvelope,
) {
    // Only this test's newly created synthetic root is read; private keys never enter receipts.
    let stored: Value = serde_json::from_slice(
        &fs::read(source_root.join("settings/offline-projection-peers.json")).unwrap(),
    )
    .unwrap();
    let key: SigningKeyDocument = serde_json::from_value(stored["identity"].clone()).unwrap();
    let input = challenge(&key, a, b, "request");
    let mut tampered = input.clone();
    tampered["nonce"] = json!("0".repeat(32));
    assert_eq!(
        tls.post(None, "/v1/projection-peer/handshake", tampered).0,
        403
    );
    let reply = post(tls, "handshake", input.clone());
    check(&reply, &input, b, "response", "");
    assert_eq!(reply["deliveryAvailable"], false);
    let replay = tls.post(None, "/v1/projection-peer/handshake", input);
    assert_eq!(replay.0, 409);
    assert_eq!(error_code(&replay), "peer_replayed");

    let input = challenge(&key, a, b, "catalog-request");
    let reply = post(tls, "catalog", input.clone());
    assert_eq!(
        reply["expiresAtMs"].as_u64().unwrap(),
        input["timestampMs"].as_u64().unwrap() + 5000
    );
    assert_eq!(reply["transferProtocol"], "loom.offline-transfer.v1");
    assert!(reply["devices"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["deviceId"] == target && v["policy"] == "confirm"));
    let bytes = serde_json::to_vec(&json!([
        reply["devices"],
        reply["users"],
        reply["expiresAtMs"],
        reply["transferProtocol"]
    ]))
    .unwrap();
    // Catalog appends its digest after the challenge, not inside the purpose like raster/transfer.
    check(
        &reply,
        &input,
        b,
        "catalog-response",
        &format!("\n{}", sha256_bytes(&bytes)),
    );

    let snapshot = png(22);
    let bytes = BASE64.decode(&snapshot.image_base64).unwrap();
    let digest = sha256_bytes(&bytes);
    let raster = json!({"digest":digest,"width":snapshot.width,"height":snapshot.height,"byteLength":bytes.len()});
    let suffix = format!(
        "{digest}\n{}\n{}\n{}\nnot-delivered\nnot-retained",
        snapshot.width,
        snapshot.height,
        bytes.len()
    );
    let input = challenge(&key, a, b, &format!("raster-request\n{suffix}"));
    let reply = post(
        tls,
        "raster-check",
        json!({"challenge":input,"raster":raster,"snapshot":snapshot}),
    );
    check(&reply, &input, b, &format!("raster-response\n{suffix}"), "");
    assert_eq!(reply["raster"], raster);
    assert_eq!(reply["deliveryAvailable"], false);
    assert_eq!(reply["retained"], false);

    // Replay the identical accepted offer, not a second invitation or a copied delivery implementation.
    let path = source_root.join("offline-projections").join(format!(
        "{}.json",
        sha256_bytes(envelope.projection_id.as_bytes())
    ));
    let record: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(record["envelope"], serde_json::to_value(envelope).unwrap());
    let mut body = json!({});
    for field in [
        "envelope",
        "snapshot",
        "sourceKey",
        "sourceName",
        "sourceEpoch",
        "targetId",
    ] {
        body[field] = record[field].clone();
    }
    let payload = json!({"action":"offer","body":body});
    let purpose = format!(
        "transfer-request\n{}",
        sha256_bytes(&serde_json::to_vec(&payload).unwrap())
    );
    let input = challenge(&key, a, b, &purpose);
    let reply = post(
        tls,
        "transfer",
        json!({"challenge":input,"payload":payload}),
    );
    let purpose = format!(
        "transfer-response\n{}",
        sha256_bytes(&serde_json::to_vec(&reply["payload"]).unwrap())
    );
    check(&reply, &input, b, &purpose, "");
    assert_eq!(
        reply["payload"],
        json!({"targetEpoch":record["targetEpoch"]})
    );
    write_once(
        &root.join("peer-https-proof.json"),
        &serde_json::to_vec(&json!({
            "verifiedRoutes":["handshake","catalog","raster-check","transfer"],
            "tlsCertificateVerified":true,"pinnedResponseSignaturesVerified":true,
            "tamperingRejected":true,"replayRejected":true,"offerIdempotent":true,
            "deliveryAvailable":false,"rasterRetained":false
        }))
        .unwrap(),
    );
}
