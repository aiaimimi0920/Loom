use super::*;
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Packet {
    challenge: Challenge,
    payload: Value,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Reply {
    challenge: Challenge,
    payload: Value,
    signature: String,
}
fn purpose(payload: &Value, response: bool) -> PeerResult<String> {
    let bytes =
        serde_json::to_vec(payload).map_err(|_| failure(400, "projection_invalid_request"))?;
    Ok(format!(
        "transfer-{}\n{}",
        if response { "response" } else { "request" },
        sha256_bytes(&bytes)
    ))
}
fn remote_error(payload: &Value) -> PeerError {
    const CODES: &[&str] = &[
        "projection_not_found",
        "projection_unlinked",
        "projection_rejected",
        "projection_invitation_expired",
        "projection_invitation_mismatch",
        "projection_access_denied",
        "projection_peer_revoked",
        "projection_target_offline",
        "projection_target_unavailable",
        "projection_content_changed",
        "projection_revision_conflict",
        "projection_invalid_receipt",
        "projection_invitation_consumed",
        "projection_store_full",
        "projection_source_limit",
        "projection_storage_unavailable",
        "projection_invalid_request",
        "projection_busy",
        "projection_confirmation_required",
        "projection_invitation_replayed",
    ];
    let code = CODES
        .iter()
        .find(|code| payload["error"]["code"].as_str() == Some(**code));
    let status = payload["error"]["status"]
        .as_u64()
        .filter(|v| (400..=599).contains(v));
    match (code, status) {
        (Some(code), Some(status)) => failure(status as u16, code),
        _ => failure(502, "projection_peer_unavailable"),
    }
}
impl OfflinePeers {
    pub(in super::super) fn exchange(
        &self,
        peer_id: &str,
        expected_revision: u64,
        payload: Value,
    ) -> PeerResult<Value> {
        let document = self
            .state
            .lock()
            .map_err(|_| failure(503, "peer_unavailable"))?
            .document
            .clone();
        if document.revision != expected_revision {
            return Err(failure(403, "projection_peer_revoked"));
        }
        let peer = document
            .peers
            .get(peer_id)
            .filter(|p| p.enabled)
            .ok_or_else(|| failure(403, "projection_peer_revoked"))?;
        let mut challenge = Challenge {
            source_id: document.identity.key_id.clone(),
            target_id: peer_id.to_owned(),
            nonce: Uuid::new_v4().simple().to_string(),
            timestamp_ms: unix_time_millis(),
            signature: String::new(),
        };
        challenge.signature = sign_message(
            &document.identity,
            &challenge.message(&purpose(&payload, false)?),
        )
        .map_err(|_| failure(503, "peer_unavailable"))?;
        let packet = Packet { challenge, payload };
        let reply: Reply = transport::request(peer, "transfer", &packet, MAX_BODY, 5)?;
        packet.challenge.validate(unix_time_millis())?;
        if reply.challenge != packet.challenge {
            return Err(failure(403, "peer_invalid_proof"));
        }
        loom_plugin_security::verify_message(
            &peer.public_key,
            &reply.challenge.message(&purpose(&reply.payload, true)?),
            &reply.signature,
        )
        .map_err(|_| failure(403, "peer_invalid_proof"))?;
        if self
            .state
            .lock()
            .map_err(|_| failure(503, "peer_unavailable"))?
            .document
            .revision
            != expected_revision
        {
            return Err(failure(403, "projection_peer_revoked"));
        }
        if reply.payload.get("error").is_some() {
            return Err(remote_error(&reply.payload));
        }
        Ok(reply.payload)
    }
    pub(crate) fn accept_transfer(
        &self,
        body: &str,
        registry: &SharedDeviceRegistryStore,
    ) -> PeerResult<Value> {
        if self
            .transfer_receiving
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(failure(429, "projection_busy"));
        }
        let _guard = ProbeGuard(&self.transfer_receiving);
        let packet: Packet = parse_transfer(body)?;
        let revision = self
            .authenticate_challenge(&packet.challenge, &purpose(&packet.payload, false)?)?
            .document
            .revision;
        let result = self.peer_action(
            &packet.challenge.source_id,
            revision,
            packet.payload,
            registry,
        );
        let payload = match result {
            Ok(value) => value,
            Err(error) => json!({"error": {"status": error.status, "code": error.code}}),
        };
        let state = self
            .state
            .lock()
            .map_err(|_| failure(503, "peer_unavailable"))?;
        if state.document.revision != revision {
            return Err(failure(403, "projection_peer_revoked"));
        }
        packet.challenge.validate(unix_time_millis())?;
        let signature = sign_message(
            &state.document.identity,
            &packet.challenge.message(&purpose(&payload, true)?),
        )
        .map_err(|_| failure(503, "peer_unavailable"))?;
        serde_json::to_value(Reply {
            challenge: packet.challenge,
            payload,
            signature,
        })
        .map_err(|_| failure(503, "peer_unavailable"))
    }
}
