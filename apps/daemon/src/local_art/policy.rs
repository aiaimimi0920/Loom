//! Explicit admin approval for one device and immutable package; deny absent/corrupt policy, never infer approval from install.
use super::*;
pub(super) static LOCK: Mutex<()> = Mutex::new(());
const MAX_BYTES: u64 = 256 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Approval {
    pub device_id: String,
    pub art_id: String,
    pub art_version: String,
    pub package_digest: String,
    pub permissions: Vec<Permission>,
    pub services: Vec<String>,
    pub valid_until_ms: u64,
    pub offline_ttl_ms: u64,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Document {
    revision: u64,
    approvals: Vec<Approval>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Put {
    expected_revision: u64,
    approvals: Vec<Approval>,
}
fn path(root: &Path) -> PathBuf {
    root.join("settings").join("local-art-approvals.json")
}
fn validate(rows: &[Approval]) -> LocalResult<()> {
    if rows.len() > 128 {
        return Err(fail(413, "local_art_approval_limit"));
    }
    let mut seen = BTreeSet::new();
    for row in rows {
        if !seen.insert((
            &row.device_id,
            &row.art_id,
            &row.art_version,
            &row.package_digest,
        )) || row.valid_until_ms < 2
            || row.valid_until_ms > 9_007_199_254_740_991
            || row.offline_ttl_ms > loom_protocol::local_art::MAX_VALIDITY_MS
        {
            return Err(fail(400, "local_art_invalid_approval"));
        }
        let grant = Grant {
            protocol_version: PROTOCOL.into(),
            grant_id: "validation".into(),
            binding: Binding {
                origin: "https://localhost".into(),
                request_id: "validation".into(),
                device_id: row.device_id.clone(),
                workspace_id: "validation".into(),
                instance_id: "validation".into(),
                generation: 1,
                art_id: row.art_id.clone(),
                art_version: row.art_version.clone(),
                package_digest: row.package_digest.clone(),
                sdk_version: SDK.into(),
            },
            execution_owner: "otter".into(),
            result_owner: "otter_workspace".into(),
            permissions: row.permissions.clone(),
            services: row.services.clone(),
            issued_at_ms: 1,
            expires_at_ms: 2,
            offline_until_ms: 0,
            source: Source {
                resource_id: format!("sha256:{}", "0".repeat(64)),
                digest: "0".repeat(64),
                size: 1,
            },
        };
        grant
            .validate()
            .map_err(|_| fail(400, "local_art_invalid_approval"))?;
    }
    Ok(())
}
fn load(root: &Path) -> LocalResult<Document> {
    match fs::symlink_metadata(path(root)) {
        Ok(metadata)
            if !metadata.is_file() || loom_security::metadata_has_link_semantics(&metadata) =>
        {
            return Err(fail(503, "local_art_policy_invalid"))
        }
        Err(error) if error.kind() != ErrorKind::NotFound => {
            return Err(fail(503, "local_art_policy_unavailable"))
        }
        _ => {}
    }
    let file = match fs::File::open(path(root)) {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Document::default()),
        Err(_) => return Err(fail(503, "local_art_policy_unavailable")),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| fail(503, "local_art_policy_unavailable"))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(fail(503, "local_art_policy_invalid"));
    }
    let document: Document =
        serde_json::from_slice(&bytes).map_err(|_| fail(503, "local_art_policy_invalid"))?;
    validate(&document.approvals).map_err(|_| fail(503, "local_art_policy_invalid"))?;
    Ok(document)
}
// Caller must hold LOCK; auto-triggers conservatively drop privileges across the whole policy revision fence without adding a persistent grant table.
pub(super) fn revision_unlocked(root: &Path) -> LocalResult<u64> {
    Ok(load(root)?.revision)
}
pub(super) fn manage(root: &Path, request: &ParsedHttpRequest) -> LocalResult<Value> {
    let _lock = LOCK
        .lock()
        .map_err(|_| fail(503, "local_art_policy_unavailable"))?;
    let mut document = load(root)?;
    if request.method == "PUT" {
        let input: Put = parse(&request.body)?;
        if input.expected_revision != document.revision {
            return Err(fail(409, "local_art_policy_conflict"));
        }
        validate(&input.approvals)?;
        document.revision = document
            .revision
            .checked_add(1)
            .ok_or_else(|| fail(409, "local_art_policy_conflict"))?;
        document.approvals = input.approvals;
        write_json_atomically(&path(root), &document)
            .map_err(|_| fail(503, "local_art_policy_unavailable"))?;
    } else if request.method != "GET" {
        return Err(fail(405, "local_art_method_not_allowed"));
    }
    serde_json::to_value(document).map_err(|_| fail(503, "local_art_policy_invalid"))
}
pub(super) fn approved(root: &Path, grant: &Grant, time: u64) -> LocalResult<Approval> {
    let _lock = LOCK
        .lock()
        .map_err(|_| fail(503, "local_art_policy_unavailable"))?;
    approved_unlocked(root, grant, time)
}
/// Exact active package lookup for description only; prepare still rechecks the complete request scope.
pub(super) fn describe_approval(
    root: &Path,
    device: &str,
    art: &str,
    version: &str,
    digest: &str,
    time: u64,
) -> LocalResult<Approval> {
    let _lock = LOCK
        .lock()
        .map_err(|_| fail(503, "local_art_policy_unavailable"))?;
    load(root)?
        .approvals
        .into_iter()
        .find(|a| {
            a.device_id == device
                && a.art_id == art
                && a.art_version == version
                && a.package_digest == digest
                && a.valid_until_ms > time
        })
        .ok_or_else(|| fail(403, "local_art_not_approved"))
}
pub(super) fn approved_unlocked(root: &Path, g: &Grant, time: u64) -> LocalResult<Approval> {
    load(root)?
        .approvals
        .into_iter()
        .find(|a| {
            a.device_id == g.binding.device_id
                && a.art_id == g.binding.art_id
                && a.art_version == g.binding.art_version
                && a.package_digest == g.binding.package_digest
                && a.valid_until_ms > time
                && g.permissions.iter().all(|p| a.permissions.contains(p))
                && g.services.iter().all(|s| a.services.contains(s))
        })
        .ok_or_else(|| fail(403, "local_art_not_approved"))
}
