// One bounded active document. Byte identity protects same-size/timestamp session rewrites;
// candidate metadata also tracks missing files, symlink retargeting and allowed-root changes.
#[derive(Default)]
pub(crate) struct HookCanvasDocumentCache {
    entry: Option<CachedDocument>,
}

struct CachedDocument {
    path: PathBuf,
    bytes: Vec<u8>,
    updated_at: Option<String>,
    candidates: Vec<PathBuf>,
    roots: Vec<PathBuf>,
    stamps: Vec<Option<(PathBuf, u64, Option<std::time::SystemTime>)>>,
    document: std::sync::Arc<HookCanvasDocument>,
}

impl HookCanvasDocumentCache {
    pub(crate) fn read(
        &mut self,
        path: &Path,
    ) -> Result<std::sync::Arc<HookCanvasDocument>, HookCanvasError> {
        let bytes = match read_session_bytes(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                self.entry = None;
                return Ok(std::sync::Arc::new(HookCanvasDocument::missing()));
            }
            Err(error) => return Err(error.into()),
        };
        let updated_at = modified_at_millis(path);
        if let Some(document) = self.cached(path, &bytes, &updated_at) {
            return Ok(document);
        }
        // Preserve the bounded reader's retry/depth contracts on a changed or invalid file.
        let Some((bytes, root)) = read_session_value(path)? else {
            self.entry = None;
            return Ok(std::sync::Arc::new(HookCanvasDocument::missing()));
        };
        Ok(self.build(path, bytes, root, modified_at_millis(path)))
    }

    pub(crate) fn serialized(
        &mut self,
        path: &Path,
        bytes: &[u8],
        root: &Value,
        updated_at: &Option<String>,
    ) -> std::sync::Arc<HookCanvasDocument> {
        self.cached(path, bytes, updated_at)
            .unwrap_or_else(|| self.build(path, bytes.to_vec(), root.clone(), updated_at.clone()))
    }

    fn cached(
        &self,
        path: &Path,
        bytes: &[u8],
        updated_at: &Option<String>,
    ) -> Option<std::sync::Arc<HookCanvasDocument>> {
        let entry = self.entry.as_ref()?;
        if entry.path != path || entry.bytes != bytes || &entry.updated_at != updated_at {
            return None;
        }
        let roots = canonical_preview_roots(path.parent().unwrap_or_else(|| Path::new(".")));
        (roots == entry.roots && candidate_stamps(&entry.candidates, &roots) == entry.stamps)
            .then(|| std::sync::Arc::clone(&entry.document))
    }

    fn build(
        &mut self,
        path: &Path,
        bytes: Vec<u8>,
        root: Value,
        updated_at: Option<String>,
    ) -> std::sync::Arc<HookCanvasDocument> {
        let directory = path.parent().unwrap_or_else(|| Path::new("."));
        let roots = canonical_preview_roots(directory);
        let mut candidates = canvas_nodes(&root, hook_canvas_source(&root))
            .iter()
            .flat_map(|node| node_preview_sources(node))
            .filter(|source| !source.starts_with("data:"))
            .map(|source| {
                let candidate = PathBuf::from(source);
                if candidate.is_absolute() {
                    candidate
                } else {
                    directory.join(candidate)
                }
            })
            .collect::<Vec<_>>();
        candidates.sort();
        candidates.dedup();
        let stamps = candidate_stamps(&candidates, &roots);
        let document = std::sync::Arc::new(HookCanvasDocument::from_serialized_root(
            path,
            bytes.clone(),
            root,
            updated_at.clone(),
        ));
        // A file/root that moved while normalizing must be revisited by the next caller.
        if roots == canonical_preview_roots(directory)
            && stamps == candidate_stamps(&candidates, &roots)
        {
            self.entry = Some(CachedDocument {
                path: path.to_path_buf(),
                bytes,
                updated_at,
                candidates,
                roots,
                stamps,
                document: std::sync::Arc::clone(&document),
            });
        } else {
            self.entry = None;
        }
        document
    }
}

fn candidate_stamps(
    paths: &[PathBuf],
    roots: &[PathBuf],
) -> Vec<Option<(PathBuf, u64, Option<std::time::SystemTime>)>> {
    paths
        .iter()
        .map(|path| {
            let canonical = fs::canonicalize(path).ok()?;
            if !roots.iter().any(|root| canonical.starts_with(root)) {
                return None;
            }
            let metadata = fs::metadata(&canonical).ok()?;
            if !metadata.is_file() {
                return None;
            }
            Some((canonical, metadata.len(), metadata.modified().ok()))
        })
        .collect()
}
