use std::time::Duration;

use loom_plugin_security::TrustStore;
use loom_security::network::{get_bounded, secure_client, OutboundPolicy};

use super::model::{
    CapabilityCatalogDocument, CapabilityCatalogEntry, MAX_CAPABILITY_CATALOG_BYTES,
};
use super::verification::{
    parse_and_verify_capability_catalog, verify_artifact, verify_sha256, CapabilityCatalogError,
};

#[derive(Debug)]
pub struct DownloadedCapabilityPackage {
    pub package_bytes: Vec<u8>,
    pub supply_chain_verified: bool,
}

pub struct CapabilityCatalogClient {
    catalog_url: String,
    policy: OutboundPolicy,
    client: reqwest::blocking::Client,
}

impl CapabilityCatalogClient {
    pub fn new(
        catalog_url: impl Into<String>,
        policy: OutboundPolicy,
    ) -> Result<Self, CapabilityCatalogError> {
        let client = secure_client(
            "loom-capability-catalog/1",
            Duration::from_secs(30),
            policy.clone(),
        )
        .map_err(CapabilityCatalogError::Network)?;
        Ok(Self {
            catalog_url: catalog_url.into(),
            policy,
            client,
        })
    }

    pub fn fetch(
        &self,
        trust_store: &TrustStore,
    ) -> Result<CapabilityCatalogDocument, CapabilityCatalogError> {
        let bytes = get_bounded(
            &self.client,
            &self.catalog_url,
            &self.policy,
            MAX_CAPABILITY_CATALOG_BYTES,
        )
        .map_err(CapabilityCatalogError::Network)?;
        parse_and_verify_capability_catalog(&bytes, trust_store, chrono::Utc::now())
    }

    pub fn download(
        &self,
        entry: &CapabilityCatalogEntry,
    ) -> Result<DownloadedCapabilityPackage, CapabilityCatalogError> {
        let package = self.download_bytes(&entry.package.url, entry.package.bytes)?;
        verify_sha256(&package, &entry.package.sha256)?;
        let sbom = self.download_bytes(&entry.sbom.url, entry.sbom.bytes)?;
        verify_artifact(&sbom, &entry.sbom)?;
        let provenance = self.download_bytes(&entry.provenance.url, entry.provenance.bytes)?;
        verify_artifact(&provenance, &entry.provenance)?;
        Ok(DownloadedCapabilityPackage {
            package_bytes: package,
            supply_chain_verified: true,
        })
    }

    fn download_bytes(
        &self,
        url: &str,
        declared_bytes: u64,
    ) -> Result<Vec<u8>, CapabilityCatalogError> {
        let limit = usize::try_from(declared_bytes)
            .map_err(|_| CapabilityCatalogError::Invalid("artifact is too large".to_owned()))?;
        let bytes = get_bounded(&self.client, url, &self.policy, limit)
            .map_err(CapabilityCatalogError::Network)?;
        if bytes.len() != limit {
            return Err(CapabilityCatalogError::Invalid(
                "artifact byte length changed".to_owned(),
            ));
        }
        Ok(bytes)
    }
}
