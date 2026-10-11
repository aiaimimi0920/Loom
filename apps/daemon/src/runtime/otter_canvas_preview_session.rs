// Otter canvas preview routing and session lock and revision state.
fn otter_canvas_preview_node_id(method: &str, path: &str) -> Option<String> {
    if method != "GET" {
        return None;
    }
    let path = path.split('?').next().unwrap_or(path);
    let encoded_id = path_id_with_suffix(path, "/v1/hook-bridge/canvas/nodes/", "/preview")?;
    Some(percent_decode(encoded_id))
}

// Parse `/v1/hook-bridge/canvas/workflows/{workflowId}/nodes/{nodeId}/preview`
// into (workflowId, nodeId), both percent-decoded. Returns None for other paths.
fn canvas_workflow_preview_ids(method: &str, path: &str) -> Option<(String, String)> {
    if method != "GET" {
        return None;
    }
    let path = path.split('?').next().unwrap_or(path);
    let rest = path.strip_prefix("/v1/hook-bridge/canvas/workflows/")?;
    let rest = rest.strip_suffix("/preview")?;
    let (encoded_workflow, encoded_node) = rest.split_once("/nodes/")?;
    if encoded_workflow.is_empty() || encoded_node.is_empty() {
        return None;
    }
    Some((
        percent_decode(encoded_workflow),
        percent_decode(encoded_node),
    ))
}

fn otter_canvas_preview_response(node_id: &str) -> Result<RouteResponse> {
    let document = match load_active_otter_canvas_document() {
        Ok(document) => document,
        Err(error) => {
            eprintln!("loom Otter canvas preview snapshot failed: {error:#}");
            return structured_error(
                500,
                json!({
                    "code": "hook_canvas_error",
                    "message": "Otter canvas preview is temporarily unavailable",
                }),
            )
            .map(|(status, body)| RouteResponse::Text { status, body });
        }
    };
    let Some(source) = document.preview_source(node_id) else {
        return structured_error(
            404,
            json!({
                "code": "preview_not_found",
                "message": "Otter canvas preview was not found",
            }),
        )
        .map(|(status, body)| RouteResponse::Text { status, body });
    };
    match source {
        otter_canvas::OtterCanvasPreviewSource::DataUrl(data_url) => {
            let body = match loom_image_io::decode_data_url_bytes(data_url) {
                Ok(body) => body,
                Err(_) => {
                    return structured_error(
                        415,
                        json!({
                            "code": "unsupported_preview_type",
                            "message": "Otter canvas preview is not a supported image",
                        }),
                    )
                    .map(|(status, body)| RouteResponse::Text { status, body });
                }
            };
            return otter_canvas_preview_binary_response(body);
        }
        otter_canvas::OtterCanvasPreviewSource::File(path) => {
            let preview_roots = document.preview_roots();
            if preview_roots.is_empty() {
                return structured_error(
                    404,
                    json!({
                        "code": "preview_not_found",
                        "message": "Otter canvas preview was not found",
                    }),
                )
                .map(|(status, body)| RouteResponse::Text { status, body });
            }
            let Ok(canonical_path) = fs::canonicalize(path) else {
                return structured_error(
                    404,
                    json!({
                        "code": "preview_not_found",
                        "message": "Otter canvas preview was not found",
                    }),
                )
                .map(|(status, body)| RouteResponse::Text { status, body });
            };
            // The preview file must be a regular file inside one of the roots the
            // document already validated its node images against.
            let within_root = preview_roots.iter().any(|root| {
                fs::canonicalize(root)
                    .map(|canonical_root| canonical_path.starts_with(&canonical_root))
                    .unwrap_or(false)
            });
            if !canonical_path.is_file() || !within_root {
                return structured_error(
                    404,
                    json!({
                        "code": "preview_not_found",
                        "message": "Otter canvas preview was not found",
                    }),
                )
                .map(|(status, body)| RouteResponse::Text { status, body });
            }
            let metadata = match fs::metadata(&canonical_path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    return structured_error(
                        404,
                        json!({
                            "code": "preview_not_found",
                            "message": "Otter canvas preview was not found",
                        }),
                    )
                    .map(|(status, body)| RouteResponse::Text { status, body });
                }
                Err(error) => return Err(error).context("read Otter canvas preview metadata"),
            };
            if metadata.len() > otter_canvas::MAX_PREVIEW_BYTES {
                return structured_error(
                    413,
                    json!({
                        "code": "preview_too_large",
                        "message": "Otter canvas preview exceeds the size limit",
                    }),
                )
                .map(|(status, body)| RouteResponse::Text { status, body });
            }
            let body = match fs::read(&canonical_path) {
                Ok(body) => body,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    return structured_error(
                        404,
                        json!({
                            "code": "preview_not_found",
                            "message": "Otter canvas preview was not found",
                        }),
                    )
                    .map(|(status, body)| RouteResponse::Text { status, body });
                }
                Err(error) => return Err(error).context("read Otter canvas preview bytes"),
            };
            otter_canvas_preview_binary_response(body)
        }
    }
}

