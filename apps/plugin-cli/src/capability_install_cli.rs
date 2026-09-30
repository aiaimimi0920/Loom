// Large local packages use the same locked/verifying registry installer without
// base64 expansion or raising the daemon's HTTP request limits.
fn install_local_capability(archive: &Path, control_plane: &Path) -> Result<Value> {
    use loom_tool_registry::capability::{
        install_capability_from_zip, CapabilityPluginRegistry, MAX_CAPABILITY_PACKAGE_DOWNLOAD_BYTES,
    };
    if !archive.is_absolute() || !control_plane.is_absolute() {
        bail!("capability archive and control-plane paths must be absolute");
    }
    let root = ensure_real_directory(control_plane, "control-plane root")?;
    let file = open_regular_file(archive, "capability archive")?;
    let maximum = MAX_CAPABILITY_PACKAGE_DOWNLOAD_BYTES as u64;
    let size = file.metadata()?.len();
    if size == 0 || size > maximum {
        bail!("capability archive exceeds the local package size limit");
    }
    let mut bytes = Vec::new();
    file.take(maximum + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        bail!("capability archive grew beyond the local package size limit");
    }
    let registry = CapabilityPluginRegistry::new(root);
    let report = install_capability_from_zip(&bytes, &registry)?;
    // Installation never grants permissions, enables code or creates trust.
    Ok(serde_json::to_value(report)?)
}
