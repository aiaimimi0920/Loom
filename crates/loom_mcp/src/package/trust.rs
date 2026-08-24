//! Publisher-signature verification and trust-policy enforcement.

use super::*;

/// Verify a staged MCP server package against the same trust store Art packages use.
///
/// MCP server packages arrive from the same places Arts do, so they get the same chain: an optional
/// `packageSecurity.signature` in the manifest, verified against `plugin-trust.json`, after which
/// the store's effective policy decides whether the resulting status is acceptable. Until now the
/// installer accepted any zip that parsed, which meant an operator who set `require-signed` or
/// `require-trusted` had that setting honoured for Arts and quietly ignored for MCP servers.
///
/// The publisher passed to the verifier is the one the manifest names, so a signature made with a
/// key this machine already trusts for that publisher reaches `Trusted` rather than stopping at
/// `Verified`. The default policy is `allow-unsigned`, so an unsigned package still installs
/// exactly as it did before.
pub(super) fn verify_package_trust(
    control_plane_root: &Path,
    manifest: &McpServerPackageManifest,
    staging_root: &Path,
) -> Result<PackageTrustStatus, McpPackageError> {
    let signature = manifest
        .package_security
        .as_ref()
        .and_then(|security| security.signature.as_ref());
    let publisher = PublisherIdentity {
        id: manifest.publisher.id.clone(),
        name: Some(manifest.publisher.name.clone()),
        website: None,
        key_id: signature.map(|signature| signature.key_id.clone()),
    };
    let trust_store = TrustStore::load(&control_plane_root.join("plugin-trust.json"))
        .map_err(|error| McpPackageError::Trust(error.to_string()))?;
    let status = verify_package_signature(staging_root, Some(&publisher), signature, &trust_store)
        .map_err(|error| McpPackageError::Trust(error.to_string()))?;
    if let Some(signature) = signature {
        enforce_publisher_key_binding(&trust_store, &manifest.publisher.id, signature)?;
    }
    trust_store
        .effective_policy()
        .enforce(status.clone())
        .map_err(|error| McpPackageError::Trust(error.to_string()))?;
    Ok(status)
}

/// Refuse a signature whose key is not one this machine records for the publisher it names.
///
/// `verify_package_signature` reports an unknown `(publisher, key)` pair as `Verified`: the signature
/// checks out, so the package is signed, just not by anyone this machine has an opinion about. That
/// is the right answer for an unknown publisher and the wrong one for a known publisher, because it
/// lets any valid key claim a name the operator has already pinned. Under `require-signed` such a
/// package would install and then be presented under the borrowed publisher's name.
///
/// A publisher with no records at all is left alone: there is no pinned key to contradict, so the
/// policy alone decides whether `Verified` is enough.
pub(super) fn enforce_publisher_key_binding(
    trust_store: &TrustStore,
    publisher_id: &str,
    signature: &PackageSignature,
) -> Result<(), McpPackageError> {
    let mut recorded = trust_store
        .publishers
        .iter()
        .filter(|record| record.publisher_id == publisher_id)
        .peekable();
    if recorded.peek().is_none() {
        return Ok(());
    }
    if recorded.any(|record| record.key_id == signature.key_id) {
        return Ok(());
    }
    Err(McpPackageError::Trust(format!(
        "package claims publisher `{publisher_id}` but is signed with key `{}`, which this machine \
         does not record for that publisher",
        signature.key_id
    )))
}
