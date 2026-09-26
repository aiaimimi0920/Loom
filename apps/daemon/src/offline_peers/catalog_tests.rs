use super::*;

fn fixture() -> (Peer, SigningKeyDocument, CatalogProof) {
    let key = generate_signing_key("peer");
    let id = trust::peer_id(&key.public_key).unwrap_or_else(|_| panic!("fixture key"));
    let peer = Peer {
        peer_id: id.clone(),
        public_key: key.public_key.clone(),
        name: "B".to_owned(),
        origin: "https://b.example.test".to_owned(),
        enabled: true,
    };
    let challenge = Challenge {
        source_id: format!("loom-{}", "a".repeat(64)),
        target_id: id,
        nonce: "a".repeat(32),
        timestamp_ms: unix_time_millis(),
        signature: "A".repeat(88),
    };
    let expires_at_ms = challenge.timestamp_ms + CATALOG_TTL_MS;
    let mut proof = CatalogProof {
        challenge,
        expires_at_ms,
        transfer_protocol: "loom.offline-transfer.v1".to_owned(),
        signature: String::new(),
        devices: vec![CatalogDevice {
            device_id: "hook-b".to_owned(),
            name: "Receiver".to_owned(),
            policy: "auto".to_owned(),
        }],
    };
    sign(&key, &mut proof);
    (peer, key, proof)
}

fn sign(key: &SigningKeyDocument, proof: &mut CatalogProof) {
    proof.signature =
        sign_message(key, &proof.message().unwrap_or_else(|_| panic!("message"))).unwrap();
}

#[test]
fn offline_catalog_signature_binds_device_metadata_and_purpose() {
    let (peer, key, mut proof) = fixture();
    assert!(proof.verify(&peer, &proof.challenge).is_ok());
    proof.devices[0].name = "Forged name".to_owned();
    assert!(proof.verify(&peer, &proof.challenge).is_err());
    proof.signature = sign_message(&key, &proof.challenge.message("response")).unwrap();
    assert!(proof.verify(&peer, &proof.challenge).is_err());
}

#[test]
fn offline_catalog_rejects_expired_duplicate_and_over_budget_signed_payloads() {
    let (peer, key, mut proof) = fixture();
    proof.devices.push(proof.devices[0].clone());
    sign(&key, &mut proof);
    assert!(proof.verify(&peer, &proof.challenge).is_err());
    proof.devices = (0..65)
        .map(|i| CatalogDevice {
            device_id: format!("hook-{i}"),
            name: "Hook".to_owned(),
            policy: "confirm".to_owned(),
        })
        .collect();
    sign(&key, &mut proof);
    assert!(proof.verify(&peer, &proof.challenge).is_err());
    proof.devices.clear();
    proof.challenge.timestamp_ms -= 6_000;
    proof.expires_at_ms = proof.challenge.timestamp_ms + CATALOG_TTL_MS;
    sign(&key, &mut proof);
    assert!(proof.verify(&peer, &proof.challenge).is_err());
}
