use super::operation::CentralOperation;
use super::response::{server_error, validate};
use crate::{
    AccountSession, CentralResponse, Content, Envelope, EnvelopeSignature, Identity, InitialImage,
    Peer, Record, Source, Status, View, PROTOCOL,
};
use ed25519_dalek::SigningKey;
use rand_core::OsRng;

const NOW: u64 = 100;
const ORIGIN: &str = "https://platform.example";

fn identity(device_id: &str, account_id: &str) -> Identity {
    let key = SigningKey::generate(&mut OsRng);
    let session = AccountSession {
        protocol: "neuro.loom-account.v1".to_owned(),
        device_id: device_id.to_owned(),
        account_id: account_id.to_owned(),
        username: "fixture".to_owned(),
        device_name: format!("device-{device_id}"),
        public_key: base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            key.verifying_key().as_bytes(),
        ),
        expires_at_ms: NOW + 600_000,
    };
    Identity::new(ORIGIN.to_owned(), session, key, NOW).unwrap()
}

fn invitation(identity: &Identity) -> Envelope {
    identity
        .invitation("sticker:source", "sticker", &"a".repeat(64), NOW)
        .unwrap()
}

fn record(identity: &Identity) -> Record {
    let envelope = invitation(identity);
    Record {
        envelope,
        initial_image: InitialImage {
            width: 100,
            height: 120,
            byte_length: 2_048,
        },
        revision: 1,
        digest: "a".repeat(64),
        width: 100,
        height: 120,
        byte_length: 2_048,
        status: Status::Invited,
        receiver: None,
        expires_at_ms: NOW + 300_000,
        updated_at_ms: NOW,
    }
}

#[test]
fn inspect_allows_unlinked_receiver_but_sync_requires_participant() {
    let source = identity("00000000-0000-4000-8000-000000000001", "account:fixture");
    let receiver = identity("00000000-0000-4000-8000-000000000002", "account:fixture");
    let record = record(&source);
    let envelope = record.envelope.clone();
    let peer = Peer {
        device_id: source.session().device_id.clone(),
        public_key: source.session().public_key.clone(),
        device_name: source.session().device_name.clone(),
        endpoint: None,
    };
    let mut inspect = CentralResponse::Projection {
        view: View {
            record: record.clone(),
            available: true,
            authorized_until_ms: NOW + 30_000,
            peer: Some(peer),
        },
    };
    validate(
        &mut inspect,
        &CentralOperation::Inspect {
            envelope: envelope.clone(),
        },
        &receiver,
        NOW,
    )
    .unwrap();
    let mut sync = CentralResponse::Sync {
        views: vec![View {
            record,
            available: true,
            authorized_until_ms: NOW + 30_000,
            peer: None,
        }],
    };
    assert_eq!(
        validate(
            &mut sync,
            &CentralOperation::Sync {
                endpoint: crate::EndpointAddress {
                    endpoint_id: "a".repeat(64),
                    addresses: vec![],
                    relay_url: None,
                },
            },
            &receiver,
            NOW,
        )
        .unwrap_err()
        .code,
        "projection_response_invalid"
    );
}

#[test]
fn server_errors_keep_known_codes_and_hide_unknown_details() {
    assert_eq!(
        server_error(409, br#"{"error":{"code":"projection_revision_conflict"}}"#).code,
        "projection_revision_conflict"
    );
    assert_eq!(
        server_error(500, br#"{"error":{"code":"database_password"}}"#).code,
        "projection_request_failed"
    );
}

#[test]
fn malformed_projection_response_is_rejected() {
    let source = identity("00000000-0000-4000-8000-000000000003", "account:fixture");
    let envelope = invitation(&source);
    let mut response = CentralResponse::Projection {
        view: View {
            record: Record {
                envelope: Envelope {
                    protocol: PROTOCOL.to_owned(),
                    projection_id: "projection:bad".to_owned(),
                    server_origin: ORIGIN.to_owned(),
                    source: Source {
                        device_id: source.session().device_id.clone(),
                        account_id: source.session().account_id.clone(),
                        public_key: source.session().public_key.clone(),
                        session_id: envelope.source.session_id.clone(),
                        unit_id: envelope.source.unit_id.clone(),
                        revision: 1,
                    },
                    content: Content {
                        kind: "sticker".to_owned(),
                        digest: "a".repeat(64),
                    },
                    expires_at_ms: envelope.expires_at_ms,
                    nonce: "b".repeat(32),
                    signature: EnvelopeSignature {
                        algorithm: "ed25519".to_owned(),
                        key_id: source.session().device_id.clone(),
                        value: "bad".to_owned(),
                    },
                },
                ..record(&source)
            },
            available: false,
            authorized_until_ms: NOW,
            peer: None,
        },
    };
    assert_eq!(
        validate(
            &mut response,
            &CentralOperation::Read {
                projection_id: envelope.projection_id,
            },
            &source,
            NOW,
        )
        .unwrap_err()
        .code,
        "projection_invalid_invitation"
    );
}
