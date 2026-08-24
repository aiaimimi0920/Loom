// Package signing and publisher trust-store mutations.
fn sign_plugin_package(directory: &Path, key_path: &Path, publisher_id: &str) -> Result<String> {
    if !is_safe_publisher_id(publisher_id) {
        bail!("publisher id is not safe: {publisher_id}");
    }
    ensure_real_directory(directory, "package root")?;
    collect_package_files(directory).context("inspect package before signing")?;
    let key = read_signing_key_document(key_path)?;
    let signature = json!({
        "algorithm": "ed25519",
        "keyId": key.key_id.clone(),
        "file": "signature.json"
    });
    if contained_regular_file_exists(directory, Path::new("framework.manifest.json"))? {
        let path = directory.join("framework.manifest.json");
        let mut manifest: Value = read_json(&path)?;
        let object = manifest
            .as_object_mut()
            .ok_or_else(|| anyhow!("framework manifest must be an object"))?;
        object.insert(
            "publisher".to_owned(),
            json!({ "id": publisher_id, "keyId": key.key_id.clone() }),
        );
        object.insert("signature".to_owned(), signature);
        write_pretty_json(path, &manifest)?;
    } else if contained_regular_file_exists(directory, Path::new("manifest.json"))? {
        let path = directory.join("manifest.json");
        let mut manifest: Value = read_json(&path)?;
        let object = manifest
            .as_object_mut()
            .ok_or_else(|| anyhow!("Art manifest must be an object"))?;
        let metadata = object
            .entry("metadata".to_owned())
            .or_insert_with(|| json!({}));
        let metadata = metadata
            .as_object_mut()
            .ok_or_else(|| anyhow!("Art metadata must be an object"))?;
        let security = metadata
            .entry("packageSecurity".to_owned())
            .or_insert_with(|| json!({}));
        let security = security
            .as_object_mut()
            .ok_or_else(|| anyhow!("Art packageSecurity metadata must be an object"))?;
        let publisher = security
            .entry("publisher".to_owned())
            .or_insert_with(|| json!({}));
        let publisher = publisher
            .as_object_mut()
            .ok_or_else(|| anyhow!("Art publisher metadata must be an object"))?;
        publisher.insert("id".to_owned(), json!(publisher_id));
        publisher.insert("keyId".to_owned(), json!(key.key_id.clone()));
        security.insert("signature".to_owned(), signature);
        write_pretty_json(path, &manifest)?;
    } else {
        bail!("package directory has no framework.manifest.json or manifest.json");
    }
    ensure_safe_destination(&directory.join("signature.json"))?;
    let document = sign_package(directory, "signature.json", &key)?;
    Ok(format!(
        "package signed: publisher={publisher_id}, keyId={}, digest={}",
        document.key_id, document.digest
    ))
}

fn trust_publisher(store_path: &Path, publisher_id: &str, key_path: &Path) -> Result<()> {
    if !is_safe_publisher_id(publisher_id) {
        bail!("publisher id is not safe: {publisher_id}");
    }
    let key = read_signing_key_document(key_path)?;
    let mut store = TrustStore::load(store_path)?;
    store.trust(PublisherTrustRecord {
        publisher_id: publisher_id.to_owned(),
        key_id: key.key_id,
        public_key: key.public_key,
        revoked: false,
    });
    store.write_atomic(store_path)?;
    Ok(())
}

fn read_signing_key_document(path: &Path) -> Result<SigningKeyDocument> {
    let bytes = read_bounded_regular_file(path, MAX_SIGNING_KEY_BYTES)
        .with_context(|| format!("read signing key {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse signing key {}", path.display()))
}

fn write_signing_key_document(path: &Path, document: &SigningKeyDocument) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(document)?;
    bytes.push(b'\n');
    write_private_bytes_atomic(path, &bytes)
        .with_context(|| format!("write signing key {}", path.display()))
}

fn revoke_publisher(store_path: &Path, publisher_id: &str, key_id: &str) -> Result<()> {
    if !is_safe_publisher_id(publisher_id) || key_id.trim().is_empty() {
        bail!("publisher id or key id is invalid");
    }
    let mut store = TrustStore::load(store_path)?;
    if !store.revoke(publisher_id, key_id) {
        bail!("publisher key was not found in trust store");
    }
    store.write_atomic(store_path)?;
    Ok(())
}