fn otter_canvas_preview_binary_response(body: Vec<u8>) -> Result<RouteResponse> {
    if body.len() as u64 > otter_canvas::MAX_PREVIEW_BYTES {
        return structured_error(
            413,
            json!({
                "code": "preview_too_large",
                "message": "Otter canvas preview exceeds the size limit",
            }),
        )
        .map(|(status, body)| RouteResponse::Text { status, body });
    }
    let Some(content_type) = otter_canvas_preview_content_type(&body) else {
        return structured_error(
            415,
            json!({
                "code": "unsupported_preview_type",
                "message": "Otter canvas preview is not a supported image",
            }),
        )
        .map(|(status, body)| RouteResponse::Text { status, body });
    };
    Ok(RouteResponse::Binary {
        status: 200,
        content_type: content_type.to_owned(),
        body,
    })
}

fn otter_canvas_preview_content_type(body: &[u8]) -> Option<&'static str> {
    if body.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some("image/png");
    }
    if body.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some("image/jpeg");
    }
    if body.len() >= 12 && &body[..4] == b"RIFF" && &body[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    None
}

fn read_otter_session_snapshot() -> (PathBuf, bool, Value, Option<String>) {
    let session_path = otter_session_path();
    if !session_path.exists() {
        return (
            session_path,
            false,
            json!({ "stickers": [], "links": [] }),
            Some("Session file not found".to_owned()),
        );
    }
    match fs::read_to_string(&session_path) {
        Ok(content) => match serde_json::from_str::<Value>(&content) {
            Ok(session) => match otter_session_document_revision(&session) {
                Ok(_) => (session_path, true, session, None),
                Err(error) => (
                    session_path,
                    false,
                    json!({ "stickers": [], "links": [] }),
                    Some(format!("Unsupported Otter session document: {error}")),
                ),
            },
            Err(error) => (
                session_path,
                false,
                json!({ "stickers": [], "links": [] }),
                Some(format!("Invalid Otter session JSON: {error}")),
            ),
        },
        Err(error) => (
            session_path,
            false,
            json!({ "stickers": [], "links": [] }),
            Some(format!("Unable to read Otter session: {error}")),
        ),
    }
}

const HOOK_LIVE_WORKFLOW_ID: &str = "hook-live";
const HOOK_SESSION_DOCUMENT_SCHEMA_VERSION: u64 = 1;
const HOOK_SESSION_FILE_LOCK_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_OTTER_ART_TERMINAL_REQUESTS: usize = 256;

#[derive(Clone, Debug)]
struct OtterLiveWorkflowSnapshot {
    source_path: PathBuf,
    bytes: Vec<u8>,
    root: Value,
    document_revision: u64,
    updated_at: Option<String>,
}

struct OtterSessionFileLease {
    file: fs::File,
}

impl OtterSessionFileLease {
    fn acquire(session_path: &Path) -> Result<Self> {
        let parent = session_path.parent().ok_or_else(|| {
            anyhow::anyhow!(
                "Otter session path `{}` has no parent",
                session_path.display()
            )
        })?;
        fs::create_dir_all(parent)
            .with_context(|| format!("create Otter session directory `{}`", parent.display()))?;
        let file_name = session_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("Otter session path has no UTF-8 file name"))?;
        let lock_path = parent.join(format!("{file_name}.lock"));
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&lock_path)
            .with_context(|| format!("open Otter session lock `{}`", lock_path.display()))?;
        let started_at = Instant::now();
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(Self { file }),
                Err(error)
                    if otter_session_lock_is_busy(&error)
                        && started_at.elapsed() < HOOK_SESSION_FILE_LOCK_TIMEOUT =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) if otter_session_lock_is_busy(&error) => {
                    anyhow::bail!(
                        "HOOK_SESSION_LOCK_TIMEOUT failed to acquire `{}` within {} ms",
                        lock_path.display(),
                        HOOK_SESSION_FILE_LOCK_TIMEOUT.as_millis()
                    );
                }
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("lock Otter session `{}`", lock_path.display()));
                }
            }
        }
    }
}

impl Drop for OtterSessionFileLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

fn otter_session_lock_is_busy(error: &std::io::Error) -> bool {
    error.kind() == ErrorKind::WouldBlock || matches!(error.raw_os_error(), Some(32 | 33))
}

fn otter_session_document_revision(root: &Value) -> std::result::Result<u64, String> {
    let schema = root.get("documentSchemaVersion");
    let revision = root.get("documentRevision");
    if schema.is_none() && revision.is_none() {
        return Ok(0);
    }
    let schema = schema
        .and_then(Value::as_u64)
        .ok_or_else(|| "documentSchemaVersion must be an unsigned integer".to_owned())?;
    if schema != HOOK_SESSION_DOCUMENT_SCHEMA_VERSION {
        return Err(format!(
            "unsupported Otter documentSchemaVersion {schema}; expected {HOOK_SESSION_DOCUMENT_SCHEMA_VERSION}"
        ));
    }
    revision
        .and_then(Value::as_u64)
        .ok_or_else(|| "documentRevision must be an unsigned integer".to_owned())
}

fn set_otter_session_document_revision(root: &mut Value, revision: u64) -> Result<()> {
    let object = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("Otter session root must be a JSON object"))?;
    object.insert(
        "documentSchemaVersion".to_owned(),
        Value::from(HOOK_SESSION_DOCUMENT_SCHEMA_VERSION),
    );
    object.insert("documentRevision".to_owned(), Value::from(revision));
    Ok(())
}
