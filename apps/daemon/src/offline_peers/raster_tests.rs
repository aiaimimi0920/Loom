use super::*;

fn fixture() -> (Peer, SigningKeyDocument, RasterRequest, RasterReceipt) {
    let key = generate_signing_key("peer");
    let peer = Peer {
        peer_id: trust::peer_id(&key.public_key).unwrap_or_else(|_| panic!("key")),
        name: "B".into(),
        origin: "https://b.example.test".into(),
        public_key: key.public_key.clone(),
        enabled: true,
    };
    let challenge = Challenge {
        source_id: format!("loom-{}", "a".repeat(64)),
        target_id: peer.peer_id.clone(),
        nonce: "a".repeat(32),
        timestamp_ms: unix_time_millis(),
        signature: "A".repeat(88),
    };
    let raster = Raster {
        digest: "a".repeat(64),
        width: 1,
        height: 1,
        byte_length: 68,
    };
    let request = RasterRequest {
        challenge: challenge.clone(),
        raster: raster.clone(),
        snapshot: ProjectionSnapshot {
            image_base64: String::new(),
            width: 1,
            height: 1,
        },
    };
    let signature = sign_message(&key, &challenge.message(&raster.purpose(true))).unwrap();
    (
        peer,
        key,
        request,
        RasterReceipt {
            challenge,
            raster,
            signature,
            delivery_available: false,
            retained: false,
        },
    )
}

#[test]
fn offline_raster_receipt_binds_bytes_metadata_challenge_and_no_delivery() {
    let (peer, key, request, mut receipt) = fixture();
    assert!(receipt.verify(&peer, &request).is_ok());
    receipt.raster.byte_length += 1;
    assert!(receipt.verify(&peer, &request).is_err());
    receipt.raster = request.raster.clone();
    receipt.delivery_available = true;
    assert!(receipt.verify(&peer, &request).is_err());
    receipt.delivery_available = false;
    receipt.retained = true;
    assert!(receipt.verify(&peer, &request).is_err());
    receipt.retained = false;
    receipt.signature = sign_message(
        &key,
        &request.challenge.message(&request.raster.purpose(false)),
    )
    .unwrap();
    assert!(receipt.verify(&peer, &request).is_err());
    receipt.signature = sign_message(
        &key,
        &request.challenge.message(&request.raster.purpose(true)),
    )
    .unwrap();
    receipt.challenge.nonce = "b".repeat(32);
    assert!(receipt.verify(&peer, &request).is_err());
}

#[test]
fn offline_raster_receipt_rejects_expired_response_and_invalid_png() {
    let (peer, key, mut request, mut receipt) = fixture();
    request.challenge.timestamp_ms -= 31_000;
    receipt.challenge = request.challenge.clone();
    receipt.signature = sign_message(
        &key,
        &request.challenge.message(&request.raster.purpose(true)),
    )
    .unwrap();
    assert!(receipt.verify(&peer, &request).is_err());
    let bad = ProjectionSnapshot {
        image_base64: BASE64.encode(b"not a png"),
        width: 1,
        height: 1,
    };
    assert!(Raster::from_snapshot(&bad).is_err());
}
