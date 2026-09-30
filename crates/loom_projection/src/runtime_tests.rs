use crate::runtime_test_central::CentralFixture;
use crate::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use serde_json::Value;
use std::{path::PathBuf, sync::Arc, time::Duration};

struct TestRoot(PathBuf);
impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn identity(origin: &str) -> Identity {
    let key = SigningKey::generate(&mut OsRng);
    Identity::new(
        origin.to_owned(),
        AccountSession {
            protocol: "neuro.loom-account.v1".to_owned(),
            device_id: uuid::Uuid::new_v4().to_string(),
            account_id: "account:test".to_owned(),
            username: "fixture".to_owned(),
            device_name: "QR fixture".to_owned(),
            public_key: STANDARD.encode(key.verifying_key().as_bytes()),
            expires_at_ms: now_ms() + 600_000,
        },
        key,
        now_ms(),
    )
    .unwrap()
}

pub(super) fn image(color: u8) -> LocalImage {
    let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([color, 90, 20, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    LocalImage {
        image_base64: STANDARD.encode(png.into_inner()),
        width: 2,
        height: 2,
    }
}

async fn read_until(runtime: &ProjectionRuntime, id: &str, revision: u64) -> Value {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let value = runtime
                .execute(
                    "hook:receiver",
                    LocalOperation::Read {
                        projection_id: id.to_owned(),
                        known_revision: 1,
                    },
                )
                .await
                .unwrap();
            if value["revision"] == revision {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .expect("automatic receiver update")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_projection_lifecycle_recovers_lost_publish_restart_and_fences_identity() {
    let central = CentralFixture::start().await;
    let source_identity = identity(&central.origin);
    let receiver_identity = identity(&central.origin);
    {
        let mut state = central.state.lock().await;
        for identity in [&source_identity, &receiver_identity] {
            state.sessions.insert(
                identity.session().device_id.clone(),
                identity.session().clone(),
            );
        }
    }
    let root =
        TestRoot(std::env::temp_dir().join(format!("loom-projection-{}", uuid::Uuid::new_v4())));
    let source = ProjectionRuntime::start(source_identity.clone(), root.0.join("source"))
        .await
        .unwrap();
    let receiver = ProjectionRuntime::start(receiver_identity.clone(), root.0.join("receiver"))
        .await
        .unwrap();
    let created = source
        .execute(
            "hook:source",
            LocalOperation::Create {
                unit_id: "unit:source".to_owned(),
                content_kind: "sticker".to_owned(),
                snapshot: image(1),
            },
        )
        .await
        .unwrap();
    let envelope: Envelope = serde_json::from_value(created["envelope"].clone()).unwrap();
    let id = envelope.projection_id.clone();
    let invited_view = source.store.lock().await.entries[&id].view.clone().unwrap();
    let preview = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if let Ok(value) = receiver
                .execute(
                    "hook:receiver",
                    LocalOperation::Inspect {
                        envelope: envelope.clone(),
                    },
                )
                .await
            {
                break value;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .expect("authenticated preview");
    assert_eq!(preview["snapshot"]["imageBase64"], image(1).image_base64);
    let accept = LocalOperation::Accept {
        envelope: envelope.clone(),
        receiver_unit_id: "unit:receiver".to_owned(),
        expected_revision: 1,
        expected_digest: envelope.content.digest.clone(),
        confirmed: true,
    };
    let accepted = receiver
        .execute("hook:receiver", accept.clone())
        .await
        .unwrap();
    assert_eq!(accepted["receiverUnitId"], "unit:receiver");
    assert!(receiver
        .execute(
            "hook:other",
            LocalOperation::Read {
                projection_id: id.clone(),
                known_revision: 1
            }
        )
        .await
        .is_err());
    for (prior, color) in [(1, 2), (2, 3)] {
        if color == 3 {
            central.state.lock().await.lose_publish_response = true;
        }
        let result = source
            .execute(
                "hook:source",
                LocalOperation::Update {
                    projection_id: id.clone(),
                    source_session_id: envelope.source.session_id.clone(),
                    prior_revision: prior,
                    revision: prior + 1,
                    snapshot: image(color),
                },
            )
            .await;
        if color == 2 {
            result.unwrap();
            let mut store = source.store.lock().await;
            let entry = store.entries.get_mut(&id).unwrap();
            entry.merge_view(invited_view.clone()).unwrap();
            assert_eq!(entry.view.as_ref().unwrap().record.revision, 2);
            assert_eq!(entry.view.as_ref().unwrap().record.status, Status::Linked);
            let mut unavailable = invited_view.clone();
            unavailable.available = false;
            unavailable.authorized_until_ms = 0;
            unavailable.peer = None;
            entry.merge_view(unavailable).unwrap();
            assert!(!entry.view.as_ref().unwrap().available);
            assert_eq!(entry.view.as_ref().unwrap().record.revision, 2);
        } else {
            assert!(result.is_err());
        }
        let received = read_until(&receiver, &id, prior + 1).await;
        assert_eq!(
            received["snapshot"]["imageBase64"],
            image(color).image_base64
        );
        assert_eq!(received["transport"], "direct");
    }
    assert_eq!(
        central.state.lock().await.mutations,
        4,
        "create, accept and two unique updates"
    );
    source.close().await;
    receiver.close().await;
    assert_eq!(
        source
            .execute("hook:source", LocalOperation::Context {})
            .await
            .unwrap_err()
            .code,
        "device_session_unavailable"
    );
    let source = ProjectionRuntime::start(source_identity, root.0.join("source"))
        .await
        .unwrap();
    let receiver = ProjectionRuntime::start(receiver_identity.clone(), root.0.join("receiver"))
        .await
        .unwrap();
    assert_eq!(
        read_until(&receiver, &id, 3).await["snapshot"]["imageBase64"],
        image(3).image_base64
    );
    let restored = receiver.execute("hook:receiver", accept).await.unwrap();
    assert_eq!(restored["receiverUnitId"], "unit:receiver");
    receiver
        .execute(
            "hook:receiver",
            LocalOperation::Unlink {
                projection_id: id.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        receiver
            .execute(
                "hook:receiver",
                LocalOperation::Read {
                    projection_id: id.clone(),
                    known_revision: 3
                }
            )
            .await
            .unwrap_err()
            .code,
        "projection_unlinked"
    );
    expired_invitations_allow_regeneration_without_discarding_acceptance(&source, &central, &id)
        .await;
    central
        .state
        .lock()
        .await
        .sessions
        .remove(&receiver_identity.session().device_id);
    tokio::time::timeout(Duration::from_secs(5), async {
        while receiver.is_active() {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    source.close().await;
    receiver.close().await;
    let changed = identity(&central.origin);
    central.state.lock().await.sessions.insert(
        changed.session().device_id.clone(),
        changed.session().clone(),
    );
    let other = ProjectionRuntime::start(changed, root.0.join("receiver"))
        .await
        .unwrap();
    assert_eq!(
        other
            .execute(
                "hook:receiver",
                LocalOperation::Read {
                    projection_id: id,
                    known_revision: 3
                }
            )
            .await
            .unwrap_err()
            .code,
        "projection_account_mismatch"
    );
    other.close().await;
    assert_eq!(Arc::strong_count(&other), 1, "all owned workers are joined");
}

async fn expired_invitations_allow_regeneration_without_discarding_acceptance(
    source: &ProjectionRuntime,
    central: &CentralFixture,
    existing_id: &str,
) {
    for accepted in [false, true] {
        let unit = if accepted {
            "unit:accepted"
        } else {
            "unit:expired"
        };
        let mut entry = source.store.lock().await.entries[existing_id].clone();
        let envelope = source
            .identity()
            .invitation(
                unit,
                "sticker",
                &entry.envelope.content.digest,
                now_ms() - 300_001,
            )
            .unwrap();
        entry.envelope = envelope.clone();
        entry.unit_id = unit.to_owned();
        entry.snapshot = image(1).snapshot(&envelope, 1).unwrap();
        entry.stopped = false;
        entry.pending = None;
        entry.pending_snapshot = None;
        let view = entry.view.as_mut().unwrap();
        view.record.envelope = envelope.clone();
        view.record.revision = 1;
        view.record.digest = entry.snapshot.digest.clone();
        view.record.status = Status::Invited;
        let receiver = view.record.receiver.take().unwrap();
        let mut record = view.record.clone();
        if accepted {
            record.status = Status::Linked;
            record.receiver = Some(receiver);
        }
        central
            .state
            .lock()
            .await
            .records
            .insert(envelope.projection_id.clone(), record);
        source.store.lock().await.save(entry).await.unwrap();
        let response = source
            .execute(
                "hook:source",
                LocalOperation::Create {
                    unit_id: unit.to_owned(),
                    content_kind: "sticker".to_owned(),
                    snapshot: image(1),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            response["envelope"]["projectionId"] == envelope.projection_id,
            accepted
        );
        assert_eq!(
            source.store.lock().await.entries[&envelope.projection_id].stopped,
            !accepted
        );
    }
}

#[test]
fn snapshot_rejects_non_png_and_forged_dimensions_even_with_matching_digest() {
    use sha2::{Digest as _, Sha256};
    let id = identity("http://127.0.0.1:1");
    let envelope = id
        .invitation("unit:test", "sticker", &"0".repeat(64), now_ms())
        .unwrap();
    let mut snapshot = image(1).snapshot(&envelope, 1).unwrap();
    snapshot.png = b"not a png".to_vec();
    snapshot.digest = format!("{:x}", Sha256::digest(&snapshot.png));
    assert_eq!(
        snapshot.validate().unwrap_err().code,
        "projection_invalid_image"
    );
    let mut snapshot = image(1).snapshot(&envelope, 1).unwrap();
    snapshot.width = 3;
    assert_eq!(
        snapshot.validate().unwrap_err().code,
        "projection_dimensions_mismatch"
    );
}
