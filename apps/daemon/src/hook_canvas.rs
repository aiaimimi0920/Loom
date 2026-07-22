use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

pub(crate) const MIN_NODE_SIZE: f64 = 24.0;
pub(crate) const DEFAULT_NODE_SIZE: f64 = 96.0;
pub(crate) const MAX_PREVIEW_BYTES: u64 = 20 * 1024 * 1024;
const SESSION_READ_ATTEMPTS: usize = 3;
const SESSION_READ_RETRY_DELAY: Duration = Duration::from_millis(20);

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HookCanvasSnapshot {
    pub available: bool,
    pub revision: String,
    pub updated_at: Option<String>,
    pub workflow_id: Option<String>,
    pub bounds: HookCanvasBounds,
    pub nodes: Vec<HookCanvasNode>,
    pub edges: Vec<HookCanvasEdge>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub(crate) struct HookCanvasBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum HookCanvasNodeKind {
    Screenshot,
    Art,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HookCanvasNode {
    pub id: String,
    pub kind: HookCanvasNodeKind,
    pub label: String,
    pub art_id: Option<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub preview_available: bool,
    pub preview_url: Option<String>,
    pub status: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HookCanvasEdge {
    pub id: String,
    pub source_node_id: String,
    pub source_port_id: Option<String>,
    pub target_node_id: String,
    pub target_port_id: Option<String>,
}

#[derive(Debug)]
pub(crate) struct HookCanvasDocument {
    pub snapshot: HookCanvasSnapshot,
    preview_paths: HashMap<String, PathBuf>,
    preview_root: Option<PathBuf>,
}

#[derive(Debug, Error)]
pub(crate) enum HookCanvasError {
    #[error("unable to read Hook session: {0}")]
    Read(#[from] std::io::Error),
    #[error("invalid Hook session JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl HookCanvasDocument {
    pub(crate) fn read(session_path: &Path) -> Result<Self, HookCanvasError> {
        let Some((bytes, root)) = read_session_value(session_path)? else {
            return Ok(Self::missing());
        };
        let session_dir = session_path.parent().unwrap_or_else(|| Path::new("."));
        let preview_root = canonical_image_root(session_dir);
        let mut warnings = Vec::new();
        let mut preview_paths = HashMap::new();
        let mut node_ids = HashSet::new();
        let mut nodes = Vec::new();

        for raw_node in canvas_nodes(&root) {
            let Some(id) = non_empty_string(raw_node.get("id")) else {
                warnings.push("已跳过缺少有效 ID 的 Hook 节点。".to_owned());
                continue;
            };
            if !node_ids.insert(id.clone()) {
                warnings.push(format!("已跳过重复的 Hook 节点 `{id}`。"));
                continue;
            }

            let (x, x_degraded) = normalized_coordinate(node_coordinate(raw_node, "x"));
            let (y, y_degraded) = normalized_coordinate(node_coordinate(raw_node, "y"));
            let (width, width_degraded) = normalized_size(node_size(raw_node, "w", "width"));
            let (height, height_degraded) = normalized_size(node_size(raw_node, "h", "height"));
            if x_degraded || y_degraded || width_degraded || height_degraded {
                warnings.push(format!("Hook 节点 `{id}` 的几何信息已归一化。"));
            }

            let art_id = node_string(raw_node, "artId");
            let node_type = node_type(raw_node);
            let kind = classify_node(node_type.as_deref(), art_id.as_deref());
            let label = match kind {
                HookCanvasNodeKind::Screenshot => "截图节点",
                HookCanvasNodeKind::Art => "Art 节点",
                HookCanvasNodeKind::Unknown => "未知节点",
            }
            .to_owned();
            let status = normalized_status(node_string(raw_node, "status").as_deref(), &kind);

            let preview_path = node_preview_source(raw_node).and_then(|source| {
                resolve_preview_path(session_dir, preview_root.as_deref(), &source).or_else(|| {
                    warnings.push(format!("Hook 节点 `{id}` 的预览不可用。"));
                    None
                })
            });
            let preview_available = preview_path.is_some();
            if let Some(path) = preview_path {
                preview_paths.insert(id.clone(), path);
            }
            let preview_url = preview_available.then(|| {
                format!(
                    "/v1/hook-bridge/canvas/nodes/{}/preview",
                    encode_path_segment(&id)
                )
            });

            nodes.push(HookCanvasNode {
                id,
                kind,
                label,
                art_id,
                x,
                y,
                width,
                height,
                preview_available,
                preview_url,
                status,
            });
        }
        nodes.sort_by(|left, right| left.id.cmp(&right.id));

        let mut edges = Vec::new();
        for (index, raw_edge) in canvas_edges(&root).iter().enumerate() {
            let source_node_id =
                first_non_empty_string(raw_edge, &["fromUnitId", "sourceNodeId", "source"]);
            let target_node_id =
                first_non_empty_string(raw_edge, &["toUnitId", "targetNodeId", "target"]);
            let (Some(source_node_id), Some(target_node_id)) = (source_node_id, target_node_id)
            else {
                warnings.push("已跳过缺少端点的 Hook 连线。".to_owned());
                continue;
            };
            if !node_ids.contains(&source_node_id) || !node_ids.contains(&target_node_id) {
                warnings.push(format!(
                    "已跳过端点不存在的 Hook 连线 `{source_node_id}` -> `{target_node_id}`。"
                ));
                continue;
            }
            let id =
                non_empty_string(raw_edge.get("id")).unwrap_or_else(|| format!("edge-{index:04}"));
            edges.push(HookCanvasEdge {
                id,
                source_node_id,
                source_port_id: first_non_empty_string(
                    raw_edge,
                    &["fromPortId", "sourcePortId", "sourceHandle"],
                ),
                target_node_id,
                target_port_id: first_non_empty_string(
                    raw_edge,
                    &["toPortId", "targetPortId", "targetHandle"],
                ),
            });
        }
        edges.sort_by(|left, right| left.id.cmp(&right.id));

        let snapshot = HookCanvasSnapshot {
            available: true,
            revision: revision_for(&bytes),
            updated_at: modified_at_millis(session_path),
            workflow_id: first_non_empty_string(&root, &["workflowId", "workflow_id"]),
            bounds: canvas_bounds(&nodes),
            nodes,
            edges,
            warnings,
        };
        Ok(Self {
            snapshot,
            preview_paths,
            preview_root,
        })
    }

    pub(crate) fn preview_path(&self, node_id: &str) -> Option<&Path> {
        self.preview_paths.get(node_id).map(PathBuf::as_path)
    }

    pub(crate) fn preview_root(&self) -> Option<&Path> {
        self.preview_root.as_deref()
    }

    fn missing() -> Self {
        Self {
            snapshot: HookCanvasSnapshot {
                available: false,
                revision: "missing".to_owned(),
                updated_at: None,
                workflow_id: None,
                bounds: HookCanvasBounds::default(),
                nodes: Vec::new(),
                edges: Vec::new(),
                warnings: Vec::new(),
            },
            preview_paths: HashMap::new(),
            preview_root: None,
        }
    }
}

fn read_session_value(session_path: &Path) -> Result<Option<(Vec<u8>, Value)>, HookCanvasError> {
    read_session_value_with(
        || fs::read(session_path),
        || thread::sleep(SESSION_READ_RETRY_DELAY),
    )
}

fn read_session_value_with<Read, Wait>(
    mut read: Read,
    mut wait: Wait,
) -> Result<Option<(Vec<u8>, Value)>, HookCanvasError>
where
    Read: FnMut() -> std::io::Result<Vec<u8>>,
    Wait: FnMut(),
{
    let mut last_json_error = None;

    for attempt in 0..SESSION_READ_ATTEMPTS {
        let bytes = match read() {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound && attempt == 0 => return Ok(None),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if attempt + 1 < SESSION_READ_ATTEMPTS {
                    wait();
                }
                continue;
            }
            Err(error) => return Err(HookCanvasError::Read(error)),
        };

        match serde_json::from_slice(&bytes) {
            Ok(root) => return Ok(Some((bytes, root))),
            Err(error) => {
                last_json_error = Some(error);
                if attempt + 1 < SESSION_READ_ATTEMPTS {
                    wait();
                }
            }
        }
    }

    match last_json_error {
        Some(error) => Err(HookCanvasError::Json(error)),
        None => Ok(None),
    }
}

fn canvas_nodes(root: &Value) -> &[Value] {
    first_non_empty_array(root, &["stickers", "nodes", "units"])
}

fn canvas_edges(root: &Value) -> &[Value] {
    first_non_empty_array(root, &["links", "edges"])
}

fn first_non_empty_array<'a>(root: &'a Value, keys: &[&str]) -> &'a [Value] {
    keys.iter()
        .filter_map(|key| root.get(key).and_then(Value::as_array))
        .find(|values| !values.is_empty())
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn node_data(node: &Value) -> Option<&Value> {
    node.get("data").filter(|value| value.is_object())
}

fn node_value<'a>(node: &'a Value, key: &str) -> Option<&'a Value> {
    node.get(key).or_else(|| node_data(node)?.get(key))
}

