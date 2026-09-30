mod operation;
mod response;
#[cfg(test)]
mod response_tests;

use crate::{error, now_ms, CentralResponse, Identity, Policy, Result};
use loom_security::network::{secure_async_client, validate_outbound_url, OutboundPolicy};
pub use operation::CentralOperation;
use std::time::Duration;

#[derive(Clone)]
pub struct CentralClient {
    identity: Identity,
    client: reqwest::Client,
    url: reqwest::Url,
}

impl CentralClient {
    pub fn new(identity: Identity) -> Result<Self> {
        let url = reqwest::Url::parse(&format!("{}/api/loom/projections", identity.origin()))
            .map_err(|_| error(400, "projection_invalid_origin"))?;
        let policy = OutboundPolicy {
            allow_http_loopback: true,
            allow_private_networks: true,
            allowed_domains: vec![],
            max_redirects: 0,
        };
        validate_outbound_url(&url, &policy)
            .map_err(|_| error(400, "projection_invalid_origin"))?;
        let client = secure_async_client("Loom-Projection/2", Duration::from_secs(8), policy)
            .map_err(|_| error(503, "projection_network_unavailable"))?;
        Ok(Self {
            identity,
            client,
            url,
        })
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    pub async fn configuration(&self) -> Result<Policy> {
        match self.execute(&CentralOperation::Configuration).await? {
            CentralResponse::Configuration { policy } => Ok(policy),
            _ => Err(error(502, "projection_response_invalid")),
        }
    }

    pub async fn execute(&self, operation: &CentralOperation) -> Result<CentralResponse> {
        if let Some(envelope) = operation.envelope() {
            envelope.validate(self.identity.origin())?;
            if envelope.source.account_id != self.identity.session().account_id {
                return Err(error(403, "projection_access_denied"));
            }
        }
        let payload = serde_json::to_string(operation)
            .map_err(|_| error(400, "projection_invalid_request"))?;
        let body = serde_json::to_vec(&self.identity.proof(&payload, now_ms())?)
            .map_err(|_| error(400, "projection_invalid_request"))?;
        if body.len() > 16 * 1024 {
            return Err(error(413, "projection_request_budget"));
        }
        let mut response = self
            .client
            .post(self.url.clone())
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|_| error(503, "projection_network_unavailable"))?;
        let status = response.status().as_u16();
        if response
            .content_length()
            .is_some_and(|size| size > 256 * 1024)
        {
            return Err(error(502, "projection_response_invalid"));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| error(503, "projection_network_unavailable"))?
        {
            if bytes.len() + chunk.len() > 256 * 1024 {
                return Err(error(502, "projection_response_invalid"));
            }
            bytes.extend_from_slice(&chunk);
        }
        if status != 200 {
            return Err(response::server_error(status, &bytes));
        }
        let mut value: CentralResponse = serde_json::from_slice(&bytes)
            .map_err(|_| error(502, "projection_response_invalid"))?;
        response::validate(&mut value, operation, &self.identity, now_ms())?;
        Ok(value)
    }
}
