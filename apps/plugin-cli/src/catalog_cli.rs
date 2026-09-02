// Canonical signing and validation for the official Capability Plugin catalog.
const MAX_CAPABILITY_CATALOG_INPUT_BYTES: u64 = 2 * 1024 * 1024;

fn sign_capability_catalog(
    payload_path: &Path,
    key_path: &Path,
    output_path: &Path,
) -> Result<String> {
    use loom_tool_registry::capability::{
        parse_and_verify_capability_catalog, CapabilityCatalogDocument,
        CapabilityCatalogPayload, CapabilityCatalogSignature, CAPABILITY_CATALOG_SCHEMA_VERSION,
        OFFICIAL_CAPABILITY_CATALOG_PUBLISHER_ID,
    };

    let payload_bytes = read_bounded_regular_file(
        payload_path,
        MAX_CAPABILITY_CATALOG_INPUT_BYTES,
    )
    .with_context(|| format!("read catalog payload {}", payload_path.display()))?;
    let payload: CapabilityCatalogPayload = serde_json::from_slice(&payload_bytes)
        .with_context(|| format!("parse catalog payload {}", payload_path.display()))?;
    if payload.schema_version != CAPABILITY_CATALOG_SCHEMA_VERSION {
        bail!("unsupported Capability catalog schema version");
    }
    if payload.publisher.id != OFFICIAL_CAPABILITY_CATALOG_PUBLISHER_ID {
        bail!("the official Capability catalog publisher must be `neuro.official`");
    }
    let key = read_signing_key_document(key_path)?;
    if payload.publisher.key_id != key.key_id {
        bail!("catalog publisher keyId does not match the signing key");
    }
    let canonical_payload = serde_json::to_vec(&payload)?;
    let signature = sign_message(&key, &canonical_payload)?;
    let document = CapabilityCatalogDocument {
        signed: payload,
        signature: CapabilityCatalogSignature {
            algorithm: "ed25519".to_owned(),
            key_id: key.key_id.clone(),
            value: signature,
        },
    };

    // Run the same parser used by the daemon before publishing any catalog bytes.
    let mut trust = TrustStore::default();
    trust.trust(PublisherTrustRecord {
        publisher_id: OFFICIAL_CAPABILITY_CATALOG_PUBLISHER_ID.to_owned(),
        key_id: key.key_id.clone(),
        public_key: key.public_key.clone(),
        revoked: false,
    });
    let document_bytes = serde_json::to_vec(&document)?;
    parse_and_verify_capability_catalog(&document_bytes, &trust, chrono::Utc::now())
        .context("validate signed Capability catalog")?;
    write_pretty_json(output_path.to_path_buf(), &serde_json::to_value(&document)?)?;
    Ok(format!(
        "Capability catalog signed: keyId={}, packages={}",
        key.key_id,
        document.signed.packages.len()
    ))
}

fn validate_capability_catalog(catalog_path: &Path, trust_store_path: &Path) -> Result<String> {
    let bytes = read_bounded_regular_file(
        catalog_path,
        MAX_CAPABILITY_CATALOG_INPUT_BYTES,
    )
    .with_context(|| format!("read Capability catalog {}", catalog_path.display()))?;
    let trust = TrustStore::load(trust_store_path)?;
    let document = loom_tool_registry::capability::parse_and_verify_capability_catalog(
        &bytes,
        &trust,
        chrono::Utc::now(),
    )?;
    Ok(format!(
        "Capability catalog valid: publisher={}, packages={}",
        document.signed.publisher.id,
        document.signed.packages.len()
    ))
}
