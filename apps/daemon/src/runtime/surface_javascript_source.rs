// Bounded assembly for JavaScript Surface entries with an optional source descriptor.
const MAX_SURFACE_JAVASCRIPT_SOURCE_FILES: usize = 32;
const MAX_SURFACE_JAVASCRIPT_DESCRIPTOR_BYTES: u64 = 64 * 1024;
const SURFACE_JAVASCRIPT_PREFIX: &[u8] = b"(() => {\n\"use strict\";\n";
const SURFACE_JAVASCRIPT_SUFFIX: &[u8] = b"\n})();\n";
const SURFACE_JAVASCRIPT_SEPARATOR: &[u8] = b"\n;\n";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SurfaceJavascriptSourceDescriptor {
    schema_version: u32,
    source_files: Vec<String>,
}

fn load_surface_javascript_source(
    control_plane_root: &Path,
    art_dir: &Path,
    variant: &loom_protocol::SurfaceVariant,
) -> Result<Vec<u8>> {
    if variant.runtime != SurfaceRuntimeKind::Javascript {
        anyhow::bail!("Surface source assembly requires a JavaScript variant");
    }

    let read_entry = |entry: &str| -> Result<Vec<u8>> {
        let path = resolve_surface_package_entry(control_plane_root, art_dir, entry)
            .map_err(|error| anyhow::anyhow!(error))?;
        let metadata = fs::metadata(&path)
            .with_context(|| format!("read JavaScript Surface metadata {}", path.display()))?;
        if metadata.len() > MAX_SURFACE_JAVASCRIPT_BYTES {
            anyhow::bail!("JavaScript Surface source exceeds {MAX_SURFACE_JAVASCRIPT_BYTES} bytes");
        }
        let source = fs::read(&path)
            .with_context(|| format!("read JavaScript Surface source {}", path.display()))?;
        std::str::from_utf8(&source).context("JavaScript Surface source is not UTF-8")?;
        Ok(source)
    };

    let entry_source = read_entry(&variant.entry)?;
    let descriptor_entry = format!("{}.sources.json", variant.entry);
    let descriptor_candidate = art_dir.join(&descriptor_entry);
    if !descriptor_candidate.try_exists()? {
        return Ok(entry_source);
    }
    let descriptor_path = resolve_surface_package_entry(
        control_plane_root,
        art_dir,
        &descriptor_entry,
    )
    .map_err(|error| anyhow::anyhow!(error))?;
    let metadata = fs::metadata(&descriptor_path)?;
    if metadata.len() > MAX_SURFACE_JAVASCRIPT_DESCRIPTOR_BYTES {
        anyhow::bail!(
            "JavaScript Surface source descriptor exceeds {MAX_SURFACE_JAVASCRIPT_DESCRIPTOR_BYTES} bytes"
        );
    }
    let descriptor: SurfaceJavascriptSourceDescriptor =
        serde_json::from_slice(&fs::read(&descriptor_path)?)
            .context("parse JavaScript Surface source descriptor")?;
    if descriptor.schema_version != 1 {
        anyhow::bail!("JavaScript Surface source descriptor schemaVersion must be 1");
    }
    if descriptor.source_files.is_empty()
        || descriptor.source_files.len() > MAX_SURFACE_JAVASCRIPT_SOURCE_FILES
    {
        anyhow::bail!(
            "JavaScript Surface source descriptor must contain 1 to {MAX_SURFACE_JAVASCRIPT_SOURCE_FILES} files"
        );
    }
    let mut seen = std::collections::HashSet::new();
    for source_file in &descriptor.source_files {
        if Path::new(source_file).extension().and_then(|value| value.to_str()) != Some("js") {
            anyhow::bail!("JavaScript Surface source files must use .js");
        }
        if source_file == &variant.entry || !seen.insert(source_file) {
            anyhow::bail!(
                "JavaScript Surface source files must be unique and must not repeat entry"
            );
        }
    }

    let mut assembled = Vec::new();
    assembled.extend_from_slice(SURFACE_JAVASCRIPT_PREFIX);
    for source in descriptor
        .source_files
        .iter()
        .map(|entry| read_entry(entry))
        .chain(std::iter::once(Ok(entry_source)))
    {
        let source = source?;
        let next_len = assembled
            .len()
            .checked_add(source.len())
            .and_then(|length| length.checked_add(SURFACE_JAVASCRIPT_SEPARATOR.len()))
            .ok_or_else(|| anyhow::anyhow!("JavaScript Surface size overflow"))?;
        if next_len + SURFACE_JAVASCRIPT_SUFFIX.len()
            > MAX_SURFACE_JAVASCRIPT_BYTES as usize
        {
            anyhow::bail!("assembled JavaScript Surface exceeds {MAX_SURFACE_JAVASCRIPT_BYTES} bytes");
        }
        assembled.extend_from_slice(&source);
        assembled.extend_from_slice(SURFACE_JAVASCRIPT_SEPARATOR);
    }
    assembled.extend_from_slice(SURFACE_JAVASCRIPT_SUFFIX);
    Ok(assembled)
}
