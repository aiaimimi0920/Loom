//! 只读取immutable resolver确认的包内声明；路径不进入wire，所有声明先受同一运行预算约束。
use super::*;
use loom_protocol::local_art::package::{validate_resources, Resource};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Entry {
    pub entry: String,
    pub resource: Resource,
}
pub(super) fn validate(entries: &[Entry]) -> LocalResult<()> {
    if entries.len() > 16 {
        return Err(fail(409, "local_art_resource_limit"));
    }
    validate_resources(
        &entries
            .iter()
            .map(|e| e.resource.clone())
            .collect::<Vec<_>>(),
    )
    .map_err(|_| fail(409, "local_art_resource_limit"))?;
    for entry in entries {
        if entry.entry.len() > 512
            || entry.entry.contains(['\\', ':'])
            || entry
                .entry
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
        {
            return Err(fail(409, "local_art_resource_path"));
        }
    }
    Ok(())
}
pub(super) fn read(s: &Services<'_>, directory: &Path, entry: &Entry) -> LocalResult<Vec<u8>> {
    let r = &entry.resource;
    let path = resolve_surface_package_entry(s.root, directory, &entry.entry)
        .map_err(|_| fail(409, "local_art_resource_path"))?;
    let file = fs::File::open(path).map_err(|_| fail(409, "local_art_resource_missing"))?;
    if file
        .metadata()
        .map_err(|_| fail(409, "local_art_resource_missing"))?
        .len()
        != r.size
    {
        return Err(fail(409, "local_art_resource_invalid"));
    }
    let mut bytes = Vec::new();
    file.take(r.size + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| fail(409, "local_art_resource_missing"))?;
    if bytes.len() as u64 != r.size
        || sha256_bytes(&bytes) != r.digest
        || (matches!(r.mime_type.as_str(), "text/plain" | "application/json")
            && (bytes.contains(&0) || std::str::from_utf8(&bytes).is_err()))
    {
        return Err(fail(409, "local_art_resource_invalid"));
    }
    if r.mime_type == "application/json" && serde_json::from_slice::<Value>(&bytes).is_err() {
        return Err(fail(409, "local_art_resource_invalid"));
    }
    if r.mime_type == "image/png"
        && (bytes.len() < 33
            || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
            || &bytes[12..16] != b"IHDR"
            || u32::from_be_bytes(bytes[16..20].try_into().unwrap()) != r.dimensions[0]
            || u32::from_be_bytes(bytes[20..24].try_into().unwrap()) != r.dimensions[1])
    {
        return Err(fail(409, "local_art_resource_invalid"));
    }
    Ok(bytes)
}