fn node_nested_value<'a>(node: &'a Value, container: &str, key: &str) -> Option<&'a Value> {
    node.get(container)
        .and_then(|value| value.get(key))
        .or_else(|| node_data(node)?.get(container)?.get(key))
}

fn node_coordinate(node: &Value, key: &str) -> Option<f64> {
    value_as_f64(node.get(key))
        .or_else(|| value_as_f64(node_nested_value(node, "position", key)))
        .or_else(|| value_as_f64(node_data(node).and_then(|data| data.get(key))))
}

fn node_size(node: &Value, short_key: &str, long_key: &str) -> Option<f64> {
    value_as_f64(node.get(short_key))
        .or_else(|| value_as_f64(node.get(long_key)))
        .or_else(|| value_as_f64(node_nested_value(node, "measured", long_key)))
        .or_else(|| value_as_f64(node_data(node).and_then(|data| data.get(short_key))))
        .or_else(|| value_as_f64(node_data(node).and_then(|data| data.get(long_key))))
}

fn node_string(node: &Value, key: &str) -> Option<String> {
    non_empty_string(node_value(node, key))
}

fn node_type(node: &Value) -> Option<String> {
    let outer = non_empty_string(node.get("type"));
    let inner = node_data(node).and_then(|data| non_empty_string(data.get("type")));
    match outer.as_deref() {
        None | Some("node" | "unit") => inner.or(outer),
        Some(_) => outer,
    }
}

