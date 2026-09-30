use crate::{AccountSession, Identity, Peer, Snapshot, Transport};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use sha2::{Digest as _, Sha256};

const NOW: u64 = 100;

fn identity(device_id: &str) -> Identity {
    let key = SigningKey::generate(&mut OsRng);
    Identity::new(
        "http://127.0.0.1:40100".to_owned(),
        AccountSession {
            protocol: "neuro.loom-account.v1".to_owned(),
            device_id: device_id.to_owned(),
            account_id: "account:transport".to_owned(),
            username: "fixture".to_owned(),
            device_name: "transport fixture".to_owned(),
            public_key: STANDARD.encode(key.verifying_key().as_bytes()),
            expires_at_ms: NOW + 600_000,
        },
        key,
        NOW,
    )
    .unwrap()
}

#[tokio::test]
async fn authenticated_loopback_transfer_checks_remote_identity_and_digest() {
    let source_identity = identity("00000000-0000-4000-8000-000000000011");
    let receiver_identity = identity("00000000-0000-4000-8000-000000000012");
    let source = Transport::bind_loopback_for_test(&source_identity)
        .await
        .unwrap();
    let receiver = Transport::bind_loopback_for_test(&receiver_identity)
        .await
        .unwrap();
    let source_peer = Peer {
        device_id: source_identity.session().device_id.clone(),
        public_key: source_identity.session().public_key.clone(),
        device_name: source_identity.session().device_name.clone(),
        endpoint: Some(source.endpoint_address()),
    };
    let receiver_peer = Peer {
        device_id: receiver_identity.session().device_id.clone(),
        public_key: receiver_identity.session().public_key.clone(),
        device_name: receiver_identity.session().device_name.clone(),
        endpoint: Some(receiver.endpoint_address()),
    };
    let png = STANDARD
        .decode(crate::runtime_tests::image(1).image_base64)
        .unwrap();
    let digest = format!("{:x}", Sha256::digest(&png));
    let snapshot = Snapshot {
        projection_id: "projection:11111111111111111111111111111111".to_owned(),
        source_session_id: "source-session".to_owned(),
        revision: 1,
        digest,
        width: 2,
        height: 2,
        png,
    };
    let receive_task = tokio::spawn(async move {
        receiver
            .receive_snapshot(
                &source_peer,
                "projection:11111111111111111111111111111111",
                "source-session",
                1,
            )
            .await
    });
    let send_result = source.send_snapshot(&receiver_peer, snapshot.clone()).await;
    let receive_result = receive_task.await.unwrap();
    send_result.unwrap();
    let received = receive_result.unwrap();
    assert_eq!(received, snapshot);
    source.close().await;
}
