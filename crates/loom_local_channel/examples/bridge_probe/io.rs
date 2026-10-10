use loom_local_channel::BridgeDiscovery;
use serde::Deserialize;
use std::fs::OpenOptions;
use std::io::Read;
use std::path::Path;

pub(super) fn read_manifest(path: &Path) -> Result<BridgeDiscovery, ()> {
    if !path.is_absolute() {
        return Err(());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|_| ())?;
    let metadata = file.metadata().map_err(|_| ())?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err(());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(());
        }
    }
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() > 1024 * 1024 {
        return Err(());
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Manifest {
        hook_bridge: Option<BridgeDiscovery>,
    }
    let manifest: Manifest = serde_json::from_slice(&bytes).map_err(|_| ())?;
    let discovery = manifest.hook_bridge.ok_or(())?;
    discovery.validate().map_err(|_| ())?;
    Ok(discovery)
}