fn node_preview_source(node: &Value) -> Option<String> {
    non_empty_string(node.get("previewSrc"))
        .or_else(|| node_data(node).and_then(|data| non_empty_string(data.get("previewSrc"))))
        .or_else(|| non_empty_string(node.get("src")))
        .or_else(|| node_data(node).and_then(|data| non_empty_string(data.get("src"))))
}

fn non_empty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn first_non_empty_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| non_empty_string(value.get(key)))
}

fn value_as_f64(value: Option<&Value>) -> Option<f64> {
    value.and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_str()?.trim().parse::<f64>().ok())
            .filter(|number| number.is_finite())
    })
}

fn normalized_coordinate(value: Option<f64>) -> (f64, bool) {
    match value {
        Some(value) if value.is_finite() => (value, false),
        _ => (0.0, true),
    }
}

fn normalized_size(value: Option<f64>) -> (f64, bool) {
    match value {
        Some(value) if value.is_finite() && value > 0.0 => (value.max(MIN_NODE_SIZE), false),
        _ => (DEFAULT_NODE_SIZE, true),
    }
}

fn classify_node(node_type: Option<&str>, art_id: Option<&str>) -> HookCanvasNodeKind {
    if art_id.is_some() || node_type.is_some_and(|value| value.eq_ignore_ascii_case("art")) {
        HookCanvasNodeKind::Art
    } else if node_type.is_some_and(|value| {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "sticker" | "screenshot" | "capture"
        )
    }) {
        HookCanvasNodeKind::Screenshot
    } else {
        HookCanvasNodeKind::Unknown
    }
}

