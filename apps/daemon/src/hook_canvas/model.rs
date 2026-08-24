use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{ErrorKind, Read as _};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub(crate) const MIN_NODE_SIZE: f64 = 24.0;
pub(crate) const DEFAULT_NODE_SIZE: f64 = 96.0;
pub(crate) const MAX_PREVIEW_BYTES: u64 = 20 * 1024 * 1024;
const MAX_HOOK_SESSION_BYTES: usize = 64 * 1024 * 1024;
const MAX_HOOK_SESSION_DEPTH: usize = 32;
const MAX_PREVIEW_DATA_URL_HEADER_BYTES: usize = 128;
const MAX_PREVIEW_CHAIN_DEPTH: usize = 64;
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
    depth_limited: bool,
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
    #[error("Hook session exceeds safety limits: {0}")]
    Limit(String),
}

#[derive(Debug, Error)]
pub(crate) enum HookCanvasWorkflowExportError {
    #[error("Hook canvas node `{0}` was not found")]
    NodeNotFound(String),
    #[error("Hook canvas node `{0}` is not canonical and cannot be exported")]
    InvalidNode(String),
}
