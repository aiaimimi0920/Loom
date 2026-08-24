//! Persistent tool registry operations.

use super::*;

pub(super) const MAX_TOOL_REGISTRY_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct ToolRegistry {
    root: PathBuf,
}

impl ToolRegistry {
    #[must_use]
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn save_tool(&self, tool: ToolDefinition) -> ToolRegistryResult<ToolDefinition> {
        self.save_tool_inner(tool, false)
    }

    pub(crate) fn save_packaged_tool(
        &self,
        tool: ToolDefinition,
    ) -> ToolRegistryResult<ToolDefinition> {
        self.save_tool_inner(tool, true)
    }

    fn save_tool_inner(
        &self,
        mut tool: ToolDefinition,
        replace_unpublished: bool,
    ) -> ToolRegistryResult<ToolDefinition> {
        self.apply_persisted_art_settings(&mut tool)?;
        tool.validate()?;
        self.ensure_root()?;

        let mut tools = self.read_tools()?;
        if replace_unpublished && tool.publisher_identity().is_some() {
            tools.retain(|existing| {
                existing.id != tool.id || existing.publisher_identity().is_some()
            });
        }
        let qualified_id = tool.qualified_id();
        if let Some(existing) = tools
            .iter_mut()
            .find(|existing| existing.qualified_id() == qualified_id)
        {
            *existing = tool.clone();
        } else {
            tools.push(tool.clone());
        }
        sort_tools(&mut tools);
        self.write_tools(&tools)?;
        Ok(tool)
    }

    pub fn list_tools(&self) -> ToolRegistryResult<Vec<ToolDefinition>> {
        self.ensure_root()?;
        let mut tools = self.read_tools()?;
        sort_tools(&mut tools);
        Ok(tools)
    }

    pub fn get_tool(&self, id: &str) -> ToolRegistryResult<Option<ToolDefinition>> {
        let tools = self.list_tools()?;
        if let Some(tool) = tools.iter().find(|tool| tool.qualified_id() == id) {
            return Ok(Some(tool.clone()));
        }
        let mut matches = tools.into_iter().filter(|tool| tool.id == id);
        let first = matches.next();
        if first.is_some() && matches.next().is_some() {
            return Err(ToolRegistryError::AmbiguousToolId { id: id.to_owned() });
        }
        Ok(first)
    }

    pub fn delete_tool(&self, id: &str) -> ToolRegistryResult<bool> {
        self.ensure_root()?;
        let mut tools = self.read_tools()?;
        let exact = tools
            .iter()
            .position(|tool| tool.qualified_id() == id)
            .or_else(|| {
                let matches = tools
                    .iter()
                    .enumerate()
                    .filter(|(_, tool)| tool.id == id)
                    .map(|(index, _)| index)
                    .collect::<Vec<_>>();
                if matches.len() == 1 {
                    Some(matches[0])
                } else {
                    None
                }
            });
        if exact.is_none() && tools.iter().filter(|tool| tool.id == id).count() > 1 {
            return Err(ToolRegistryError::AmbiguousToolId { id: id.to_owned() });
        }
        let before = tools.len();
        if let Some(index) = exact {
            tools.remove(index);
        }
        let deleted = tools.len() != before;
        if deleted {
            self.write_tools(&tools)?;
        }
        Ok(deleted)
    }

    fn ensure_root(&self) -> ToolRegistryResult<()> {
        fs::create_dir_all(&self.root)?;
        Ok(())
    }

    fn tools_path(&self) -> PathBuf {
        self.root.join(TOOLS_FILE)
    }

    fn read_tools(&self) -> ToolRegistryResult<Vec<ToolDefinition>> {
        let path = self.tools_path();
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content = read_bounded_registry_file(&path)?;
        let mut tools = match serde_json::from_str(&content) {
            Ok(tools) => tools,
            Err(error) => {
                let Some(tools) = recover_tools_with_trailing_delimiters(&content) else {
                    return Err(ToolRegistryError::Json(error));
                };
                self.write_corruption_backup(&content)?;
                self.write_tools(&tools)?;
                tools
            }
        };
        for tool in &mut tools {
            self.apply_persisted_art_settings(tool)?;
        }
        Ok(tools)
    }

