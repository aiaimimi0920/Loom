//! Bounded raster transport proof. Never creates a Hook invitation or retains image data.
use super::*;
use handshake::{Challenge, ProbeGuard};
use loom_protocol::projection::MAX_PROJECTION_HTTP_BYTES;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Raster {
    digest: String,
    width: u32,
    height: u32,
    byte_length: usize,
}

impl Raster {
    fn from_snapshot(snapshot: &ProjectionSnapshot) -> PeerResult<Self> {
        if snapshot.image_base64.len() > MAX_PROJECTION_IMAGE_BYTES.div_ceil(3) * 4 {
            return Err(failure(413, "projection_image_budget"));
        }
        let bytes = BASE64
            .decode(&snapshot.image_base64)
            .map_err(|_| failure(400, "projection_image_invalid"))?;
        let raster = Self {
            digest: sha256_bytes(&bytes),
            width: snapshot.width,
            height: snapshot.height,
            byte_length: bytes.len(),
        };
        drop(bytes);
        validate_projection_snapshot(snapshot, &raster.digest)
            .map_err(|error| failure(error.status, error.code))?;
        Ok(raster)
    }

    fn purpose(&self, response: bool) -> String {
        // All raster metadata is authenticated; the receiver independently recomputes it.
        format!(
            "raster-{}\n{}\n{}\n{}\n{}\nnot-delivered\nnot-retained",
            if response { "response" } else { "request" },
            self.digest,
            self.width,
            self.height,
            self.byte_length
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RasterRequest {
    challenge: Challenge,
    raster: Raster,
    snapshot: ProjectionSnapshot,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RasterReceipt {
    challenge: Challenge,
    raster: Raster,
    signature: String,
    delivery_available: bool,
    retained: bool,
}

impl RasterReceipt {
    fn verify(&self, peer: &Peer, request: &RasterRequest) -> PeerResult<()> {
        request.challenge.validate(unix_time_millis())?;
        if self.challenge != request.challenge
            || self.raster != request.raster
            || self.delivery_available
            || self.retained
            || self.signature.len() != 88
        {
            return Err(failure(403, "peer_invalid_raster_receipt"));
        }
        loom_plugin_security::verify_message(
            &peer.public_key,
            &self.challenge.message(&self.raster.purpose(true)),
            &self.signature,
        )
        .map_err(|_| failure(403, "peer_invalid_raster_receipt"))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RasterProbe {
    peer_id: String,
    snapshot: ProjectionSnapshot,
}

fn parse_raster<T: serde::de::DeserializeOwned>(body: &str) -> PeerResult<T> {
    if body.len() > MAX_PROJECTION_HTTP_BYTES {
        return Err(failure(413, "peer_body_too_large"));
    }
    serde_json::from_str(body).map_err(|_| failure(400, "peer_invalid_request"))
}

impl OfflinePeers {
    pub(super) fn accept_raster(&self, body: &str) -> PeerResult<Value> {
        if self
            .raster_receiving
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(failure(429, "peer_raster_busy"));
        }
        let _guard = ProbeGuard(&self.raster_receiving);
        let input: RasterRequest = parse_raster(body)?;
        let revision = self
            .authenticate_challenge(&input.challenge, &input.raster.purpose(false))?
            .document
            .revision;
        // PNG decoding holds no trust/device lock. Untrusted signatures never reach the decoder.
        if Raster::from_snapshot(&input.snapshot)? != input.raster {
            return Err(failure(400, "peer_raster_mismatch"));
        }
        let state = self
            .state
            .lock()
            .map_err(|_| failure(503, "peer_unavailable"))?;
        if state.document.revision != revision {
            return Err(failure(409, "peer_configuration_changed"));
        }
        input.challenge.validate(unix_time_millis())?;
        let signature = sign_message(
            &state.document.identity,
            &input.challenge.message(&input.raster.purpose(true)),
        )
        .map_err(|_| failure(503, "peer_unavailable"))?;
        serde_json::to_value(RasterReceipt {
            challenge: input.challenge,
            raster: input.raster,
            signature,
            delivery_available: false,
            retained: false,
        })
        .map_err(|_| failure(503, "peer_unavailable"))
    }

    pub(super) fn probe_raster(&self, body: &str) -> PeerResult<Value> {
        if self
            .probing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(failure(429, "peer_probe_busy"));
        }
        let _guard = ProbeGuard(&self.probing);
        let input: RasterProbe = parse_raster(body)?;
        let (document, peer) = {
            let state = self
                .state
                .lock()
                .map_err(|_| failure(503, "peer_unavailable"))?;
            let peer = state
                .document
                .peers
                .get(&input.peer_id)
                .filter(|peer| peer.enabled)
                .ok_or_else(|| failure(403, "peer_not_trusted"))?
                .clone();
            (state.document.clone(), peer)
        };
        let raster = Raster::from_snapshot(&input.snapshot)?;
        let mut challenge = Challenge {
            source_id: document.identity.key_id.clone(),
            target_id: peer.peer_id.clone(),
            nonce: Uuid::new_v4().simple().to_string(),
            timestamp_ms: unix_time_millis(),
            signature: String::new(),
        };
        challenge.signature = sign_message(
            &document.identity,
            &challenge.message(&raster.purpose(false)),
        )
        .map_err(|_| failure(503, "peer_unavailable"))?;
        let request = RasterRequest {
            challenge,
            raster,
            snapshot: input.snapshot,
        };
        let receipt: RasterReceipt =
            transport::request(&peer, "raster-check", &request, 16_384, 10)?;
        receipt.verify(&peer, &request)?;
        let state = self
            .state
            .lock()
            .map_err(|_| failure(503, "peer_unavailable"))?;
        if state.document.revision != document.revision {
            return Err(failure(409, "peer_configuration_changed"));
        }
        Ok(
            json!({"peerId": peer.peer_id, "revision": document.revision,
            "rasterVerified": true, "raster": receipt.raster, "deliveryAvailable": false, "retained": false}),
        )
    }
}

#[cfg(test)]
#[path = "raster_tests.rs"]
mod tests;
