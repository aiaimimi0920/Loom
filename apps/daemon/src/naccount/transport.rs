use super::{error, Result, ISSUER};
use loom_security::network::{secure_client, OutboundPolicy};
use reqwest::blocking::{Client, Response};
use serde_json::Value;
use std::{io::Read, time::Duration};

pub(super) fn client() -> Result<Client> {
    secure_client(
        "Loom-NAccount/1",
        Duration::from_secs(10),
        OutboundPolicy {
            allow_http_loopback: false,
            allow_private_networks: false,
            allowed_domains: vec![],
            max_redirects: 0,
        },
    )
    .map_err(|_| error(503, "naccount_network_unavailable"))
}
pub(super) fn json(response: Response) -> Result<Value> {
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(error(
            if status == 400 || status == 401 {
                401
            } else {
                503
            },
            "naccount_request_rejected",
        ));
    }
    let mut bytes = Vec::new();
    response
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| error(503, "naccount_network_unavailable"))?;
    if bytes.len() > 65536 {
        return Err(error(502, "naccount_response_invalid"));
    }
    serde_json::from_slice(&bytes).map_err(|_| error(502, "naccount_response_invalid"))
}
pub(super) fn token(params: &[(&str, &str)]) -> Result<Value> {
    json(
        client()?
            .post(format!("{ISSUER}/oauth2/v1/token"))
            .form(params)
            .send()
            .map_err(|_| error(503, "naccount_network_unavailable"))?,
    )
}
pub(super) fn profile(token: &str) -> Result<Value> {
    json(
        client()?
            .get(format!("{ISSUER}/oauth2/v1/userinfo"))
            .bearer_auth(token)
            .send()
            .map_err(|_| error(503, "naccount_network_unavailable"))?,
    )
}
pub(super) fn revoke(client_id: &str, refresh: &str) -> bool {
    let Ok(client) = client() else {
        return false;
    };
    client
        .post(format!("{ISSUER}/oauth2/v1/revoke"))
        .basic_auth(client_id, Some(""))
        .form(&[("token", refresh), ("token_type_hint", "refresh_token")])
        .send()
        .is_ok_and(|response| response.status().is_success())
}