    fn apply_persisted_art_settings(&self, tool: &mut ToolDefinition) -> ToolRegistryResult<()> {
        let Some(control_plane_root) = self.root.parent().filter(|_| {
            self.root
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("tools"))
        }) else {
            return Ok(());
        };
        // A preferences lookup must never fail a registry read. The store now recovers from a
        // damaged settings file on its own, but the id validation in `get_optional` can still
        // reject a qualified id that `ToolDefinition::validate` accepted, and the read itself can
        // fail for reasons that have nothing to do with this tool (a permission error on the
        // control-plane directory). In every one of those cases the honest answer is "this Art has
        // no stored settings", not "the registry is unreadable and every Art disappears".
        let settings = art_settings::ArtSettingsStore::new(control_plane_root)
            .get_optional(&tool.qualified_id())
            .unwrap_or_default();
        if let Some(metadata) = tool
            .metadata
            .as_mut()
            .and_then(serde_json::Value::as_object_mut)
        {
            metadata.remove("artUserSettings");
        }
        if let Some(settings) = settings {
            art_settings::apply_settings_metadata(tool, &settings);
        }
        Ok(())
    }

    fn write_tools(&self, tools: &[ToolDefinition]) -> ToolRegistryResult<()> {
        let content = serde_json::to_string_pretty(tools)?;
        let (temporary_path, mut temporary_file) = self.create_transient_file("tmp")?;
        if let Err(error) = temporary_file
            .write_all(content.as_bytes())
            .and_then(|()| temporary_file.sync_all())
        {
            let _ = fs::remove_file(&temporary_path);
            return Err(ToolRegistryError::Io(error));
        }
        drop(temporary_file);

        if let Err(error) = replace_registry_file(&temporary_path, &self.tools_path()) {
            let _ = fs::remove_file(&temporary_path);
            return Err(ToolRegistryError::Io(error));
        }
        Ok(())
    }

    fn write_corruption_backup(&self, content: &str) -> ToolRegistryResult<PathBuf> {
        let (backup_path, mut backup_file) = self.create_transient_file("corrupt")?;
        if let Err(error) = backup_file
            .write_all(content.as_bytes())
            .and_then(|()| backup_file.sync_all())
        {
            let _ = fs::remove_file(&backup_path);
            return Err(ToolRegistryError::Io(error));
        }
        Ok(backup_path)
    }

    fn create_transient_file(&self, marker: &str) -> ToolRegistryResult<(PathBuf, File)> {
        for _ in 0..100 {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let sequence = REGISTRY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = self.root.join(format!(
                "{TOOLS_FILE}.{marker}-{}-{timestamp}-{sequence}",
                std::process::id()
            ));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok((path, file)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(ToolRegistryError::Io(error)),
            }
        }

        Err(ToolRegistryError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not allocate a unique tool registry temporary file",
        )))
    }
}

pub(super) fn recover_tools_with_trailing_delimiters(content: &str) -> Option<Vec<ToolDefinition>> {
    let mut stream = serde_json::Deserializer::from_str(content).into_iter::<Vec<ToolDefinition>>();
    let tools = stream.next()?.ok()?;
    let trailing = content.get(stream.byte_offset()..)?;
    if trailing.trim().is_empty()
        || !trailing
            .chars()
            .all(|character| character.is_whitespace() || matches!(character, '}' | ']'))
    {
        return None;
    }
    Some(tools)
}

fn read_bounded_registry_file(path: &Path) -> std::io::Result<String> {
    let file = File::open(path)?;
    let capacity = file
        .metadata()?
        .len()
        .min((MAX_TOOL_REGISTRY_BYTES + 1) as u64) as usize;
    let mut bytes = Vec::with_capacity(capacity);
    file.take((MAX_TOOL_REGISTRY_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_TOOL_REGISTRY_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("tool registry exceeds {MAX_TOOL_REGISTRY_BYTES} bytes"),
        ));
    }
    String::from_utf8(bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

#[cfg(not(windows))]
pub(crate) fn replace_registry_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
pub(crate) fn replace_registry_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    fn extended_length_path(path: &Path) -> std::io::Result<Vec<u16>> {
        // Canonicalize only the parent. Following the final component would turn a
        // destination symlink into an overwrite of its target instead of replacing
        // the directory entry selected by the caller.
        let parent = path.parent().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "registry file path has no parent",
            )
        })?;
        let file_name = path.file_name().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "registry file path has no file name",
            )
        })?;
        let absolute = fs::canonicalize(parent)?.join(file_name);
        let wide = absolute.as_os_str().encode_wide().collect::<Vec<_>>();
        let mut extended =
            if wide.starts_with(&[b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16])
                || wide.starts_with(&[b'\\' as u16, b'\\' as u16, b'.' as u16, b'\\' as u16])
            {
                wide
            } else if wide.starts_with(&[b'\\' as u16, b'\\' as u16]) {
                let mut path = r"\\?\UNC\".encode_utf16().collect::<Vec<_>>();
                path.extend_from_slice(&wide[2..]);
                path
            } else {
                let mut path = r"\\?\".encode_utf16().collect::<Vec<_>>();
                path.extend_from_slice(&wide);
                path
            };
        extended.push(0);
        Ok(extended)
    }

    let source = extended_length_path(source)?;
    let destination = extended_length_path(destination)?;
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

pub(super) fn sort_tools(tools: &mut [ToolDefinition]) {
    tools.sort_by_key(ToolDefinition::qualified_id);
}
