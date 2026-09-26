//! Signed one-hop device metadata. A remote catalog never grants raster access.
use super::*;
use handshake::{Challenge, ProbeGuard};
use sha2::{Digest, Sha256};

const CATALOG_TTL_MS: u64 = 5_000;
#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogDevice {
    device_id: String,
    name: String,
    policy: String,
}

impl CatalogDevice {
    fn valid(&self) -> bool {
        loom_protocol::projection::projection_identifier_valid(&self.device_id)
            && !self.name.is_empty()
            && self.name.len() <= 1024
            && !self.name.chars().any(char::is_control)
            && matches!(self.policy.as_str(), "confirm" | "auto")
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogProof {
    challenge: Challenge,
    devices: Vec<CatalogDevice>,
    expires_at_ms: u64,
    transfer_protocol: String,
    signature: String,
}

impl CatalogProof {
    fn message(&self) -> PeerResult<Vec<u8>> {
        let payload =
            serde_json::to_vec(&(&self.devices, self.expires_at_ms, &self.transfer_protocol))
                .map_err(|_| failure(503, "peer_unavailable"))?;
        let mut message = self.challenge.message("catalog-response");
        message.extend_from_slice(format!("\n{:x}", Sha256::digest(payload)).as_bytes());
        Ok(message)
    }

    fn verify(&self, peer: &Peer, challenge: &Challenge) -> PeerResult<()> {
        challenge.validate(unix_time_millis())?;
        let mut ids = BTreeSet::new();
        if self.challenge != *challenge
            || self.transfer_protocol != "loom.offline-transfer.v1"
            || self.devices.len() > 64
            || self.expires_at_ms != challenge.timestamp_ms.saturating_add(CATALOG_TTL_MS)
            || self.expires_at_ms <= unix_time_millis()
            || self.signature.len() != 88
            || !self
                .devices
                .iter()
                .all(|device| device.valid() && ids.insert(&device.device_id))
        {
            return Err(failure(403, "peer_invalid_catalog"));
        }
        loom_plugin_security::verify_message(&peer.public_key, &self.message()?, &self.signature)
            .map_err(|_| failure(403, "peer_invalid_catalog"))
    }
}

impl OfflinePeers {
    pub(super) fn accept_catalog(
        &self,
        input: Challenge,
        registry: &SharedDeviceRegistryStore,
    ) -> PeerResult<Value> {
        let state = self.authenticate_challenge(&input, "catalog-request")?;
        let registry = registry.try_lock().map_err(|_| failure(503, "peer_busy"))?;
        let now = unix_time_millis();
        // Read local presence directly: aggregated foreign devices must never be re-exported.
        let devices = registry
            .projections
            .presence
            .iter()
            .filter_map(|(id, presence)| {
                let device = registry.authorized_keyed_device(id).ok()?;
                if device.session_epoch != presence.epoch
                    || now.saturating_sub(presence.seen) >= PROJECTION_PRESENCE_TTL
                {
                    return None;
                }
                let policy = match presence.policy {
                    ProjectionReceivePolicy::Confirm => "confirm",
                    ProjectionReceivePolicy::Auto => "auto",
                    ProjectionReceivePolicy::Disabled => return None,
                };
                let item = CatalogDevice {
                    device_id: id.clone(),
                    name: device.name.clone(),
                    policy: policy.to_owned(),
                };
                item.valid().then_some(item)
            })
            .take(64)
            .collect();
        drop(registry);
        let expires_at_ms = input.timestamp_ms.saturating_add(CATALOG_TTL_MS);
        let mut proof = CatalogProof {
            challenge: input,
            devices,
            expires_at_ms,
            transfer_protocol: "loom.offline-transfer.v1".to_owned(),
            signature: String::new(),
        };
        proof.signature = sign_message(&state.document.identity, &proof.message()?)
            .map_err(|_| failure(503, "peer_unavailable"))?;
        serde_json::to_value(proof).map_err(|_| failure(503, "peer_unavailable"))
    }

    pub fn append_targets(&self, response: &mut Value) {
        response["capabilities"]["offlinePeerDirectory"] = json!(true);
        response["capabilities"]["offlinePeers"] = json!(true);
        if self
            .catalog_fetching
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            response["peerDirectory"] = json!({"status": "busy"});
            return;
        }
        let _guard = ProbeGuard(&self.catalog_fetching);
        let document = match self.state.lock() {
            Ok(state) => state.document.clone(),
            Err(_) => {
                response["peerDirectory"] = json!({"status": "unavailable"});
                return;
            }
        };
        // At most 16 bounded workers per daemon, and no registry/trust lock across network I/O.
        let replies = thread::scope(|scope| {
            let tasks: Vec<_> = document
                .peers
                .values()
                .filter(|peer| peer.enabled)
                .map(|peer| {
                    let identity = &document.identity;
                    thread::Builder::new()
                        .name("loom-peer-catalog".to_owned())
                        .spawn_scoped(scope, move || {
                            let mut challenge = Challenge {
                                source_id: identity.key_id.clone(),
                                target_id: peer.peer_id.clone(),
                                nonce: Uuid::new_v4().simple().to_string(),
                                timestamp_ms: unix_time_millis(),
                                signature: String::new(),
                            };
                            challenge.signature =
                                sign_message(identity, &challenge.message("catalog-request"))
                                    .map_err(|_| failure(503, "peer_unavailable"))?;
                            let proof: CatalogProof =
                                transport::request(peer, "catalog", &challenge, 192 * 1024, 2)?;
                            proof.verify(peer, &challenge)?;
                            Ok((peer.clone(), proof))
                        })
                })
                .collect();
            tasks
                .into_iter()
                .map(|task| match task {
                    Ok(task) => task
                        .join()
                        .unwrap_or_else(|_| Err(failure(503, "peer_unavailable"))),
                    Err(_) => Err(failure(503, "peer_unavailable")),
                })
                .collect::<Vec<PeerResult<(Peer, CatalogProof)>>>()
        });
        let state = match self.state.lock() {
            Ok(state) if state.document.revision == document.revision => state,
            _ => {
                response["peerDirectory"] = json!({"status": "unavailable"});
                return;
            }
        };
        let Some(targets) = response["targets"].as_array_mut() else {
            return;
        };
        let mut unavailable = 0;
        for reply in replies {
            let Ok((peer, proof)) = reply else {
                unavailable += 1;
                continue;
            };
            if proof.expires_at_ms <= unix_time_millis() {
                unavailable += 1;
                continue;
            }
            for device in proof.devices {
                if targets.len() >= 64 {
                    break;
                }
                let id = format!(
                    "peer-target:{:x}",
                    Sha256::digest(format!("{}\n{}", peer.peer_id, device.device_id))
                );
                targets.push(
                    json!({"deviceId": id, "name": device.name, "policy": device.policy,
                    "route": "offline_peer", "peerId": peer.peer_id, "peerName": peer.name,
                    "remoteDeviceId": device.device_id, "deliveryAvailable": true,
                    "transferProtocol": "loom.offline-transfer.v1"}),
                );
            }
        }
        drop(state);
        response["peerDirectory"] = json!({"status": if unavailable == 0 { "complete" } else { "partial" },
            "unavailablePeers": unavailable});
    }
}
