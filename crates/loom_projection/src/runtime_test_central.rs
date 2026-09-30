//! HTTP coordination fixture; production Redis authorization has its own Platform tests.
use crate::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, VerifyingKey};
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeMap, sync::Arc};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::Mutex,
    task::JoinHandle,
};

#[derive(Default)]
pub(super) struct CentralState {
    pub sessions: BTreeMap<String, AccountSession>,
    pub records: BTreeMap<String, Record>,
    endpoints: BTreeMap<String, EndpointAddress>,
    pub lose_publish_response: bool,
    pub mutations: usize,
}

pub(super) struct CentralFixture {
    pub origin: String,
    pub state: Arc<Mutex<CentralState>>,
    server: JoinHandle<()>,
}

impl Drop for CentralFixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl CentralFixture {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(CentralState::default()));
        let stored = state.clone();
        let server_origin = origin.clone();
        let server = tokio::spawn(async move {
            let mut tasks = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (mut stream, _) = accepted.unwrap();
                        let stored = stored.clone();
                        let origin = server_origin.clone();
                        tasks.spawn(async move {
                            let mut bytes = Vec::new();
                            let (header_end, length) = loop {
                                let mut chunk = [0u8; 2048];
                                let read = stream.read(&mut chunk).await.unwrap();
                                if read == 0 { return; }
                                bytes.extend_from_slice(&chunk[..read]);
                                assert!(bytes.len() <= 32 * 1024);
                                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                                    let header = std::str::from_utf8(&bytes[..end]).unwrap();
                                    let length = header.lines().find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:")
                                        .map(|v| v.trim().parse::<usize>().unwrap())).unwrap();
                                    break (end + 4, length);
                                }
                            };
                            assert!(length <= 16 * 1024);
                            while bytes.len() < header_end + length {
                                let mut chunk = [0u8; 2048];
                                let read = stream.read(&mut chunk).await.unwrap();
                                if read == 0 { return; }
                                bytes.extend_from_slice(&chunk[..read]);
                            }
                            let proof: Value = serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
                            let (status, value) = handle(&mut *stored.lock().await, &origin, proof);
                            let body = serde_json::to_vec(&value).unwrap();
                            let header = format!("HTTP/1.1 {status} Result\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                            let _ = stream.write_all(header.as_bytes()).await;
                            let _ = stream.write_all(&body).await;
                        });
                    }
                    _ = tasks.join_next(), if !tasks.is_empty() => {}
                }
            }
        });
        Self {
            origin,
            state,
            server,
        }
    }
}

fn failure(status: u16, code: &str) -> (u16, Value) {
    (status, json!({"error":{"code":code}}))
}

