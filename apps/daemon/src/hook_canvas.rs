use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub(crate) const MIN_NODE_SIZE: f64 = 24.0;
pub(crate) const DEFAULT_NODE_SIZE: f64 = 96.0;
pub(crate) const MAX_PREVIEW_BYTES: u64 = 20 * 1024 * 1024;
const DEFAULT_EDGE_PORT_GAP: f64 = 6.0;
const MINIFIED_EDGE_PORT_GAP: f64 = 4.0;
const SESSION_READ_ATTEMPTS: usize = 3;
const SESSION_READ_RETRY_DELAY: Duration = Duration::from_millis(20);
const DISABLED_PREFIX: &str = "__DISABLED__";
const STICKER_WORKFLOW_USES: &str = "__sticker__";
const DEFAULT_IMAGE_INPUTS: &[&str] = &["image", "input_image", "input"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct HookCanvasBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct HookCanvasPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum HookCanvasNodeKind {
    Screenshot,
    Art,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HookCanvasNode {
    pub id: String,
    // Connected-component identity in Hook session world coordinates. Nodes with
    // the same component_id belong to the same pipeline regardless of current
    // viewport pan/zoom in the Loom frontend.
    pub component_id: String,
    // Stable YAML-safe workflow node id derived from Hook session data. This is
    // daemon-owned A-class graph metadata, independent of current viewport.
    pub workflow_node_id: String,
    // Direct upstream workflow node ids (already normalized to workflow_node_id
    // space) for workflow export serialization. Frontends should filter these by
    // the selected component/workflow scope instead of re-deriving graph edges.
    pub upstream_workflow_node_ids: Vec<String>,
    pub kind: HookCanvasNodeKind,
    pub label: String,
    pub art_id: Option<String>,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub preview_available: bool,
    pub preview_url: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub minified: bool,
    pub crop: Option<HookCanvasCrop>,
    // Hook renders opacity live (not baked into the image): `opacityMini` when
    // minified, else `opacityNormal`. Default 0.9 for mini and 1.0 for normal,
    // matching Hook's `getOpacity`.
    pub opacity: f64,
    // Raw node params (`unit.params`) passed through verbatim so the frontend can
    // show each Art node's current parameter values when exposing them as
    // workflow inputs. Null when the node has no params.
    #[serde(default)]
    pub params: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub result_candidates: Vec<HookCanvasResultCandidate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_result_index: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HookCanvasResultCandidate {
    pub index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub image_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_page_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u64>,
}

// When Hook shows a sticker minified, it does NOT scale the whole image down.
// The node box is a fixed `unit.w × unit.h` window (overflow hidden); the image
// is laid out at its full `savedRect` pixel size and shifted by `-cropOffset`,
// so the box shows a 1:1 crop window of the image. To reproduce this in a
// resolution-independent way, the daemon pre-computes ratios relative to the
// node box (window) size:
//   image_width_ratio  = savedRect.w / unit.w   (image is this × the box wide)
//   image_height_ratio = savedRect.h / unit.h
//   offset_x_ratio     = cropOffset.x / unit.w  (pan, as a fraction of box width)
//   offset_y_ratio     = cropOffset.y / unit.h
// The frontend then renders the image at `image_*_ratio × 100%` and offsets it
// by `-offset_*_ratio × 100%` of the node box — no dependency on the rendered
// pixel size or the canvas zoom.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HookCanvasCrop {
    pub image_width_ratio: f64,
    pub image_height_ratio: f64,
    pub offset_x_ratio: f64,
    pub offset_y_ratio: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HookCanvasEdge {
    pub id: String,
    pub source_node_id: String,
    pub source_port_id: Option<String>,
    // World-space output port anchor, already offset away from the node edge by
    // Hook's visual link gap. Frontends should only project this through their
    // current viewport/minimap transform.
    pub source_point: HookCanvasPoint,
    pub target_node_id: String,
    pub target_port_id: Option<String>,
    // World-space input port anchor, already offset away from the node edge by
    // Hook's visual link gap. Frontends should only project this through their
    // current viewport/minimap transform.
    pub target_point: HookCanvasPoint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum HookCanvasPreviewSource {
    File(PathBuf),
    DataUrl(String),
}

// A frozen canvas snapshot scoped to one connected component, plus each member
// node's preview source so the caller can persist image copies alongside it.
pub(crate) struct HookCanvasComponentSnapshot {
    pub snapshot: HookCanvasSnapshot,
    pub previews: Vec<(String, HookCanvasPreviewSource)>,
}

#[derive(Clone, Debug)]
struct HookCanvasSessionLink {
    from_unit_id: String,
    to_unit_id: String,
    to_port_id: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct HookCanvasResolvedPreview {
    source: Option<HookCanvasPreviewSource>,
    had_candidates: bool,
}

#[derive(Debug)]
pub(crate) struct HookCanvasDocument {
    pub snapshot: HookCanvasSnapshot,
    preview_sources: HashMap<String, HookCanvasPreviewSource>,
    preview_roots: Vec<PathBuf>,
}

#[derive(Debug, Error)]
pub(crate) enum HookCanvasError {
    #[error("unable to read Hook session: {0}")]
    Read(#[from] std::io::Error),
    #[error("invalid Hook session JSON: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Error)]
pub(crate) enum HookCanvasWorkflowExportError {
    #[error("Hook canvas node `{0}` was not found")]
    NodeNotFound(String),
    #[error("Hook canvas node `{0}` is not canonical and cannot be exported")]
    InvalidNode(String),
}

impl HookCanvasDocument {
    pub(crate) fn read(session_path: &Path) -> Result<Self, HookCanvasError> {
        let Some((bytes, root)) = read_session_value(session_path)? else {
            return Ok(Self::missing());
        };
        Ok(Self::from_serialized_root(
            session_path,
            bytes,
            root,
            modified_at_millis(session_path),
        ))
    }

    pub(crate) fn from_serialized_root(
        source_path: &Path,
        bytes: Vec<u8>,
        root: Value,
        updated_at: Option<String>,
    ) -> Self {
        let session_dir = source_path.parent().unwrap_or_else(|| Path::new("."));
        let preview_roots = canonical_preview_roots(session_dir);
        let mut warnings = Vec::new();
        let mut preview_sources = HashMap::new();
        let mut preview_versions = Vec::new();
        let mut node_ids = HashSet::new();
        let mut raw_nodes = HashMap::new();
        let mut nodes = Vec::new();

        let canvas_source = hook_canvas_source(&root);
        for raw_node in canvas_nodes(&root, canvas_source) {
            let Some(id) = non_empty_string(raw_node.get("id")) else {
                warnings.push("已跳过缺少有效 ID 的 Hook 节点。".to_owned());
                continue;
            };
            if !node_ids.insert(id.clone()) {
                warnings.push(format!("已跳过重复的 Hook 节点 `{id}`。"));
                continue;
            }

            let (x, x_degraded) = normalized_coordinate(node_coordinate(raw_node, "x", canvas_source));
            let (y, y_degraded) = normalized_coordinate(node_coordinate(raw_node, "y", canvas_source));
            let (width, width_degraded) =
                normalized_size(node_size(raw_node, "w", "width", canvas_source));
            let (height, height_degraded) =
                normalized_size(node_size(raw_node, "h", "height", canvas_source));
            if x_degraded || y_degraded || width_degraded || height_degraded {
                warnings.push(format!("Hook 节点 `{id}` 的几何信息已归一化。"));
            }

            let raw_art_id = node_string(raw_node, "artId");
            let node_type = node_type(raw_node);
            let kind = classify_node(node_type.as_deref(), raw_art_id.as_deref());
            let art_id = matches!(kind, HookCanvasNodeKind::Art)
                .then_some(raw_art_id)
                .flatten();
            let label = match kind {
                HookCanvasNodeKind::Screenshot => "截图节点",
                HookCanvasNodeKind::Art => "Art 节点",
                HookCanvasNodeKind::Unknown => "未知节点",
            }
            .to_owned();
            let status =
                normalized_status(node_string(raw_node, "status").as_deref(), &kind).to_owned();
            let error_message =
                node_string(raw_node, "errorMessage").or_else(|| node_string(raw_node, "error"));

            let minified = node_value(raw_node, "minified")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let crop = if minified {
                extract_crop(raw_node, width, height)
            } else {
                None
            };
            // Hook applies opacity at render time (not baked into the image):
            // opacityMini when minified, opacityNormal otherwise.
            let opacity_key = if minified {
                "opacityMini"
            } else {
                "opacityNormal"
            };
            let opacity = value_as_f64(node_value(raw_node, opacity_key))
                .map(|value| value.clamp(0.0, 1.0))
                .unwrap_or(1.0);
            let params = node_value(raw_node, "params")
                .cloned()
                .unwrap_or(Value::Null);
            let result_candidates = node_result_candidates(raw_node);
            let selected_result_index = node_selected_result_index(raw_node, &params);

            raw_nodes.insert(id.clone(), raw_node.clone());

            nodes.push(HookCanvasNode {
                id,
                component_id: String::new(),
                workflow_node_id: String::new(),
                upstream_workflow_node_ids: Vec::new(),
                kind,
                label,
                art_id,
                x,
                y,
                width,
                height,
                preview_available: false,
                preview_url: None,
                status,
                error_message,
                minified,
                crop,
                opacity,
                params,
                result_candidates,
                selected_result_index,
            });
        }
        nodes.sort_by(|left, right| left.id.cmp(&right.id));
        let node_lookup = nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect::<HashMap<_, _>>();

        let mut edges = Vec::new();
        let mut session_links = Vec::new();
        for (index, raw_edge) in canvas_edges(&root, canvas_source).iter().enumerate() {
            let source_node_id = edge_endpoint(raw_edge, canvas_source, EdgeEnd::Source);
            let target_node_id = edge_endpoint(raw_edge, canvas_source, EdgeEnd::Target);
            let (Some(source_node_id), Some(target_node_id)) = (source_node_id, target_node_id)
            else {
                warnings.push("已跳过缺少端点的 Hook 连线。".to_owned());
                continue;
            };
            let (Some(source_node), Some(target_node)) = (
                node_lookup.get(source_node_id.as_str()),
                node_lookup.get(target_node_id.as_str()),
            ) else {
                warnings.push(format!(
                    "已跳过端点不存在的 Hook 连线 `{source_node_id}` -> `{target_node_id}`。"
                ));
                continue;
            };
            let id =
                non_empty_string(raw_edge.get("id")).unwrap_or_else(|| format!("edge-{index:04}"));
            let target_port_id = edge_port(raw_edge, canvas_source, EdgeEnd::Target);
            session_links.push(HookCanvasSessionLink {
                from_unit_id: source_node_id.clone(),
                to_unit_id: target_node_id.clone(),
                to_port_id: target_port_id.clone(),
            });
            let (source_point, target_point) = edge_port_points(source_node, target_node);
            edges.push(HookCanvasEdge {
                id,
                source_node_id,
                source_port_id: edge_port(raw_edge, canvas_source, EdgeEnd::Source),
                source_point,
                target_node_id,
                target_port_id,
                target_point,
            });
        }
        edges.sort_by(|left, right| left.id.cmp(&right.id));

        let mut preview_cache = HashMap::new();
        for node in &mut nodes {
            let resolved = resolve_effective_preview_source(
                node.id.as_str(),
                &raw_nodes,
                &session_links,
                session_dir,
                &preview_roots,
                &mut preview_cache,
                &mut HashSet::new(),
            );
            if resolved.source.is_none() && resolved.had_candidates {
                warnings.push(format!("Hook 节点 `{}` 的预览不可用。", node.id));
            }
            let preview_version = resolved.source.as_ref().map(preview_source_version);
            if let Some(source) = resolved.source {
                preview_sources.insert(node.id.clone(), source);
                node.preview_available = true;
            }
            if let Some(version) = preview_version.as_deref() {
                preview_versions.push(format!("{}:{version}", node.id));
            }
            if node.preview_available {
                let base = format!(
                    "/v1/hook-bridge/canvas/nodes/{}/preview",
                    encode_path_segment(&node.id)
                );
                node.preview_url = Some(match preview_version.as_deref() {
                    Some(version) => format!("{base}?v={version}"),
                    None => base,
                });
            }
        }

        let component_ids = component_ids_for(&nodes, &edges);
        let (workflow_node_ids, mut upstream_workflow_node_ids) =
            workflow_export_metadata_for(&nodes, &edges);
        for node in &mut nodes {
            node.component_id = component_ids
                .get(node.id.as_str())
                .cloned()
                .unwrap_or_else(|| node.id.clone());
            node.workflow_node_id = workflow_node_ids
                .get(node.id.as_str())
                .cloned()
                .unwrap_or_else(|| sanitize_workflow_node_id(&node.id));
            node.upstream_workflow_node_ids = upstream_workflow_node_ids
                .remove(node.id.as_str())
                .unwrap_or_default();
        }

        let snapshot = HookCanvasSnapshot {
            available: true,
            revision: revision_for(&bytes, &preview_versions),
            updated_at,
            workflow_id: non_empty_string(root.get("workflowId")),
            bounds: canvas_bounds(&nodes),
            nodes,
            edges,
            warnings,
        };
        Self {
            snapshot,
            preview_sources,
            preview_roots,
        }
    }

    #[cfg(test)]
    pub(crate) fn preview_path(&self, node_id: &str) -> Option<&Path> {
        match self.preview_sources.get(node_id) {
            Some(HookCanvasPreviewSource::File(path)) => Some(path.as_path()),
            _ => None,
        }
    }

    pub(crate) fn preview_source(&self, node_id: &str) -> Option<&HookCanvasPreviewSource> {
        self.preview_sources.get(node_id)
    }

    pub(crate) fn override_preview_source(
        &mut self,
        node_id: &str,
        source: HookCanvasPreviewSource,
        cache_token: Option<&str>,
    ) {
        self.preview_sources.insert(node_id.to_owned(), source);
        let Some(node) = self
            .snapshot
            .nodes
            .iter_mut()
            .find(|node| node.id == node_id)
        else {
            return;
        };
        node.preview_available = true;
        let base = format!(
            "/v1/hook-bridge/canvas/nodes/{}/preview",
            encode_path_segment(node_id)
        );
        node.preview_url = Some(match cache_token {
            Some(token) if !token.trim().is_empty() => format!("{base}?v={token}"),
            _ => base,
        });
    }

    pub(crate) fn preview_roots(&self) -> &[PathBuf] {
        &self.preview_roots
    }

    pub(crate) fn export_workflow_yaml_for_selected_node(
        &self,
        selected_node_id: &str,
        workflow_name: &str,
    ) -> Result<String, HookCanvasWorkflowExportError> {
        let Some(selected_node) = self
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == selected_node_id)
        else {
            return Err(HookCanvasWorkflowExportError::NodeNotFound(
                selected_node_id.to_owned(),
            ));
        };

        let members = self
            .snapshot
            .nodes
            .iter()
            .filter(|node| node.component_id == selected_node.component_id)
            .collect::<Vec<_>>();
        let safe_name = if workflow_name.trim().is_empty() {
            "hook-pipeline"
        } else {
            workflow_name.trim()
        };
        let mut lines = vec![
            format!("name: {}", yaml_single_quoted(safe_name)),
            "nodes:".to_owned(),
        ];
        if members.is_empty() {
            lines.push("  []".to_owned());
            return Ok(format!("{}\n", lines.join("\n")));
        }

        let selected_workflow_ids = members
            .iter()
            .map(|node| node.workflow_node_id.clone())
            .collect::<HashSet<_>>();
        let workflow_ids_by_raw_node = members
            .iter()
            .map(|node| (node.id.as_str(), node.workflow_node_id.as_str()))
            .collect::<HashMap<_, _>>();
        for node in members {
            lines.push(format!("  - id: {}", node.workflow_node_id));
            let uses = match &node.kind {
                HookCanvasNodeKind::Screenshot => STICKER_WORKFLOW_USES,
                HookCanvasNodeKind::Art => node
                    .art_id
                    .as_deref()
                    .ok_or_else(|| HookCanvasWorkflowExportError::InvalidNode(node.id.clone()))?,
                HookCanvasNodeKind::Unknown => {
                    return Err(HookCanvasWorkflowExportError::InvalidNode(node.id.clone()));
                }
            };
            lines.push(format!("    uses: {}", yaml_single_quoted(uses)));
            let needs = node
                .upstream_workflow_node_ids
                .iter()
                .filter(|id| selected_workflow_ids.contains(*id))
                .cloned()
                .collect::<Vec<_>>();
            if !needs.is_empty() {
                lines.push(format!("    needs: [{}]", needs.join(", ")));
            }
            let mut seen_target_ports = HashSet::new();
            let incoming_edges = self
                .snapshot
                .edges
                .iter()
                .filter(|edge| edge.target_node_id == node.id)
                .filter_map(|edge| {
                    let source_node_id = workflow_ids_by_raw_node
                        .get(edge.source_node_id.as_str())?
                        .to_string();
                    let source_port_id = edge
                        .source_port_id
                        .as_deref()
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or("output_image")
                        .to_owned();
                    let target_port_id = edge
                        .target_port_id
                        .as_deref()
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or("image")
                        .to_owned();
                    if !seen_target_ports.insert(target_port_id.clone()) {
                        return None;
                    }
                    Some((source_node_id, source_port_id, target_port_id))
                })
                .collect::<Vec<_>>();
            if !incoming_edges.is_empty() {
                lines.push("    with:".to_owned());
                for (source_node_id, source_port_id, target_port_id) in incoming_edges {
                    let reference =
                        format!("${{{{ nodes.{source_node_id}.outputs.{source_port_id} }}}}");
                    lines.push(format!(
                        "      {}: {}",
                        yaml_mapping_key(&target_port_id),
                        yaml_single_quoted(&reference)
                    ));
                }
            }
        }

        Ok(format!("{}\n", lines.join("\n")))
    }

    // Build a frozen snapshot scoped to the selected node's connected component,
    // plus each member node's current preview source. The snapshot keeps node
    // geometry/crop and the in-component edges, so it renders identically to the
    // live canvas. The caller persists the images and rewrites each node's
    // `preview_url` to point at the saved-workflow preview route.
    pub(crate) fn component_snapshot_for_selected_node(
        &self,
        selected_node_id: &str,
    ) -> Result<HookCanvasComponentSnapshot, HookCanvasWorkflowExportError> {
        let Some(selected_node) = self
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == selected_node_id)
        else {
            return Err(HookCanvasWorkflowExportError::NodeNotFound(
                selected_node_id.to_owned(),
            ));
        };
        let component_id = selected_node.component_id.clone();

        let nodes: Vec<HookCanvasNode> = self
            .snapshot
            .nodes
            .iter()
            .filter(|node| node.component_id == component_id)
            .cloned()
            .collect();
        let member_ids: HashSet<String> = nodes.iter().map(|node| node.id.clone()).collect();
        let edges: Vec<HookCanvasEdge> = self
            .snapshot
            .edges
            .iter()
            .filter(|edge| {
                member_ids.contains(&edge.source_node_id)
                    && member_ids.contains(&edge.target_node_id)
            })
            .cloned()
            .collect();

        let previews: Vec<(String, HookCanvasPreviewSource)> = nodes
            .iter()
            .filter_map(|node| {
                self.preview_sources
                    .get(&node.id)
                    .map(|source| (node.id.clone(), source.clone()))
            })
            .collect();

        let snapshot = HookCanvasSnapshot {
            available: true,
            revision: self.snapshot.revision.clone(),
            updated_at: self.snapshot.updated_at.clone(),
            workflow_id: self.snapshot.workflow_id.clone(),
            bounds: canvas_bounds(&nodes),
            nodes,
            edges,
            warnings: Vec::new(),
        };
        Ok(HookCanvasComponentSnapshot { snapshot, previews })
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
            preview_sources: HashMap::new(),
            preview_roots: Vec::new(),
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

#[derive(Clone, Copy)]
enum HookCanvasSource {
    Session,
    Workflow,
}

#[derive(Clone, Copy)]
enum EdgeEnd {
    Source,
    Target,
}

fn hook_canvas_source(root: &Value) -> HookCanvasSource {
    if root.get("stickers").is_some() || root.get("links").is_some() {
        HookCanvasSource::Session
    } else {
        HookCanvasSource::Workflow
    }
}

fn canvas_nodes(root: &Value, source: HookCanvasSource) -> &[Value] {
    let key = match source {
        HookCanvasSource::Session => "stickers",
        HookCanvasSource::Workflow => "nodes",
    };
    root.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn canvas_edges(root: &Value, source: HookCanvasSource) -> &[Value] {
    let key = match source {
        HookCanvasSource::Session => "links",
        HookCanvasSource::Workflow => "edges",
    };
    root.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn edge_endpoint(raw_edge: &Value, source: HookCanvasSource, end: EdgeEnd) -> Option<String> {
    let key = match (source, end) {
        (HookCanvasSource::Session, EdgeEnd::Source) => "fromUnitId",
        (HookCanvasSource::Session, EdgeEnd::Target) => "toUnitId",
        (HookCanvasSource::Workflow, EdgeEnd::Source) => "source",
        (HookCanvasSource::Workflow, EdgeEnd::Target) => "target",
    };
    first_non_empty_string(raw_edge, &[key])
}

fn edge_port(raw_edge: &Value, source: HookCanvasSource, end: EdgeEnd) -> Option<String> {
    let key = match (source, end) {
        (HookCanvasSource::Session, EdgeEnd::Source) => "fromPortId",
        (HookCanvasSource::Session, EdgeEnd::Target) => "toPortId",
        (HookCanvasSource::Workflow, EdgeEnd::Source) => "sourceHandle",
        (HookCanvasSource::Workflow, EdgeEnd::Target) => "targetHandle",
    };
    first_non_empty_string(raw_edge, &[key])
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

fn node_art_result_metadata(node: &Value) -> Option<&Value> {
    node_value(node, "loomMetadata").and_then(|metadata| metadata.get("candidates"))
}

fn node_result_candidates(node: &Value) -> Vec<HookCanvasResultCandidate> {
    let metadata = node_art_result_metadata(node);
    let items = metadata
        .and_then(|metadata| metadata.get("items"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let image_url = item.get("imageUrl").and_then(Value::as_str)?.to_owned();
                    Some(HookCanvasResultCandidate {
                        index: item.get("index").and_then(Value::as_u64).unwrap_or(0) as usize,
                        title: item.get("title").and_then(Value::as_str).map(str::to_owned),
                        image_url,
                        thumbnail: item
                            .get("thumbnail")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        preview: item
                            .get("preview")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        thumbnail_url: item
                            .get("thumbnailUrl")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        source_page_url: item
                            .get("sourcePageUrl")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        width: item.get("width").and_then(Value::as_u64),
                        height: item.get("height").and_then(Value::as_u64),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    items
}

fn node_selected_result_index(node: &Value, params: &Value) -> Option<usize> {
    node_art_result_metadata(node)
        .and_then(|metadata| metadata.get("selectedIndex"))
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .or_else(|| {
            params
                .get("result_index")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
        })
}

fn node_coordinate(node: &Value, key: &str, source: HookCanvasSource) -> Option<f64> {
    match source {
        HookCanvasSource::Session => value_as_f64(node.get(key))
            .or_else(|| value_as_f64(node_data(node).and_then(|data| data.get(key)))),
        HookCanvasSource::Workflow => value_as_f64(node_nested_value(node, "position", key)),
    }
}

fn node_size(
    node: &Value,
    short_key: &str,
    long_key: &str,
    source: HookCanvasSource,
) -> Option<f64> {
    match source {
        HookCanvasSource::Session => value_as_f64(node.get(short_key))
            .or_else(|| value_as_f64(node.get(long_key)))
            .or_else(|| value_as_f64(node_data(node).and_then(|data| data.get(short_key))))
            .or_else(|| value_as_f64(node_data(node).and_then(|data| data.get(long_key)))),
        HookCanvasSource::Workflow => value_as_f64(node_nested_value(node, "measured", long_key))
            .or_else(|| value_as_f64(node.get(long_key))),
    }
}

// Build the crop viewport for a minified sticker, mirroring Hook's
// `computeMinifiedStickerViewport`: the source image is rendered at
// `savedRect` size and shifted by `cropOffset` so the node window shows a
// local region of the full image instead of the whole image scaled down.
// `imageEditState.cropRect/sourceSize` further refines the source region when
// present. Returns None when the node has no savedRect (nothing to crop).
fn extract_crop(node: &Value, window_width: f64, window_height: f64) -> Option<HookCanvasCrop> {
    if !(window_width > 0.0) || !(window_height > 0.0) {
        return None;
    }
    let saved_rect = node_value(node, "savedRect")?;
    let source_width = value_as_f64(saved_rect.get("w"))?;
    let source_height = value_as_f64(saved_rect.get("h"))?;
    if !(source_width > 0.0) || !(source_height > 0.0) {
        return None;
    }
    let base_offset_x = node_value(node, "cropOffset")
        .and_then(|value| value_as_f64(value.get("x")))
        .unwrap_or(0.0);
    let base_offset_y = node_value(node, "cropOffset")
        .and_then(|value| value_as_f64(value.get("y")))
        .unwrap_or(0.0);

    // Hook's getMinifiedViewport: imageEditState.cropRect/sourceSize refines the
    // laid-out source region when present, otherwise the whole savedRect is used.
    let (viewport_width, viewport_height, offset_x, offset_y) = node_value(node, "imageEditState")
        .and_then(|edit| {
            let crop_rect = edit.get("cropRect")?;
            let source_size = edit.get("sourceSize")?;
            let width = value_as_f64(source_size.get("w"))?;
            let height = value_as_f64(source_size.get("h"))?;
            let crop_x = value_as_f64(crop_rect.get("x"))?;
            let crop_y = value_as_f64(crop_rect.get("y"))?;
            (width > 0.0 && height > 0.0).then_some((
                width,
                height,
                crop_x + base_offset_x,
                crop_y + base_offset_y,
            ))
        })
        .unwrap_or((source_width, source_height, base_offset_x, base_offset_y));

    // Corner-click special case: Hook clamps the crop offset at minify time so
    // the crop window never leaves the image (useUnitActions):
    //   offset = clamp(raw, 0, max(0, savedRect - window))
    // A double-click near an edge/corner would otherwise push the window past the
    // image edge and expose blank space. We reproduce that clamp defensively so
    // the window's far edge aligns with the image edge regardless of what the
    // stored offset was.
    let max_offset_x = (viewport_width - window_width).max(0.0);
    let max_offset_y = (viewport_height - window_height).max(0.0);
    let offset_x = offset_x.clamp(0.0, max_offset_x);
    let offset_y = offset_y.clamp(0.0, max_offset_y);

    // Ratios relative to the node window (unit.w/unit.h), mirroring Hook's
    // `img { width: viewport.width px; left: -viewport.offsetX px }` inside a
    // `unit.w × unit.h` overflow-hidden box.
    Some(HookCanvasCrop {
        image_width_ratio: viewport_width / window_width,
        image_height_ratio: viewport_height / window_height,
        offset_x_ratio: offset_x / window_width,
        offset_y_ratio: offset_y / window_height,
    })
}

fn node_string(node: &Value, key: &str) -> Option<String> {
    non_empty_string(node_value(node, key))
}

fn node_type(node: &Value) -> Option<String> {
    non_empty_string(node.get("type"))
}

fn is_image_like_port_name(name: &str) -> bool {
    let normalized = name.trim().to_ascii_lowercase();
    normalized.contains("image")
        || DEFAULT_IMAGE_INPUTS.contains(&normalized.as_str())
        || normalized.ends_with("_image")
        || normalized.ends_with("_file")
}

fn find_connected_image_input<'a>(
    node_id: &str,
    links: &'a [HookCanvasSessionLink],
) -> Option<&'a HookCanvasSessionLink> {
    links.iter().find(|link| {
        link.to_unit_id == node_id
            && link
                .to_port_id
                .as_deref()
                .is_some_and(is_image_like_port_name)
    })
}

fn sticker_image_input_disabled(node: &Value) -> bool {
    node_nested_value(node, "params", "image")
        .and_then(Value::as_str)
        .is_some_and(|value| value == DISABLED_PREFIX)
}

fn sticker_manual_image_data_url(node: &Value) -> Option<String> {
    node_nested_value(node, "params", "image_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| is_supported_image_data_url(value))
        .map(str::to_owned)
}

fn sticker_annotation_count(node: &Value) -> usize {
    node_value(node, "annotationState")
        .and_then(|state| state.get("elements"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

fn has_meaningful_image_edit_state(node: &Value) -> bool {
    let Some(state) = node_value(node, "imageEditState") else {
        return false;
    };

    if state
        .get("contentEraseStrokes")
        .and_then(Value::as_array)
        .is_some_and(|strokes| !strokes.is_empty())
    {
        return true;
    }
    if state.get("cropRect").is_some_and(|value| !value.is_null()) {
        return true;
    }
    if state
        .get("sourceSize")
        .is_some_and(|value| !value.is_null())
    {
        return true;
    }
    if value_as_f64(state.get("rotation")).is_some_and(|rotation| rotation != 0.0) {
        return true;
    }
    if state
        .get("flippedX")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }
    if state
        .get("flippedY")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }
    if value_as_f64(state.get("borderWidth")).is_some_and(|width| width > 0.0) {
        return true;
    }
    if non_empty_string(state.get("borderColor")).is_some() {
        return true;
    }
    if value_as_f64(state.get("cornerRadius")).is_some_and(|radius| radius > 0.0) {
        return true;
    }

    state
        .get("beautify")
        .and_then(|beautify| beautify.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn sticker_requires_local_baked_preview(node: &Value) -> bool {
    sticker_annotation_count(node) > 0
        || node_string(node, "rasterizedAnnotationLayerSrc").is_some()
        || has_meaningful_image_edit_state(node)
}

fn push_normalized_preview_source(sources: &mut Vec<String>, value: Option<String>) {
    if let Some(raw) = value {
        if let Some(path) = normalize_preview_source(&raw) {
            if !sources.contains(&path) {
                sources.push(path);
            }
        }
    }
}

fn node_preview_only_sources(node: &Value) -> Vec<String> {
    let mut sources = Vec::new();
    push_normalized_preview_source(&mut sources, non_empty_string(node.get("previewSrc")));
    push_normalized_preview_source(
        &mut sources,
        node_data(node).and_then(|data| non_empty_string(data.get("previewSrc"))),
    );
    sources
}

fn node_src_fallback_sources(node: &Value) -> Vec<String> {
    let mut sources = Vec::new();
    push_normalized_preview_source(&mut sources, non_empty_string(node.get("src")));
    push_normalized_preview_source(
        &mut sources,
        node_data(node).and_then(|data| non_empty_string(data.get("src"))),
    );
    push_normalized_preview_source(&mut sources, non_empty_string(node.get("filePath")));
    push_normalized_preview_source(
        &mut sources,
        node_data(node).and_then(|data| non_empty_string(data.get("filePath"))),
    );
    sources
}

// Hook references a node's image through several shapes: a Tauri asset URL
// (`http://asset.localhost/<percent-encoded-path>`), a `file://` URL, a plain
// absolute path, or a clean `filePath` field. Return every candidate in
// preference order (a preview-sized image first, then the full image, then the
// raw file path) so the caller can pick the first that resolves to a real file.
fn node_preview_sources(node: &Value) -> Vec<String> {
    let mut sources = node_preview_only_sources(node);
    for source in node_src_fallback_sources(node) {
        if !sources.contains(&source) {
            sources.push(source);
        }
    }
    sources
}

fn resolve_effective_preview_source(
    node_id: &str,
    raw_nodes: &HashMap<String, Value>,
    links: &[HookCanvasSessionLink],
    session_dir: &Path,
    preview_roots: &[PathBuf],
    cache: &mut HashMap<String, HookCanvasResolvedPreview>,
    visiting: &mut HashSet<String>,
) -> HookCanvasResolvedPreview {
    if let Some(cached) = cache.get(node_id) {
        return cached.clone();
    }
    if !visiting.insert(node_id.to_owned()) {
        return HookCanvasResolvedPreview::default();
    }

    let resolved = raw_nodes
        .get(node_id)
        .map_or_else(HookCanvasResolvedPreview::default, |node| {
            let node_kind = classify_node(
                node_type(node).as_deref(),
                node_string(node, "artId").as_deref(),
            );

            if matches!(node_kind, HookCanvasNodeKind::Screenshot) {
                if sticker_requires_local_baked_preview(node) {
                    let local_sources = node_preview_sources(node);
                    return HookCanvasResolvedPreview {
                        source: resolve_first_preview_source(
                            session_dir,
                            preview_roots,
                            &local_sources,
                        ),
                        had_candidates: !local_sources.is_empty(),
                    };
                }

                let mut had_candidates = false;
                if !sticker_image_input_disabled(node) {
                    if let Some(link) = find_connected_image_input(node_id, links) {
                        had_candidates = true;
                        let upstream = resolve_effective_preview_source(
                            link.from_unit_id.as_str(),
                            raw_nodes,
                            links,
                            session_dir,
                            preview_roots,
                            cache,
                            visiting,
                        );
                        if upstream.source.is_some() {
                            return upstream;
                        }
                        had_candidates |= upstream.had_candidates;
                    }
                    if let Some(manual_image) = sticker_manual_image_data_url(node) {
                        let source =
                            resolve_preview_source(session_dir, preview_roots, &manual_image);
                        return HookCanvasResolvedPreview {
                            source,
                            had_candidates: true,
                        };
                    }
                }

                let local_sources = node_preview_sources(node);
                return HookCanvasResolvedPreview {
                    source: resolve_first_preview_source(
                        session_dir,
                        preview_roots,
                        &local_sources,
                    ),
                    had_candidates: had_candidates || !local_sources.is_empty(),
                };
            }

            let local_sources = node_preview_sources(node);
            if let Some(source) =
                resolve_first_preview_source(session_dir, preview_roots, &local_sources)
            {
                return HookCanvasResolvedPreview {
                    source: Some(source),
                    had_candidates: true,
                };
            }

            let mut had_candidates = !local_sources.is_empty();
            if let Some(link) = find_connected_image_input(node_id, links) {
                had_candidates = true;
                let upstream = resolve_effective_preview_source(
                    link.from_unit_id.as_str(),
                    raw_nodes,
                    links,
                    session_dir,
                    preview_roots,
                    cache,
                    visiting,
                );
                if upstream.source.is_some() {
                    return upstream;
                }
                had_candidates |= upstream.had_candidates;
            }
            HookCanvasResolvedPreview {
                source: None,
                had_candidates,
            }
        });

    visiting.remove(node_id);
    cache.insert(node_id.to_owned(), resolved.clone());
    resolved
}

// Convert a Hook image reference into a local filesystem path. Tauri asset URLs
// and `file://` URLs are decoded; plain paths pass through unchanged. Returns
// `None` for remote (http/https non-asset) references Loom must not fetch.
fn normalize_preview_source(source: &str) -> Option<String> {
    let trimmed = source.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = asset_url_path(trimmed) {
        return non_empty_after_decode(&rest);
    }
    if let Some(rest) = trimmed.strip_prefix("file://") {
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        return non_empty_after_decode(rest);
    }
    // Any other URL scheme (http/https to a real host) is a remote resource that
    // the local preview endpoint must not read from disk.
    if looks_like_remote_url(trimmed) {
        return None;
    }
    Some(trimmed.to_owned())
}

// Extract the path portion of a Tauri asset URL such as
// `http://asset.localhost/C%3A%5C...png` or `asset://localhost/...`.
fn asset_url_path(source: &str) -> Option<String> {
    let without_scheme = source
        .strip_prefix("http://")
        .or_else(|| source.strip_prefix("https://"))
        .or_else(|| source.strip_prefix("asset://"))?;
    let (host, rest) = without_scheme.split_once('/')?;
    if !host.eq_ignore_ascii_case("asset.localhost") && !host.eq_ignore_ascii_case("localhost") {
        return None;
    }
    Some(rest.to_owned())
}

fn looks_like_remote_url(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn non_empty_after_decode(encoded: &str) -> Option<String> {
    let decoded = percent_decode(encoded);
    let trimmed = decoded.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let high = (bytes[index + 1] as char).to_digit(16);
                let low = (bytes[index + 2] as char).to_digit(16);
                if let (Some(high), Some(low)) = (high, low) {
                    decoded.push((high * 16 + low) as u8);
                    index += 3;
                    continue;
                }
                decoded.push(bytes[index]);
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
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
    if matches!(node_type, Some("art" | "artNode"))
        && art_id.is_some_and(|value| loom_workflow_store::validate_art_id(value).is_ok())
    {
        HookCanvasNodeKind::Art
    } else if node_type == Some("sticker") {
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

// Directories the daemon is allowed to serve preview images from. Hook stores
// canvas images in two places: the session's own `images/` directory and the
// shared `clipboard_cache` under `%LOCALAPPDATA%\Hook`
// (current live captures referenced by absolute path or Tauri asset URL). Both
// are canonicalized so the preview endpoint can enforce a strict prefix check
// and never read outside them.
fn canonical_preview_roots(session_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut push = |path: PathBuf| {
        if let Ok(canonical) = fs::canonicalize(&path) {
            if canonical.is_dir() && !roots.contains(&canonical) {
                roots.push(canonical);
            }
        }
    };
    push(session_dir.join("images"));
    for root in hook_clipboard_cache_roots() {
        push(root);
    }
    roots
}

// Candidate `clipboard_cache` locations. Hook writes live capture images to
// `%LOCALAPPDATA%\Hook\clipboard_cache`; an explicit override supports isolated
// smokes and non-default installs.
fn hook_clipboard_cache_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(dir) = std::env::var_os("LOOM_HOOK_IMAGE_ROOT") {
        let dir = PathBuf::from(dir);
        if !dir.as_os_str().is_empty() {
            roots.push(dir);
        }
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("Hook").join("clipboard_cache"));
    }
    roots
}

fn resolve_first_preview_source(
    session_dir: &Path,
    preview_roots: &[PathBuf],
    sources: &[String],
) -> Option<HookCanvasPreviewSource> {
    sources
        .iter()
        .find_map(|source| resolve_preview_source(session_dir, preview_roots, source))
}

fn resolve_preview_source(
    session_dir: &Path,
    preview_roots: &[PathBuf],
    source: &str,
) -> Option<HookCanvasPreviewSource> {
    let trimmed = source.trim();
    if is_supported_image_data_url(trimmed) {
        return Some(HookCanvasPreviewSource::DataUrl(trimmed.to_owned()));
    }
    if preview_roots.is_empty() {
        return None;
    }
    let source_path = Path::new(trimmed);
    let candidate = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        session_dir.join(source_path)
    };
    let candidate = fs::canonicalize(candidate).ok()?;
    (candidate.is_file() && preview_roots.iter().any(|root| candidate.starts_with(root)))
        .then_some(HookCanvasPreviewSource::File(candidate))
}

fn is_supported_image_data_url(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    lower.starts_with("data:image/") && lower.contains(";base64,")
}

fn revision_for(bytes: &[u8], preview_versions: &[String]) -> String {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    // Fold each node's preview content version into the revision so an in-place
    // image update (same session.json, same node id, same file path) still
    // produces a new revision and forces the desktop to replace its snapshot.
    for version in preview_versions {
        version.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

// A cheap content version for a preview image derived from its size and last
// modification time. This changes when Hook overwrites the file in place, which
// lets the preview URL bust WebView/browser caching without reading the whole
// image on every canvas read.
fn preview_source_version(source: &HookCanvasPreviewSource) -> String {
    match source {
        HookCanvasPreviewSource::File(path) => preview_file_content_version(path),
        HookCanvasPreviewSource::DataUrl(data_url) => preview_data_url_content_version(data_url),
    }
}

fn preview_file_content_version(path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    if let Ok(metadata) = fs::metadata(path) {
        metadata.len().hash(&mut hasher);
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                duration.as_nanos().hash(&mut hasher);
            }
        }
    }
    format!("{:016x}", hasher.finish())
}

fn preview_data_url_content_version(data_url: &str) -> String {
    let mut hasher = DefaultHasher::new();
    data_url.hash(&mut hasher);
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

fn edge_port_points(
    source: &HookCanvasNode,
    target: &HookCanvasNode,
) -> (HookCanvasPoint, HookCanvasPoint) {
    let source_gap = if source.minified {
        MINIFIED_EDGE_PORT_GAP
    } else {
        DEFAULT_EDGE_PORT_GAP
    };
    let target_gap = if target.minified {
        MINIFIED_EDGE_PORT_GAP
    } else {
        DEFAULT_EDGE_PORT_GAP
    };
    (
        HookCanvasPoint {
            x: source.x + source.width + source_gap,
            y: source.y + source.height / 2.0,
        },
        HookCanvasPoint {
            x: target.x - target_gap,
            y: target.y + target.height / 2.0,
        },
    )
}

fn component_ids_for(
    nodes: &[HookCanvasNode],
    edges: &[HookCanvasEdge],
) -> HashMap<String, String> {
    let mut adjacency = nodes
        .iter()
        .map(|node| (node.id.clone(), Vec::<String>::new()))
        .collect::<HashMap<_, _>>();
    for edge in edges {
        if let Some(neighbors) = adjacency.get_mut(&edge.source_node_id) {
            neighbors.push(edge.target_node_id.clone());
        }
        if let Some(neighbors) = adjacency.get_mut(&edge.target_node_id) {
            neighbors.push(edge.source_node_id.clone());
        }
    }

    let mut visited = HashSet::new();
    let mut component_ids = HashMap::new();

    for node in nodes {
        if visited.contains(&node.id) {
            continue;
        }

        let mut queue = std::collections::VecDeque::from([node.id.clone()]);
        let mut members = Vec::new();
        visited.insert(node.id.clone());

        while let Some(current) = queue.pop_front() {
            members.push(current.clone());
            for neighbor in adjacency.get(&current).into_iter().flatten() {
                if visited.insert(neighbor.clone()) {
                    queue.push_back(neighbor.clone());
                }
            }
        }

        members.sort();
        let component_id = members.first().cloned().unwrap_or_else(|| node.id.clone());
        for member in members {
            component_ids.insert(member, component_id.clone());
        }
    }

    component_ids
}

fn workflow_export_metadata_for(
    nodes: &[HookCanvasNode],
    edges: &[HookCanvasEdge],
) -> (HashMap<String, String>, HashMap<String, Vec<String>>) {
    let mut ordered_nodes = nodes.iter().collect::<Vec<_>>();
    ordered_nodes.sort_by(|left, right| left.id.cmp(&right.id));

    let mut used_ids = HashSet::new();
    let mut workflow_node_ids = HashMap::new();

    for node in ordered_nodes {
        let base = workflow_node_id_base(node);
        let mut candidate = base.clone();
        let mut suffix = 2;
        while used_ids.contains(&candidate) {
            candidate = format!("{base}-{suffix}");
            suffix += 1;
        }
        used_ids.insert(candidate.clone());
        workflow_node_ids.insert(node.id.clone(), candidate);
    }

    let mut upstream_workflow_node_ids = nodes
        .iter()
        .map(|node| (node.id.clone(), Vec::<String>::new()))
        .collect::<HashMap<_, _>>();

    for edge in edges {
        let Some(source_workflow_node_id) = workflow_node_ids.get(&edge.source_node_id).cloned()
        else {
            continue;
        };
        let Some(target_upstreams) = upstream_workflow_node_ids.get_mut(&edge.target_node_id)
        else {
            continue;
        };
        if !target_upstreams.contains(&source_workflow_node_id) {
            target_upstreams.push(source_workflow_node_id);
        }
    }

    for upstreams in upstream_workflow_node_ids.values_mut() {
        upstreams.sort();
    }

    (workflow_node_ids, upstream_workflow_node_ids)
}

fn workflow_node_id_base(node: &HookCanvasNode) -> String {
    let base = node
        .art_id
        .as_deref()
        .and_then(|art_id| art_id.rsplit('/').next())
        .unwrap_or(node.id.as_str());
    sanitize_workflow_node_id(base)
}

fn yaml_single_quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn yaml_mapping_key(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        value.to_owned()
    } else {
        yaml_single_quoted(value)
    }
}

fn sanitize_workflow_node_id(value: &str) -> String {
    let mut sanitized = String::new();
    let mut previous_was_dash = false;

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            sanitized.push(ch);
            previous_was_dash = false;
        } else if !previous_was_dash {
            sanitized.push('-');
            previous_was_dash = true;
        }
    }

    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        "node".to_owned()
    } else {
        trimmed.to_owned()
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
        let session_dir = root.join("com.yamiyu.hook");
        fs::create_dir_all(session_dir.join("images")).expect("create Hook fixture dirs");
        let path = session_dir.join("session.json");
        fs::write(&path, json).expect("write Hook session fixture");
        path
    }

    // Percent-encode a filesystem path the way Tauri's asset protocol does so the
    // fixture matches the real `http://asset.localhost/<encoded>` shape Hook writes.
    fn encode_asset_url_path(path: &str) -> String {
        let mut encoded = String::new();
        for byte in path.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                    encoded.push(char::from(byte));
                }
                _ => encoded.push_str(&format!("%{byte:02X}")),
            }
        }
        encoded
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
                {"id":"art","type":"art","artId":"neuro.official/custom-image","src":"images/art.png","x":1576.0,"y":499.0,"w":60.0,"h":60.0}
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
        assert_eq!(art.art_id.as_deref(), Some("neuro.official/custom-image"));
        assert!(art.preview_available);
        assert!(art
            .preview_url
            .as_deref()
            .expect("art preview url")
            .starts_with("/v1/hook-bridge/canvas/nodes/art/preview?v="));
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
    fn precomputes_world_edge_points_and_connected_component_ids() {
        let root = test_root("geometry");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"a","type":"sticker","x":0,"y":0,"w":80,"h":80},
                {"id":"b","type":"art","artId":"neuro.official/geometry-b","x":200,"y":0,"w":80,"h":80},
                {"id":"c","type":"art","artId":"neuro.official/geometry-c","x":400,"y":0,"w":80,"h":80},
                {"id":"mini","type":"sticker","x":0,"y":200,"w":80,"h":80,"minified":true}
              ],
              "links": [
                {"id":"e1","fromUnitId":"a","toUnitId":"b"},
                {"id":"e2","fromUnitId":"b","toUnitId":"c"}
              ]
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("normalize geometry");
        let node = |id: &str| {
            document
                .snapshot
                .nodes
                .iter()
                .find(|node| node.id == id)
                .expect("node present")
        };
        let edge = |id: &str| {
            document
                .snapshot
                .edges
                .iter()
                .find(|edge| edge.id == id)
                .expect("edge present")
        };

        let component = node("a").component_id.clone();
        assert_eq!(node("b").component_id, component);
        assert_eq!(node("c").component_id, component);
        assert_eq!(node("mini").component_id, "mini");

        assert_eq!(
            edge("e1").source_point,
            HookCanvasPoint { x: 86.0, y: 40.0 }
        );
        assert_eq!(
            edge("e1").target_point,
            HookCanvasPoint { x: 194.0, y: 40.0 }
        );
        assert_eq!(
            edge_port_points(node("mini"), node("b")).0,
            HookCanvasPoint { x: 84.0, y: 240.0 }
        );
    }

    #[test]
    fn precomputes_unique_workflow_export_metadata() {
        let root = test_root("workflow-export");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"capture","type":"sticker","x":0,"y":0,"w":80,"h":80},
                {"id":"resize-a","type":"art","artId":"neuro.official/resize","x":200,"y":0,"w":80,"h":80},
                {"id":"resize-b","type":"art","artId":"neuro.official/resize","x":400,"y":0,"w":80,"h":80}
              ],
              "links": [
                {"id":"e1","fromUnitId":"capture","toUnitId":"resize-a"},
                {"id":"e2","fromUnitId":"resize-a","toUnitId":"resize-b"}
              ]
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("normalize workflow export");
        let capture = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "capture")
            .expect("capture node");
        let resize_a = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "resize-a")
            .expect("resize-a node");
        let resize_b = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "resize-b")
            .expect("resize-b node");

        assert_eq!(capture.workflow_node_id, "capture");
        assert_eq!(resize_a.workflow_node_id, "resize");
        assert_eq!(resize_b.workflow_node_id, "resize-2");
        assert_eq!(capture.upstream_workflow_node_ids, Vec::<String>::new());
        assert_eq!(
            resize_a.upstream_workflow_node_ids,
            vec!["capture".to_string()]
        );
        assert_eq!(
            resize_b.upstream_workflow_node_ids,
            vec!["resize".to_string()]
        );
    }

    #[test]
    fn exports_selected_component_as_workflow_yaml() {
        let root = test_root("workflow-yaml");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"a","type":"sticker","x":0,"y":0,"w":80,"h":80},
                {"id":"b","type":"art","artId":"neuro.official/resize","x":200,"y":0,"w":80,"h":80},
                {"id":"c","type":"art","artId":"neuro.official/resize","x":400,"y":0,"w":80,"h":80},
                {"id":"lonely","type":"sticker","x":0,"y":200,"w":80,"h":80}
              ],
              "links": [
                {"id":"e1","fromUnitId":"a","toUnitId":"b"},
                {"id":"e2","fromUnitId":"b","toUnitId":"c"}
              ]
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("normalize workflow yaml");
        let yaml = document
            .export_workflow_yaml_for_selected_node("a", "hook-export")
            .expect("export workflow yaml");

        assert!(yaml.contains("name: 'hook-export'"));
        assert!(yaml.contains("- id: a"));
        assert!(yaml.contains("uses: '__sticker__'"));
        assert!(yaml.contains("uses: 'neuro.official/resize'"));
        assert!(yaml.contains("- id: resize"));
        assert!(yaml.contains("- id: resize-2"));
        assert!(yaml.contains("needs: [a]"));
        assert!(yaml.contains("needs: [resize]"));
        assert!(yaml.contains("image: '${{ nodes.a.outputs.output_image }}'"));
        assert!(yaml.contains("image: '${{ nodes.resize.outputs.output_image }}'"));
        assert!(!yaml.contains("lonely"));
    }

    #[test]
    fn exports_multi_image_edge_target_ports_into_workflow_yaml() {
        let root = test_root("workflow-yaml-multi-image");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"input","type":"sticker","x":0,"y":0,"w":80,"h":80},
                {"id":"reference","type":"sticker","x":0,"y":200,"w":80,"h":80},
                {"id":"color","type":"art","artId":"neuro.official/color-transfer","x":200,"y":100,"w":80,"h":80},
                {"id":"compress","type":"art","artId":"neuro.official/compress","x":400,"y":100,"w":80,"h":80}
              ],
              "links": [
                {"id":"e1","fromUnitId":"input","fromPortId":"output","toUnitId":"color","toPortId":"input"},
                {"id":"e2","fromUnitId":"reference","fromPortId":"output_image","toUnitId":"color","toPortId":"reference"},
                {"id":"e3","fromUnitId":"color","fromPortId":"output","toUnitId":"compress","toPortId":"input"}
              ]
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("normalize multi-image workflow");
        let yaml = document
            .export_workflow_yaml_for_selected_node("input", "color-compress")
            .expect("export multi-image workflow yaml");

        assert!(yaml.contains("needs: [input, reference]"));
        assert!(yaml.contains("input: '${{ nodes.input.outputs.output }}'"));
        assert!(yaml.contains("reference: '${{ nodes.reference.outputs.output_image }}'"));
        assert!(yaml.contains("input: '${{ nodes.color-transfer.outputs.output }}'"));
    }

    #[test]
    fn rejects_workflow_export_with_noncanonical_art_identity() {
        let root = test_root("workflow-yaml-quoting");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"a","type":"sticker","x":0,"y":0,"w":80,"h":80},
                {"id":"b","type":"art","artId":"resize:smart's","x":200,"y":0,"w":80,"h":80}
              ],
              "links": [
                {"id":"e1","fromUnitId":"a","toUnitId":"b"}
              ]
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("normalize workflow yaml");
        let error = document
            .export_workflow_yaml_for_selected_node("a", "Hook: Export's")
            .expect_err("unsafe Art identity must fail closed");

        assert!(matches!(
            error,
            HookCanvasWorkflowExportError::InvalidNode(node_id) if node_id == "b"
        ));
    }

    #[test]
    fn extracts_minified_crop_window_from_saved_rect_and_offset() {
        let root = test_root("minified-crop");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {
                  "id":"mini",
                  "type":"sticker",
                  "src":"images/mini.png",
                  "x":2000.0,"y":-4.0,"w":100.0,"h":100.0,
                  "minified":true,
                  "savedRect":{"x":614.0,"y":1177.0,"w":461.0,"h":421.0},
                  "cropOffset":{"x":185.0,"y":72.0}
                },
                {
                  "id":"full",
                  "type":"sticker",
                  "src":"images/full.png",
                  "x":100.0,"y":100.0,"w":300.0,"h":200.0
                }
              ],
              "links": []
            }"#,
        );
        let images = session.parent().expect("session parent").join("images");
        fs::write(images.join("mini.png"), b"mini").expect("write mini preview");
        fs::write(images.join("full.png"), b"full").expect("write full preview");

        let document = HookCanvasDocument::read(&session).expect("normalize crop canvas");
        let mini = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "mini")
            .expect("mini node");
        assert!(mini.minified);
        let crop = mini.crop.as_ref().expect("crop window");
        // window is 100x100, savedRect 461x421, offset 185/72 → ratios to the box.
        assert_eq!(crop.image_width_ratio, 4.61);
        assert_eq!(crop.image_height_ratio, 4.21);
        assert_eq!(crop.offset_x_ratio, 1.85);
        assert_eq!(crop.offset_y_ratio, 0.72);

        let full = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "full")
            .expect("full node");
        assert!(!full.minified);
        assert!(full.crop.is_none());
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
        assert!(document.preview_roots().is_empty());
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
    fn revision_and_preview_url_change_when_image_is_updated_in_place() {
        let root = test_root("preview-version");
        let session = write_session(
            &root,
            r#"{"stickers":[{"id":"capture","type":"sticker","src":"images/capture.png"}],"links":[]}"#,
        );
        let images = session.parent().expect("session parent").join("images");
        fs::write(images.join("capture.png"), b"first").expect("write first preview");
        let first = HookCanvasDocument::read(&session).expect("first snapshot");

        // Overwrite the same node's image in place. The session JSON, node id, and
        // file path are all unchanged; only the image bytes differ.
        fs::write(images.join("capture.png"), b"second image bytes")
            .expect("overwrite preview in place");
        let second = HookCanvasDocument::read(&session).expect("second snapshot");

        assert_ne!(
            first.snapshot.revision, second.snapshot.revision,
            "in-place image update must produce a new revision"
        );
        let first_url = first.snapshot.nodes[0]
            .preview_url
            .as_deref()
            .expect("first preview url");
        let second_url = second.snapshot.nodes[0]
            .preview_url
            .as_deref()
            .expect("second preview url");
        assert!(first_url.starts_with("/v1/hook-bridge/canvas/nodes/capture/preview?v="));
        assert_ne!(
            first_url, second_url,
            "in-place image update must bust the preview URL cache token"
        );
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
    fn classifies_only_canonical_art_and_sticker_nodes() {
        let root = test_root("classification");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"art-by-id","artId":"neuro.official/resize"},
                {"id":"capture","type":"capture"},
                {"id":"art","type":"art","artId":"neuro.official/resize"},
                {"id":"sticker","type":"sticker"},
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
                ("art", &HookCanvasNodeKind::Art, "Art 节点"),
                ("art-by-id", &HookCanvasNodeKind::Unknown, "未知节点"),
                ("capture", &HookCanvasNodeKind::Unknown, "未知节点"),
                ("sticker", &HookCanvasNodeKind::Screenshot, "截图节点"),
                ("unknown", &HookCanvasNodeKind::Unknown, "未知节点"),
            ]
        );
    }

    #[test]
    fn passes_through_node_params_for_parameter_exposure() {
        let root = test_root("node-params");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"art","type":"art","artId":"neuro.official/resize","params":{"width":512,"mode":"fit"}},
                {"id":"plain","type":"sticker"}
              ],
              "links": []
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("read params");
        let art = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "art")
            .expect("art node");
        assert_eq!(art.params["width"], 512);
        assert_eq!(art.params["mode"], "fit");
        let plain = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "plain")
            .expect("plain node");
        assert!(plain.params.is_null());
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
    fn sticker_preview_uses_upstream_image_input_before_local_src() {
        let root = test_root("sticker-upstream-preview");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {
                  "id":"upstream",
                  "type":"sticker",
                  "src":"images/upstream-square.png",
                  "x":0,"y":0,"w":100,"h":100
                },
                {
                  "id":"target",
                  "type":"sticker",
                  "src":"images/target-rect.png",
                  "x":200,"y":0,"w":200,"h":100
                }
              ],
              "links": [
                {
                  "id":"upstream-image",
                  "fromUnitId":"upstream",
                  "fromPortId":"output",
                  "toUnitId":"target",
                  "toPortId":"image"
                }
              ]
            }"#,
        );
        let images = session.parent().expect("session parent").join("images");
        fs::write(images.join("upstream-square.png"), b"upstream").expect("write upstream");
        fs::write(images.join("target-rect.png"), b"target").expect("write target");
        let expected_upstream =
            fs::canonicalize(images.join("upstream-square.png")).expect("canonical upstream");

        let document = HookCanvasDocument::read(&session).expect("normalize upstream preview");

        assert_eq!(
            document.preview_path("target"),
            Some(expected_upstream.as_path()),
            "sticker preview should mirror Hook and display the upstream image input instead of stretching the target's own src",
        );
    }

    #[test]
    fn disabled_sticker_image_input_falls_back_to_local_src() {
        let root = test_root("sticker-disabled-input-preview");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {
                  "id":"upstream",
                  "type":"sticker",
                  "src":"images/upstream-square.png",
                  "x":0,"y":0,"w":100,"h":100
                },
                {
                  "id":"target",
                  "type":"sticker",
                  "src":"images/target-rect.png",
                  "x":200,"y":0,"w":200,"h":100,
                  "params":{"image":"__DISABLED__"}
                }
              ],
              "links": [
                {
                  "id":"upstream-image",
                  "fromUnitId":"upstream",
                  "fromPortId":"output",
                  "toUnitId":"target",
                  "toPortId":"image"
                }
              ]
            }"#,
        );
        let images = session.parent().expect("session parent").join("images");
        fs::write(images.join("upstream-square.png"), b"upstream").expect("write upstream");
        fs::write(images.join("target-rect.png"), b"target").expect("write target");
        let expected_target =
            fs::canonicalize(images.join("target-rect.png")).expect("canonical target");

        let document = HookCanvasDocument::read(&session).expect("normalize disabled preview");

        assert_eq!(
            document.preview_path("target"),
            Some(expected_target.as_path()),
            "when Hook disables the sticker image input, Loom must keep the node's own local preview",
        );
    }

    #[test]
    fn error_art_preview_prefers_local_src_over_upstream_input() {
        let root = test_root("error-art-local-preview");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {
                  "id":"upstream",
                  "type":"sticker",
                  "src":"images/upstream-square.png",
                  "x":0,"y":0,"w":100,"h":100
                },
                {
                  "id":"failed-art",
                  "type":"art",
                  "artId":"neuro.official/cloud-upscale",
                  "status":"error",
                  "src":"images/failed-art-error.png",
                  "x":200,"y":0,"w":200,"h":100
                }
              ],
              "links": [
                {
                  "id":"upstream-image",
                  "fromUnitId":"upstream",
                  "fromPortId":"output_image",
                  "toUnitId":"failed-art",
                  "toPortId":"input_image"
                }
              ]
            }"#,
        );
        let images = session.parent().expect("session parent").join("images");
        fs::write(images.join("upstream-square.png"), b"upstream").expect("write upstream");
        fs::write(images.join("failed-art-error.png"), b"art-error").expect("write art error");
        let expected_art =
            fs::canonicalize(images.join("failed-art-error.png")).expect("canonical art error");

        let document = HookCanvasDocument::read(&session).expect("normalize error art preview");

        assert_eq!(
            document.preview_path("failed-art"),
            Some(expected_art.as_path()),
            "when Hook stores a failed Art node's own local preview/error image, Loom must keep that preview instead of falling back to the upstream input image",
        );
    }

    #[test]
    fn error_art_preview_prefers_realistic_src_only_shape_over_upstream_input() {
        let root = test_root("error-art-realistic-shape");
        let session_dir = root.join("com.yamiyu.hook");
        let image_dir = session_dir.join("images");
        fs::create_dir_all(&image_dir).expect("create image dir");
        let upstream_path = fs::canonicalize({
            let path = image_dir.join("upstream.png");
            fs::write(&path, b"upstream").expect("write upstream");
            path
        })
        .expect("canonical upstream");
        let failed_art_path = fs::canonicalize({
            let path = image_dir.join("failed-art.png");
            fs::write(&path, b"art-error").expect("write art error");
            path
        })
        .expect("canonical art error");

        let session = write_session(
            &root,
            &format!(
                r#"{{
                  "workflowId":"hook-error-preview",
                  "stickers":[
                    {{
                      "id":"upstream",
                      "type":"sticker",
                      "src":"{upstream_src}",
                      "x":120,"y":80,"w":360,"h":210
                    }},
                    {{
                      "id":"failed-art",
                      "type":"art",
                      "artId":"neuro.official/custom-1770131241684",
                      "status":"error",
                      "src":"{failed_art_src}",
                      "x":600,"y":190,"w":190,"h":150,
                      "minified":true,
                      "opacityMini":0.9,
                      "opacityNormal":1.0,
                      "savedRect":{{"x":1508.0,"y":7.0,"w":500.0,"h":750.0}},
                      "cropOffset":{{"x":269.33333333333326,"y":384.33333333333326}},
                      "params":{{"reference":"upstream","strength":61}}
                    }}
                  ],
                  "links":[
                    {{
                      "id":"upstream-to-failed-art",
                      "fromUnitId":"upstream",
                      "fromPortId":"output",
                      "toUnitId":"failed-art",
                      "toPortId":"input"
                    }}
                  ]
                }}"#,
                upstream_src = upstream_path.to_string_lossy().replace('\\', "\\\\"),
                failed_art_src = failed_art_path.to_string_lossy().replace('\\', "\\\\"),
            ),
        );

        let document =
            HookCanvasDocument::read(&session).expect("normalize realistic error art preview");
        let failed_art = document
            .snapshot
            .nodes
            .iter()
            .find(|node| node.id == "failed-art")
            .expect("failed-art node");

        assert_eq!(failed_art.status, "error");
        assert!(failed_art.minified);
        assert_eq!(
            document.preview_path("failed-art"),
            Some(failed_art_path.as_path()),
            "a realistic Hook Art-node shape that only carries local src must still keep its own failed preview instead of falling back to the upstream input image",
        );
    }

    #[test]
    fn sticker_preview_prefers_local_baked_preview_when_annotation_state_exists() {
        let root = test_root("sticker-local-baked-preview");
        let local_baked_preview = "data:image/png;base64,LOCAL_BAKED_PREVIEW";
        let session = write_session(
            &root,
            &format!(
                r##"{{
                  "stickers": [
                    {{
                      "id":"upstream",
                      "type":"sticker",
                      "src":"images/upstream-square.png",
                      "x":0,"y":0,"w":100,"h":100
                    }},
                    {{
                      "id":"target",
                      "type":"sticker",
                      "src":"images/target-rect.png",
                      "previewSrc":"{local_baked_preview}",
                      "annotationState": {{
                        "serialCounter": 1,
                        "elements": [
                          {{
                            "id":"line-b",
                            "type":"line",
                            "zIndex": 1,
                            "points":[{{"x":20,"y":50}},{{"x":180,"y":50}}],
                            "style": {{"color":"#ffffff","width":2}}
                          }}
                        ]
                      }},
                      "x":200,"y":0,"w":200,"h":100
                    }}
                  ],
                  "links": [
                    {{
                      "id":"upstream-image",
                      "fromUnitId":"upstream",
                      "fromPortId":"output",
                      "toUnitId":"target",
                      "toPortId":"image"
                    }}
                  ]
                }}"##
            ),
        );
        let images = session.parent().expect("session parent").join("images");
        fs::write(images.join("upstream-square.png"), b"upstream").expect("write upstream");
        fs::write(images.join("target-rect.png"), b"target").expect("write target");

        let document = HookCanvasDocument::read(&session).expect("normalize local baked preview");

        assert_eq!(
            document.preview_source("target"),
            Some(&HookCanvasPreviewSource::DataUrl(local_baked_preview.to_owned())),
            "a sticker with persisted annotation state must prefer its own baked preview over the raw upstream image input",
        );
    }

    #[test]
    fn sticker_preview_prefers_local_baked_preview_through_detached_chain() {
        let root = test_root("sticker-detached-chain-preview");
        let baked_preview_b = "data:image/png;base64,LOCAL_B";
        let baked_preview_c = "data:image/png;base64,LOCAL_C";
        let session = write_session(
            &root,
            &format!(
                r##"{{
                  "stickers": [
                    {{
                      "id":"a",
                      "type":"sticker",
                      "src":"images/a.png",
                      "annotationState": {{
                        "serialCounter": 1,
                        "elements": [
                          {{
                            "id":"line-a",
                            "type":"line",
                            "zIndex": 1,
                            "points":[{{"x":50,"y":0}},{{"x":50,"y":100}}],
                            "style": {{"color":"#ffffff","width":2}}
                          }}
                        ]
                      }},
                      "x":0,"y":0,"w":100,"h":100
                    }},
                    {{
                      "id":"b",
                      "type":"sticker",
                      "src":"images/b.png",
                      "previewSrc":"{baked_preview_b}",
                      "annotationState": {{
                        "serialCounter": 2,
                        "elements": [
                          {{
                            "id":"line-b",
                            "type":"line",
                            "zIndex": 1,
                            "points":[{{"x":20,"y":50}},{{"x":180,"y":50}}],
                            "style": {{"color":"#ffffff","width":2}}
                          }}
                        ]
                      }},
                      "x":120,"y":0,"w":200,"h":100
                    }},
                    {{
                      "id":"c",
                      "type":"sticker",
                      "src":"images/c.png",
                      "previewSrc":"{baked_preview_c}",
                      "annotationState": {{
                        "serialCounter": 2,
                        "elements": [
                          {{
                            "id":"line-b",
                            "type":"line",
                            "zIndex": 1,
                            "points":[{{"x":40,"y":100}},{{"x":360,"y":100}}],
                            "style": {{"color":"#ffffff","width":4}}
                          }}
                        ]
                      }},
                      "x":360,"y":0,"w":400,"h":200
                    }}
                  ],
                  "links": [
                    {{
                      "id":"a-b",
                      "fromUnitId":"a",
                      "fromPortId":"output",
                      "toUnitId":"b",
                      "toPortId":"image"
                    }},
                    {{
                      "id":"b-c",
                      "fromUnitId":"b",
                      "fromPortId":"output",
                      "toUnitId":"c",
                      "toPortId":"image"
                    }}
                  ]
                }}"##
            ),
        );
        let images = session.parent().expect("session parent").join("images");
        fs::write(images.join("a.png"), b"a").expect("write a");
        fs::write(images.join("b.png"), b"b").expect("write b");
        fs::write(images.join("c.png"), b"c").expect("write c");

        let document =
            HookCanvasDocument::read(&session).expect("normalize detached chain preview");

        assert_eq!(
            document.preview_source("c"),
            Some(&HookCanvasPreviewSource::DataUrl(baked_preview_c.to_owned())),
            "when a downstream sticker already carries its own baked propagated preview, Loom must not recurse all the way back to ancestor raw inputs",
        );
    }

    #[test]
    fn resolves_tauri_asset_url_previews_from_the_clipboard_cache_root() {
        let root = test_root("asset-url-clipboard-cache");
        let cache = root.join("clipboard_cache");
        fs::create_dir_all(&cache).expect("create clipboard cache");
        let image_path = cache.join("Hook_capture_1.png");
        fs::write(&image_path, b"capture-bytes").expect("write cache image");

        // Point the daemon's clipboard-cache root at the isolated fixture dir.
        let previous = std::env::var_os("LOOM_HOOK_IMAGE_ROOT");
        std::env::set_var("LOOM_HOOK_IMAGE_ROOT", &cache);

        // Hook writes the image as a percent-encoded Tauri asset URL in `src`
        // and the clean absolute path in `filePath`.
        let canonical_cache = fs::canonicalize(&cache).expect("canonicalize cache");
        let canonical_image = canonical_cache.join("Hook_capture_1.png");
        let encoded = encode_asset_url_path(&canonical_image.to_string_lossy());
        let session = write_session(
            &root,
            &format!(
                r#"{{
                  "stickers": [
                    {{
                      "id":"capture",
                      "type":"sticker",
                      "src":"http://asset.localhost/{encoded}",
                      "filePath":"{file_path}",
                      "x":10,"y":20,"w":320,"h":180
                    }}
                  ],
                  "links": []
                }}"#,
                file_path = canonical_image.to_string_lossy().replace('\\', "\\\\"),
            ),
        );

        let document = HookCanvasDocument::read(&session).expect("normalize asset url preview");
        let node = &document.snapshot.nodes[0];

        assert!(
            node.preview_available,
            "asset-url preview from clipboard_cache should resolve"
        );
        assert!(node
            .preview_url
            .as_deref()
            .expect("preview url")
            .starts_with("/v1/hook-bridge/canvas/nodes/capture/preview?v="));
        assert_eq!(
            document.preview_path("capture"),
            Some(canonical_image.as_path())
        );

        if let Some(previous) = previous {
            std::env::set_var("LOOM_HOOK_IMAGE_ROOT", previous);
        } else {
            std::env::remove_var("LOOM_HOOK_IMAGE_ROOT");
        }
    }

    #[test]
    fn accepts_current_hook_workflow_sync_shape() {
        let root = test_root("nested");
        let session = write_session(
            &root,
            r#"{
              "workflowId": "hook-live",
              "nodes": [
                {
                  "id":"nested",
                  "type":"artNode",
                  "position":{"x":12,"y":24},
                  "measured":{"width":320,"height":180},
                  "data":{"artId":"neuro.official/ocr","previewSrc":"images/nested.png","status":"processing"}
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

        let bytes = fs::read(&session).expect("read workflow sync fixture");
        let root_value = serde_json::from_slice(&bytes).expect("parse workflow sync fixture");
        let document = HookCanvasDocument::from_serialized_root(&session, bytes, root_value, None);
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
    fn session_shape_ignores_workflow_containers_and_endpoint_aliases() {
        let root = test_root("hybrid-session");
        let session = write_session(
            &root,
            r#"{
              "stickers": [
                {"id":"session-node","type":"sticker","x":4,"y":8,"w":320,"h":180}
              ],
              "nodes": [
                {"id":"wire-node","type":"artNode","position":{"x":99,"y":99},"measured":{"width":1,"height":1}}
              ],
              "links": [
                {"id":"alias","source":"session-node","target":"session-node","sourceHandle":"out","targetHandle":"in"}
              ],
              "edges": [
                {"id":"wire","source":"wire-node","target":"wire-node"}
              ]
            }"#,
        );
        let bytes = fs::read(&session).expect("read hybrid session fixture");
        let root_value = serde_json::from_slice(&bytes).expect("parse hybrid session fixture");
        let document = HookCanvasDocument::from_serialized_root(&session, bytes, root_value, None);

        assert_eq!(document.snapshot.nodes.len(), 1);
        assert_eq!(document.snapshot.nodes[0].id, "session-node");
        assert_eq!(document.snapshot.nodes[0].x, 4.0);
        assert_eq!(document.snapshot.nodes[0].y, 8.0);
        assert_eq!(document.snapshot.nodes[0].width, 320.0);
        assert_eq!(document.snapshot.nodes[0].height, 180.0);
        assert!(document.snapshot.edges.is_empty());
    }

    #[test]
    fn workflow_shape_ignores_session_endpoint_aliases() {
        let root = test_root("hybrid-workflow");
        let session = write_session(
            &root,
            r#"{
              "nodes": [
                {"id":"wire-node","type":"artNode","position":{"x":12,"y":24},"measured":{"width":32,"height":48}}
              ],
              "edges": [
                {"id":"alias","fromUnitId":"wire-node","toUnitId":"wire-node","fromPortId":"out","toPortId":"in"}
              ]
            }"#,
        );
        let bytes = fs::read(&session).expect("read hybrid workflow fixture");
        let root_value = serde_json::from_slice(&bytes).expect("parse hybrid workflow fixture");
        let document = HookCanvasDocument::from_serialized_root(&session, bytes, root_value, None);

        assert_eq!(document.snapshot.nodes.len(), 1);
        assert_eq!(document.snapshot.nodes[0].id, "wire-node");
        assert_eq!(document.snapshot.nodes[0].x, 12.0);
        assert_eq!(document.snapshot.edges.len(), 0);
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
        let source = hook_canvas_source(&root);
        assert_eq!(canvas_nodes(&root, source).len(), 1);
        assert_eq!(canvas_nodes(&root, source)[0]["id"], "ready");
    }

    #[test]
    fn malformed_session_is_reported_as_json_error() {
        let root = test_root("malformed");
        let session = write_session(&root, "{not-json");

        let error = HookCanvasDocument::read(&session).expect_err("malformed session must fail");

        assert!(matches!(error, HookCanvasError::Json(_)));
    }
}
