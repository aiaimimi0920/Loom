// One native owner republishes HTTP discovery and the current bridge identity atomically.
struct LocalCapabilityManifest {
    path: PathBuf,
    document: Value,
}

fn write_local_capability_manifest(
    manifest_dir: &Path,
    address: SocketAddr,
    auth_token: Option<&str>,
) -> Result<LocalCapabilityManifest> {
    fs::create_dir_all(manifest_dir).context("create Loom manifest directory")?;
    restrict_sensitive_path_permissions(manifest_dir, true)
        .context("restrict Loom manifest directory")?;
    let mut transport = json!({
        "type": "http", "baseUrl": format!("http://{}", address), "auth": "none"
    });
    if let Some(token) = auth_token {
        transport["auth"] = Value::String("bearer".to_owned());
        transport["authToken"] = Value::String(token.to_owned());
    }
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("read system time for Loom manifest")?
        .as_secs();
    let owner = LocalCapabilityManifest {
        path: manifest_dir.join("loom.json"),
        document: json!({
            "schemaVersion": 1, "appId": "loom", "displayName": "Loom",
            "version": loom_core::LOOM_VERSION, "pid": std::process::id(),
            "transport": transport, "capabilities": invokable_capability_ids(),
            "startedAt": started_at, "hookBridge": null
        }),
    };
    owner.publish_bridge(None)?;
    Ok(owner)
}

impl LocalCapabilityManifest {
    fn publish_bridge(&self, bridge: Option<&loom_local_channel::BridgeDiscovery>) -> Result<()> {
        // The immutable HTTP metadata and start identity survive bridge restarts unchanged.
        let mut document = self.document.clone();
        document["hookBridge"] = serde_json::to_value(bridge)?;
        let mut bytes = serde_json::to_vec_pretty(&document)?;
        bytes.push(b'\n');
        let (temporary, mut file) =
            create_sensitive_temporary(&self.path).context("create Loom discovery temporary")?;
        let result = (|| -> Result<()> {
            file.write_all(&bytes).context("write Loom discovery")?;
            file.sync_all().context("flush Loom discovery")?;
            drop(file);
            restrict_sensitive_path_permissions(&temporary, false)?;
            if self.path.is_file() {
                restrict_sensitive_path_permissions(&self.path, false)?;
            }
            replace_sensitive_file(&temporary, &self.path)?;
            restrict_sensitive_path_permissions(&self.path, false)?;
            sync_sensitive_parent(&self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.context("publish private Loom discovery")
    }
}
