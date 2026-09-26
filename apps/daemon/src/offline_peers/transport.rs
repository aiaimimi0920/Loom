//! One bounded transport policy for signed offline control messages; no credentials forwarded.
use super::*;

pub(super) fn request<T: serde::de::DeserializeOwned>(
    peer: &Peer,
    action: &'static str,
    body: &impl Serialize,
    limit: usize,
    seconds: u64,
) -> PeerResult<T> {
    let policy = OutboundPolicy {
        allow_http_loopback: true,
        allow_private_networks: true,
        max_redirects: 0,
        ..OutboundPolicy::default()
    };
    let url = reqwest::Url::parse(&format!("{}/v1/projection-peer/{action}", peer.origin))
        .map_err(|_| failure(400, "peer_invalid_configuration"))?;
    validate_outbound_url(&url, &policy).map_err(|_| failure(400, "peer_invalid_configuration"))?;
    let client = secure_client("Loom-Offline-Peer/1", Duration::from_secs(seconds), policy)
        .map_err(|_| failure(503, "peer_transport_unavailable"))?;
    let response = client
        .post(url)
        .json(body)
        .send()
        .map_err(|_| failure(502, "peer_unreachable"))?;
    if !response.status().is_success() {
        return Err(failure(502, "peer_handshake_rejected"));
    }
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err(failure(502, "peer_response_too_large"));
    }
    let mut bytes = Vec::new();
    response
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| failure(502, "peer_unreachable"))?;
    if bytes.len() > limit {
        return Err(failure(502, "peer_response_too_large"));
    }
    serde_json::from_slice(&bytes).map_err(|_| failure(502, "peer_invalid_response"))
}