fn normalized_status(value: Option<&str>, kind: &HookCanvasNodeKind) -> &'static str {
    match value.map(|value| value.to_ascii_lowercase()).as_deref() {
        Some("ready") => "ready",
        Some("processing") => "processing",
        Some("error") => "error",
        Some("unknown") => "unknown",
        _ if matches!(kind, HookCanvasNodeKind::Unknown) => "unknown",
        _ => "ready",
    }
}

fn canonical_image_root(session_dir: &Path) -> Option<PathBuf> {
    let root = session_dir.join("images");
    fs::canonicalize(root).ok().filter(|path| path.is_dir())
}

fn resolve_preview_path(
    session_dir: &Path,
    preview_root: Option<&Path>,
    source: &str,
) -> Option<PathBuf> {
    let preview_root = preview_root?;
    let source_path = Path::new(source);
    let candidate = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        session_dir.join(source_path)
    };
    let candidate = fs::canonicalize(candidate).ok()?;
    (candidate.is_file() && candidate.starts_with(preview_root)).then_some(candidate)
}

fn revision_for(bytes: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn modified_at_millis(path: &Path) -> Option<String> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis().to_string())
}

fn canvas_bounds(nodes: &[HookCanvasNode]) -> HookCanvasBounds {
    let Some(first) = nodes.first() else {
        return HookCanvasBounds::default();
    };
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x + first.width;
    let mut max_y = first.y + first.height;
    for node in &nodes[1..] {
        min_x = min_x.min(node.x);
        min_y = min_y.min(node.y);
        max_x = max_x.max(node.x + node.width);
        max_y = max_y.max(node.y + node.height);
    }
    HookCanvasBounds {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn test_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("loom-hook-canvas-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create Hook fixture root");
        root
    }

    fn write_session(root: &Path, json: &str) -> PathBuf {
        let session_dir = root.join("com.vmjcv.arthook-next");
        fs::create_dir_all(session_dir.join("images")).expect("create Hook fixture dirs");
        let path = session_dir.join("session.json");
        fs::write(&path, json).expect("write Hook session fixture");
        path
    }

    #[test]
    fn normalizes_realistic_hook_session_into_canvas_snapshot() {
        let root = test_root("realistic");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"capture","type":"sticker","src":"images/capture.png","x":1816.0,"y":201.0,"w":500.0,"h":750.0},
                {"id":"small","type":"sticker","src":"images/missing.png","x":1792.0,"y":346.0,"w":60.0,"h":60.0},
                {"id":"art","type":"art","artId":"custom-image","src":"images/art.png","x":1576.0,"y":499.0,"w":60.0,"h":60.0}
              ],
              "links": [
                {"id":"edge-1","fromUnitId":"capture","fromPortId":"output_image","toUnitId":"art","toPortId":"input_image"}
              ]
            }"#,
        );
        let images = session.parent().expect("session parent").join("images");
        fs::write(images.join("capture.png"), b"capture").expect("write capture preview");
        fs::write(images.join("art.png"), b"art").expect("write art preview");

        let document = HookCanvasDocument::read(&session).expect("normalize Hook canvas");

        assert!(document.snapshot.available);
        assert_eq!(document.snapshot.nodes.len(), 3);
        assert_eq!(document.snapshot.edges.len(), 1);
        assert_eq!(document.snapshot.bounds.x, 1576.0);
        assert_eq!(document.snapshot.bounds.y, 201.0);
        assert_eq!(document.snapshot.bounds.width, 740.0);
        assert_eq!(document.snapshot.bounds.height, 750.0);
        let art = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "art")
            .expect("art node");
        assert_eq!(art.kind, HookCanvasNodeKind::Art);
        assert_eq!(art.art_id.as_deref(), Some("custom-image"));
        assert!(art.preview_available);
        assert_eq!(
            art.preview_url.as_deref(),
            Some("/v1/hook-bridge/canvas/nodes/art/preview")
        );
        let small = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "small")
            .expect("small node");
        assert!(!small.preview_available);
        assert!(small.preview_url.is_none());
        assert_eq!(document.snapshot.edges[0].source_node_id, "capture");
        assert_eq!(document.snapshot.edges[0].target_node_id, "art");
    }

    #[test]
    fn invalid_geometry_and_dangling_edges_degrade_locally() {
        let root = test_root("invalid");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"capture","type":"sticker","x":"bad","y":-20,"w":0,"h":-1},
                {"type":"art","x":5,"y":5,"w":40,"h":40}
              ],
              "links": [
                {"id":"dangling","fromUnitId":"missing","toUnitId":"capture"}
              ]
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("degraded Hook canvas");

        assert_eq!(document.snapshot.nodes.len(), 1);
        assert_eq!(document.snapshot.nodes[0].x, 0.0);
        assert_eq!(document.snapshot.nodes[0].y, -20.0);
        assert_eq!(document.snapshot.nodes[0].width, DEFAULT_NODE_SIZE);
        assert_eq!(document.snapshot.nodes[0].height, DEFAULT_NODE_SIZE);
        assert!(document.snapshot.edges.is_empty());
        assert!(!document.snapshot.warnings.is_empty());
    }

    #[test]
    fn missing_session_returns_a_valid_empty_snapshot() {
        let root = test_root("missing");
        let document = HookCanvasDocument::read(&root.join("session.json"))
            .expect("missing session is a valid empty state");

        assert!(!document.snapshot.available);
        assert!(document.snapshot.nodes.is_empty());
        assert!(document.snapshot.edges.is_empty());
        assert_eq!(document.snapshot.revision, "missing");
        assert!(document.preview_root().is_none());
    }

    #[test]
    fn revision_changes_when_session_content_changes() {
        let root = test_root("revision");
        let session = write_session(&root, r#"{"stickers":[],"links":[]}"#);
        let first = HookCanvasDocument::read(&session).expect("first snapshot");
        fs::write(
            &session,
            r#"{"stickers":[{"id":"one","type":"sticker"}],"links":[]}"#,
        )
        .expect("rewrite session");
        let second = HookCanvasDocument::read(&session).expect("second snapshot");

        assert_eq!(first.snapshot.revision.len(), 16);
        assert_ne!(first.snapshot.revision, second.snapshot.revision);
    }

    #[test]
    fn negative_coordinates_are_preserved_in_bounds() {
        let root = test_root("negative-bounds");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"left","type":"sticker","x":-120,"y":-40,"w":20,"h":30},
                {"id":"right","type":"sticker","x":80,"y":60,"w":40,"h":50}
              ],
              "links": []
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("negative bounds");

        assert_eq!(document.snapshot.bounds.x, -120.0);
        assert_eq!(document.snapshot.bounds.y, -40.0);
        assert_eq!(document.snapshot.bounds.width, 240.0);
        assert_eq!(document.snapshot.bounds.height, 150.0);
        assert_eq!(document.snapshot.nodes[0].width, MIN_NODE_SIZE);
    }

    #[test]
    fn classifies_art_screenshot_and_unknown_nodes() {
        let root = test_root("classification");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"art-by-id","artId":"resize"},
                {"id":"capture","type":"capture"},
                {"id":"unknown","type":"custom"}
              ],
              "links": []
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("classify nodes");
        let kinds = document
            .snapshot
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), &node.kind, node.label.as_str()))
            .collect::<Vec<_>>();

        assert_eq!(
            kinds,
            vec![
                ("art-by-id", &HookCanvasNodeKind::Art, "Art 节点"),
                ("capture", &HookCanvasNodeKind::Screenshot, "截图节点"),
                ("unknown", &HookCanvasNodeKind::Unknown, "未知节点"),
            ]
        );
    }

    #[test]
    fn preview_paths_outside_the_session_image_root_are_not_registered() {
        let root = test_root("preview-boundary");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"escape","type":"sticker","src":"../outside.png"}
              ],
              "links": []
            }"#,
        );
        fs::write(root.join("outside.png"), b"outside").expect("write outside image");

        let document = HookCanvasDocument::read(&session).expect("normalize outside preview");
        let node = &document.snapshot.nodes[0];

        assert!(!node.preview_available);
        assert!(node.preview_url.is_none());
        assert!(document.preview_path("escape").is_none());
    }

    #[test]
    fn accepts_nested_hook_bridge_broadcast_shapes() {
        let root = test_root("nested");
        let session = write_session(
            &root,
            r#"{
              "workflowId": "hook-live",
              "nodes": [
                {
                  "id":"nested",
                  "type":"node",
                  "position":{"x":12,"y":24},
                  "measured":{"width":320,"height":180},
                  "data":{"type":"art","artId":"ocr","previewSrc":"images/nested.png","status":"processing"}
                }
              ],
              "edges": [
                {"id":"self","source":"nested","target":"nested","sourceHandle":"out","targetHandle":"in"}
              ]
            }"#,
        );
        fs::write(
            session
                .parent()
                .expect("session parent")
                .join("images")
                .join("nested.png"),
            b"nested",
        )
        .expect("write nested preview");

        let document = HookCanvasDocument::read(&session).expect("normalize nested broadcast");
        let node = &document.snapshot.nodes[0];

        assert_eq!(document.snapshot.workflow_id.as_deref(), Some("hook-live"));
        assert_eq!(node.x, 12.0);
        assert_eq!(node.y, 24.0);
        assert_eq!(node.width, 320.0);
        assert_eq!(node.height, 180.0);
        assert_eq!(node.kind, HookCanvasNodeKind::Art);
        assert_eq!(node.status, "processing");
        assert_eq!(document.snapshot.edges.len(), 1);
        assert_eq!(
            document.snapshot.edges[0].source_port_id.as_deref(),
            Some("out")
        );
        assert_eq!(
            document.snapshot.edges[0].target_port_id.as_deref(),
            Some("in")
        );
    }

    #[test]
    fn empty_primary_arrays_do_not_hide_non_empty_compatibility_arrays() {
        let root = test_root("compat-array-fallback");
        let session = write_session(
            &root,
            r#"{
              "stickers": [],
              "nodes": [
                {"id":"source","type":"sticker","x":10,"y":20},
                {"id":"target","type":"art","x":120,"y":20}
              ],
              "links": [],
              "edges": [
                {"id":"source-target","source":"source","target":"target"}
              ]
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("normalize compatibility arrays");

        assert_eq!(document.snapshot.nodes.len(), 2);
        assert_eq!(document.snapshot.edges.len(), 1);
        assert_eq!(document.snapshot.edges[0].source_node_id, "source");
        assert_eq!(document.snapshot.edges[0].target_node_id, "target");
    }

    #[test]
    fn retries_a_transient_partial_session_write() {
        let mut reads = VecDeque::from([
            b"{\"stickers\":[".to_vec(),
            br#"{"stickers":[{"id":"ready","type":"sticker"}],"links":[]}"#.to_vec(),
        ]);
        let mut waits = 0;

        let (_, root) = read_session_value_with(
            || Ok(reads.pop_front().expect("session read fixture")),
            || waits += 1,
        )
        .expect("retry partial Hook session")
        .expect("session remains available");

        assert_eq!(waits, 1);
        assert_eq!(canvas_nodes(&root).len(), 1);
        assert_eq!(canvas_nodes(&root)[0]["id"], "ready");
    }

    #[test]
    fn malformed_session_is_reported_as_json_error() {
        let root = test_root("malformed");
        let session = write_session(&root, "{not-json");

        let error = HookCanvasDocument::read(&session).expect_err("malformed session must fail");

        assert!(matches!(error, HookCanvasError::Json(_)));
    }
}
