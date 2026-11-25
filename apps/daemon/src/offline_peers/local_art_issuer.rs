//! Reuse the persistent daemon identity, never the package publisher key or peer trust decisions.
use super::*;

impl OfflinePeers {
    pub(crate) fn issue_local_art_visibility_package(
        &self,
        package: &loom_protocol::local_art::package::visibility::Package,
    ) -> anyhow::Result<Value> {
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("issuer unavailable"))?;
        let bytes = loom_plugin_security::local_art_grant::issue_visibility_package(
            &state.document.identity,
            package,
        )
        .map_err(|_| anyhow::anyhow!("local Art visibility package signing failed"))?;
        Ok(serde_json::from_slice(&bytes)?)
    }
    pub(crate) fn verify_local_art(
        &self,
        envelope: &loom_protocol::local_art::Envelope,
    ) -> anyhow::Result<loom_protocol::local_art::Grant> {
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("issuer unavailable"))?;
        let key = &state.document.identity;
        loom_plugin_security::local_art_grant::verify(
            &serde_json::to_vec(envelope)?,
            &key.key_id,
            &key.public_key,
        )
        .map_err(|_| anyhow::anyhow!("invalid local Art grant"))
    }
    pub(crate) fn local_art_issuer(&self) -> anyhow::Result<Value> {
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("issuer unavailable"))?;
        let key = &state.document.identity;
        Ok(
            json!({"keyId":key.key_id,"publicKey":key.public_key,"algorithm":"ed25519",
            "supportedPrepareProtocols":[loom_protocol::local_art::package::visibility::PREPARE]}),
        )
    }

    pub(crate) fn issue_local_art(
        &self,
        grant: &loom_protocol::local_art::Grant,
    ) -> anyhow::Result<Vec<u8>> {
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("issuer unavailable"))?;
        loom_plugin_security::local_art_grant::issue(&state.document.identity, grant)
            .map_err(|_| anyhow::anyhow!("local Art signing failed"))
    }
}
