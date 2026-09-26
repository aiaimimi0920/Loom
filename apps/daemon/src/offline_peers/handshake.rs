//! Bounded mutual proof of possession; never a source of device or transitive peer trust.
use super::*;

const CLOCK_WINDOW_MS: u64 = 30_000;
const MAX_NONCES: usize = 128;

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Challenge {
    pub(super) source_id: String,
    pub(super) target_id: String,
    pub(super) nonce: String,
    pub(super) timestamp_ms: u64,
    pub(super) signature: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Proof {
    challenge: Challenge,
    signature: String,
    delivery_available: bool,
}

impl Challenge {
    pub(super) fn message(&self, purpose: &str) -> Vec<u8> {
        format!(
            "loom.offline-peer.v1\n{purpose}\n{}\n{}\n{}\n{}",
            self.source_id, self.target_id, self.nonce, self.timestamp_ms
        )
        .into_bytes()
    }

    pub(super) fn validate(&self, now: u64) -> PeerResult<()> {
        if self.source_id.len() != 69
            || self.target_id.len() != 69
            || self.nonce.len() != 32
            || !self.nonce.bytes().all(|b| b.is_ascii_hexdigit())
            || self.signature.len() != 88
            || now.abs_diff(self.timestamp_ms) > CLOCK_WINDOW_MS
        {
            return Err(failure(403, "peer_invalid_proof"));
        }
        Ok(())
    }
}

pub(super) struct ProbeGuard<'a>(pub(super) &'a AtomicBool);
impl Drop for ProbeGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl OfflinePeers {
    pub(super) fn authenticate_challenge(
        &self,
        input: &Challenge,
        purpose: &str,
    ) -> PeerResult<std::sync::MutexGuard<'_, State>> {
        let now = unix_time_millis();
        input.validate(now)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| failure(503, "peer_unavailable"))?;
        if input.target_id != state.document.identity.key_id {
            return Err(failure(403, "peer_identity_mismatch"));
        }
        let peer = state
            .document
            .peers
            .get(&input.source_id)
            .filter(|peer| peer.enabled)
            .ok_or_else(|| failure(403, "peer_not_trusted"))?;
        loom_plugin_security::verify_message(
            &peer.public_key,
            &input.message(purpose),
            &input.signature,
        )
        .map_err(|_| failure(403, "peer_invalid_proof"))?;
        // Keep accepted nonces for the entire possible timestamp-validity interval.
        state.nonces.retain(|_, expires| *expires >= now);
        let nonce_key = format!("{}:{}", input.source_id, input.nonce);
        if state.nonces.contains_key(&nonce_key) {
            return Err(failure(409, "peer_replayed"));
        }
        if state.nonces.len() >= MAX_NONCES {
            return Err(failure(429, "peer_busy"));
        }
        state.nonces.insert(
            nonce_key,
            input.timestamp_ms.saturating_add(CLOCK_WINDOW_MS),
        );
        Ok(state)
    }

    pub(super) fn accept_handshake(&self, input: Challenge) -> PeerResult<Value> {
        let state = self.authenticate_challenge(&input, "request")?;
        let signature = sign_message(&state.document.identity, &input.message("response"))
            .map_err(|_| failure(503, "peer_unavailable"))?;
        serde_json::to_value(Proof {
            challenge: input,
            signature,
            delivery_available: false,
        })
        .map_err(|_| failure(503, "peer_unavailable"))
    }

    pub(super) fn probe(&self, id: &str) -> PeerResult<Value> {
        if self
            .probing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(failure(429, "peer_probe_busy"));
        }
        let _guard = ProbeGuard(&self.probing);
        let (document, peer) = {
            let state = self
                .state
                .lock()
                .map_err(|_| failure(503, "peer_unavailable"))?;
            let peer = state
                .document
                .peers
                .get(id)
                .filter(|peer| peer.enabled)
                .ok_or_else(|| failure(403, "peer_not_trusted"))?
                .clone();
            (state.document.clone(), peer)
        };
        let mut challenge = Challenge {
            source_id: document.identity.key_id.clone(),
            target_id: peer.peer_id.clone(),
            nonce: Uuid::new_v4().simple().to_string(),
            timestamp_ms: unix_time_millis(),
            signature: String::new(),
        };
        challenge.signature = sign_message(&document.identity, &challenge.message("request"))
            .map_err(|_| failure(503, "peer_unavailable"))?;
        let proof: Proof = transport::request(&peer, "handshake", &challenge, 16_384, 5)?;
        challenge.validate(unix_time_millis())?;
        if proof.challenge != challenge || proof.delivery_available || proof.signature.len() != 88 {
            return Err(failure(403, "peer_invalid_proof"));
        }
        loom_plugin_security::verify_message(
            &peer.public_key,
            &challenge.message("response"),
            &proof.signature,
        )
        .map_err(|_| failure(403, "peer_invalid_proof"))?;
        // Never report success for trust that was changed/revoked while network I/O was pending.
        let state = self
            .state
            .lock()
            .map_err(|_| failure(503, "peer_unavailable"))?;
        if state.document.revision != document.revision {
            return Err(failure(409, "peer_configuration_changed"));
        }
        Ok(json!({"peerId": peer.peer_id, "verified": true,
            "revision": document.revision, "deliveryAvailable": false}))
    }
}