fn handle(state: &mut CentralState, origin: &str, proof: Value) -> (u16, Value) {
    let device = proof["deviceId"].as_str().unwrap();
    let Some(session) = state.sessions.get(device).cloned() else {
        return failure(401, "device_session_unavailable");
    };
    let payload = proof["payload"].as_str().unwrap();
    assert!(!payload.contains("imageBase64"));
    let key: [u8; 32] = STANDARD
        .decode(&session.public_key)
        .unwrap()
        .try_into()
        .unwrap();
    let signature = Signature::from_slice(
        &STANDARD
            .decode(proof["signature"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap();
    let signed = format!(
        "neuro.loom-account.v1\nprojection\n{device}\n{}\n{}\n{:x}",
        proof["timestampMs"].as_u64().unwrap(),
        proof["nonce"].as_str().unwrap(),
        Sha256::digest(payload.as_bytes())
    );
    VerifyingKey::from_bytes(&key)
        .unwrap()
        .verify_strict(signed.as_bytes(), &signature)
        .unwrap();
    let operation: CentralOperation = serde_json::from_str(payload).unwrap();
    if let CentralOperation::Configuration = operation {
        return (
            200,
            json!({"kind":"configuration","policy":Policy { protocol:PROTOCOL.to_owned(), server_origin:origin.to_owned(),
            relay_urls:vec![], sync_interval_ms:100, authorization_lease_ms:30_000, presence_ttl_ms:45_000, max_records:64 }}),
        );
    }
    if let CentralOperation::Sync { endpoint } = operation {
        state.endpoints.insert(device.to_owned(), endpoint);
        let views: Vec<_> = state
            .records
            .values()
            .filter(|r| participant(r, device))
            .map(|r| view(state, r, device))
            .collect();
        return (200, json!({"kind":"sync","views":views}));
    }
    if let CentralOperation::Peer {
        projection_id,
        peer_device_id,
        peer_public_key,
        envelope,
    } = &operation
    {
        let Some(record) = state.records.get(projection_id) else {
            return failure(404, "projection_not_found");
        };
        let Some(peer) = peer(state, peer_device_id) else {
            return failure(409, "projection_peer_unavailable");
        };
        if record.status == Status::Stopped
            || record.envelope.source.device_id != device
            || &peer.public_key != peer_public_key
            || (record.status == Status::Invited && envelope.as_ref() != Some(&record.envelope))
            || (record.status == Status::Linked
                && record.receiver.as_ref().unwrap().device_id != *peer_device_id)
        {
            return failure(403, "projection_access_denied");
        }
        return (
            200,
            json!({"kind":"peer","peer":peer,"authorizedUntilMs":now_ms()+30_000}),
        );
    }
    let id = match &operation {
        CentralOperation::Create { envelope, .. }
        | CentralOperation::Inspect { envelope }
        | CentralOperation::Accept { envelope, .. } => {
            envelope.validate(origin).unwrap();
            envelope.projection_id.clone()
        }
        CentralOperation::Read { projection_id }
        | CentralOperation::Unlink { projection_id }
        | CentralOperation::Publish { projection_id, .. } => projection_id.clone(),
        _ => unreachable!(),
    };
    if let CentralOperation::Create {
        envelope,
        width,
        height,
        byte_length,
    } = operation
    {
        assert_eq!(envelope.source.device_id, device);
        state.mutations += 1;
        state.records.entry(id.clone()).or_insert(Record {
            revision: 1,
            digest: envelope.content.digest.clone(),
            envelope,
            initial_image: InitialImage {
                width,
                height,
                byte_length,
            },
            width,
            height,
            byte_length,
            status: Status::Invited,
            receiver: None,
            expires_at_ms: session.expires_at_ms,
            updated_at_ms: now_ms(),
        });
    } else {
        let Some(record) = state.records.get_mut(&id) else {
            return failure(404, "projection_not_found");
        };
        match operation {
            CentralOperation::Inspect { .. } if record.status == Status::Invited => {}
            CentralOperation::Accept {
                receiver_unit_id,
                expected_revision,
                expected_digest,
                ..
            } => {
                if let Some(receiver) = &record.receiver {
                    if receiver.device_id != device || receiver.unit_id != receiver_unit_id {
                        return failure(409, "projection_access_denied");
                    }
                } else {
                    if record.revision != expected_revision || record.digest != expected_digest {
                        return failure(409, "projection_content_changed");
                    }
                    record.receiver = Some(Receiver {
                        device_id: device.to_owned(),
                        unit_id: receiver_unit_id,
                        revision: expected_revision,
                        digest: expected_digest,
                    });
                    record.status = Status::Linked;
                    state.mutations += 1;
                }
            }
            CentralOperation::Publish {
                prior_revision,
                revision,
                digest,
                width,
                height,
                byte_length,
                ..
            } => {
                if record.status == Status::Stopped
                    || record.envelope.source.device_id != device
                    || record.revision != prior_revision
                {
                    return failure(409, "projection_revision_conflict");
                }
                record.revision = revision;
                record.digest = digest;
                record.width = width;
                record.height = height;
                record.byte_length = byte_length;
                record.updated_at_ms = now_ms();
                state.mutations += 1;
                if std::mem::take(&mut state.lose_publish_response) {
                    return failure(503, "projection_network_unavailable");
                }
            }
            CentralOperation::Unlink { .. } if participant(record, device) => {
                record.status = Status::Stopped;
                state.mutations += 1;
            }
            CentralOperation::Read { .. } if participant(record, device) => {}
            _ => return failure(403, "projection_access_denied"),
        }
    }
    (
        200,
        json!({"kind":"projection","view":view(state, &state.records[&id], device)}),
    )
}

fn participant(record: &Record, device: &str) -> bool {
    record.envelope.source.device_id == device
        || record
            .receiver
            .as_ref()
            .is_some_and(|r| r.device_id == device)
}
fn peer(state: &CentralState, device: &str) -> Option<Peer> {
    let session = state.sessions.get(device)?;
    Some(Peer {
        device_id: device.to_owned(),
        public_key: session.public_key.clone(),
        device_name: session.device_name.clone(),
        endpoint: state.endpoints.get(device).cloned(),
    })
}
fn view(state: &CentralState, record: &Record, device: &str) -> View {
    let peer_id = if record.envelope.source.device_id == device {
        record.receiver.as_ref().map(|r| r.device_id.as_str())
    } else {
        Some(record.envelope.source.device_id.as_str())
    };
    let peer = peer_id.and_then(|id| peer(state, id));
    let available = record.status != Status::Stopped && peer_id.is_none_or(|_| peer.is_some());
    View {
        record: record.clone(),
        available,
        authorized_until_ms: if available { now_ms() + 30_000 } else { 0 },
        peer,
    }
}
