use super::*;

impl WallStore {
    pub(crate) fn open(root: &Path) -> anyhow::Result<Self> {
        fs::create_dir_all(root)?;
        crate::restrict_sensitive_path_permissions(root, true)?;
        let lock_path = root.join("writer.lock");
        let writer_lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        crate::restrict_sensitive_path_permissions(&lock_path, false)?;
        writer_lock
            .try_lock_exclusive()
            .map_err(|error| anyhow::anyhow!("wall store already has a writer: {error}"))?;
        let path = root.join("walls.json");
        let document = match File::open(&path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(MAX_DOCUMENT_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)?;
                anyhow::ensure!(
                    bytes.len() <= MAX_DOCUMENT_BYTES,
                    "wall store exceeds size limit"
                );
                let document: WallDocument = serde_json::from_slice(&bytes)?;
                validate_document(&document)?;
                document
            }
            Err(error) if error.kind() == ErrorKind::NotFound => WallDocument {
                storage_version: 1,
                ..WallDocument::default()
            },
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            path,
            surface_links: Mutex::new(BTreeMap::new()),
            state: Mutex::new(WallState {
                timeline: timing::WallTimeline::new(&document.layouts),
                document,
                leases: BTreeMap::new(),
                write_failed: false,
            }),
            _writer_lock: writer_lock,
        })
    }

    pub(super) fn commit(&self, state: &mut WallState, document: WallDocument) -> WallResult<()> {
        validate_document(&document)?;
        let bytes = serde_json::to_vec(&document).map_err(|_| WallStoreError::unavailable())?;
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(WallStoreError::new(
                413,
                "wall_capacity",
                "wall storage size limit reached",
            ));
        }
        if let Err(error) = crate::write_bytes_atomically(
            &self.path,
            &bytes,
            crate::AtomicWritePermissions::Restrict,
        ) {
            // A post-rename flush/ACL error has an uncertain commit outcome. Do not
            // serve an older in-memory revision over a newer durable document.
            state.write_failed = true;
            eprintln!("wall persistence failed; reopen required: {error:#}");
            return Err(WallStoreError::unavailable());
        }
        state.timeline.reconcile(&document.layouts);
        state.document = document;
        Ok(())
    }
}

fn validate_document(document: &WallDocument) -> WallResult<()> {
    if document.storage_version != 1 || document.revision > WALL_MAX_REVISION {
        return Err(WallStoreError::invalid(
            "unsupported wall storage version or revision",
        ));
    }
    if document.endpoints.len() > MAX_ENDPOINTS || document.layouts.len() > MAX_WALLS {
        return Err(WallStoreError::new(
            413,
            "wall_capacity",
            "wall record limit reached",
        ));
    }
    if document.revision == 0 && (!document.endpoints.is_empty() || !document.layouts.is_empty()) {
        return Err(WallStoreError::invalid(
            "populated wall storage needs a revision",
        ));
    }
    let mut endpoints = BTreeSet::new();
    let mut outputs = BTreeSet::new();
    for endpoint in &document.endpoints {
        validate_tile_endpoint(endpoint).map_err(|error| WallStoreError::invalid(error.0))?;
        if !endpoints.insert(endpoint.endpoint_id.as_str())
            || !outputs.insert((&endpoint.device_id, &endpoint.output_id))
        {
            return Err(WallStoreError::conflict(
                "duplicate endpoint or physical output",
            ));
        }
    }
    let mut walls = BTreeSet::new();
    let mut assigned = BTreeSet::new();
    for layout in &document.layouts {
        validate_wall_layout(layout).map_err(|error| WallStoreError::invalid(error.0))?;
        if layout.revision > document.revision || !walls.insert(&layout.wall_id) {
            return Err(WallStoreError::invalid(
                "inconsistent wall identity or revision",
            ));
        }
        for tile in &layout.tiles {
            if !endpoints.contains(tile.endpoint_id.as_str()) {
                return Err(WallStoreError::invalid(
                    "wall references an unregistered endpoint",
                ));
            }
            if !assigned.insert(&tile.endpoint_id) {
                return Err(WallStoreError::conflict(
                    "endpoint already belongs to another wall",
                ));
            }
        }
    }
    super::presentation::validate_presentations(document)
}

pub(super) fn next_document(state: &WallState, base_revision: u64) -> WallResult<WallDocument> {
    if base_revision != state.document.revision {
        return Err(WallStoreError::conflict(
            "stale catalog revision; read current state",
        ));
    }
    if base_revision >= WALL_MAX_REVISION {
        return Err(WallStoreError::conflict("wall revision exhausted"));
    }
    let mut next = state.document.clone();
    next.revision += 1;
    Ok(next)
}
