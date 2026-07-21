# Loom Hook Canvas Thumbnail Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Loom's YAML-first Hook screenshot synchronization surface with a live, safe, real-canvas thumbnail that opens a full visual workflow canvas.

**Architecture:** The daemon reads Hook's existing session file through a new `hook_canvas` adapter, returns a normalized `HookCanvasSnapshot`, and serves node previews through a node-ID-only binary endpoint. The desktop uses pure layout helpers plus focused React components for the thumbnail and full canvas, while existing Hook Bridge events invalidate the canvas revision without forcing navigation. YAML and protocol diagnostics remain available only inside a collapsed advanced section.

**Tech Stack:** Rust 2021, serde/serde_json, Loom's custom loopback HTTP server, React 19, TypeScript, SVG/HTML canvas composition, Node test runner, PowerShell 5.1, WebView2 CDP, existing Loom release scripts.

---

## File Map

Create:

```text
apps/daemon/src/hook_canvas.rs
apps/desktop/src/services/hookCanvas.ts
apps/desktop/src/services/hookCanvas.test.ts
apps/desktop/src/components/hook/HookCanvasNode.tsx
apps/desktop/src/components/hook/HookCanvasThumbnail.tsx
apps/desktop/src/components/hook/HookCanvasView.tsx
scripts/tests/Test-HookCanvasUiContract.ps1
scripts/Inspect-LoomWebView.mjs
scripts/Invoke-LoomHookCanvasUiSmoke.ps1
docs/progress/phase-44-hook-canvas-thumbnail.md
```

Modify:

```text
apps/daemon/src/lib.rs
apps/desktop/src/App.tsx
apps/desktop/src/styles.css
apps/desktop/src/services/loomApi.ts
apps/desktop/src/services/hookBridgeWorkflowSync.ts
apps/desktop/src/services/hookBridgeWorkflowSync.test.ts
scripts/verify-release.ps1
scripts/tests/Test-GitHubActionsContract.ps1
.github/workflows/ci.yml
docs/progress/MASTER.md
README.md
```

The Hook repository and the user's real Hook session are read-only throughout implementation and verification.

---

### Task 1: Normalize Hook session data into a stable daemon canvas model

**Files:**
- Create: `apps/daemon/src/hook_canvas.rs`
- Modify: `apps/daemon/src/lib.rs:1-45`
- Test: `apps/daemon/src/hook_canvas.rs`

- [ ] **Step 1: Add failing normalization tests**

Create `apps/daemon/src/hook_canvas.rs` with a test module first. The primary fixture must match the real flat Hook session shape rather than the separate workflow broadcast shape:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "loom-hook-canvas-{name}-{}",
            std::process::id()
        ));
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
                {"id":"small","type":"sticker","src":"images/small.png","x":1792.0,"y":346.0,"w":60.0,"h":60.0},
                {"id":"art","type":"art","artId":"custom-image","src":"images/art.png","x":1576.0,"y":499.0,"w":60.0,"h":60.0}
              ],
              "links": [
                {"id":"edge-1","fromUnitId":"capture","fromPortId":"output_image","toUnitId":"art","toPortId":"input_image"}
              ]
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("normalize Hook canvas");

        assert!(document.snapshot.available);
        assert_eq!(document.snapshot.nodes.len(), 3);
        assert_eq!(document.snapshot.edges.len(), 1);
        assert_eq!(document.snapshot.bounds.x, 1576.0);
        assert_eq!(document.snapshot.bounds.y, 201.0);
        assert_eq!(document.snapshot.bounds.width, 740.0);
        assert_eq!(document.snapshot.bounds.height, 750.0);
        assert_eq!(document.snapshot.nodes[2].kind, HookCanvasNodeKind::Art);
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
                {"id":"capture","type":"sticker","x":"bad","y":-20,"w":0,"h":-1}
              ],
              "links": [
                {"id":"dangling","fromUnitId":"missing","toUnitId":"capture"}
              ]
            }"#,
        );

        let document = HookCanvasDocument::read(&session).expect("degraded Hook canvas");

        assert_eq!(document.snapshot.nodes.len(), 1);
        assert!(document.snapshot.nodes[0].width >= MIN_NODE_SIZE);
        assert!(document.snapshot.nodes[0].height >= MIN_NODE_SIZE);
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
    }

    #[test]
    fn revision_changes_when_session_content_changes() {
        let root = test_root("revision");
        let session = write_session(&root, r#"{"stickers":[],"links":[]}"#);
        let first = HookCanvasDocument::read(&session).expect("first snapshot");
        fs::write(&session, r#"{"stickers":[{"id":"one","type":"sticker"}],"links":[]}"#)
            .expect("rewrite session");
        let second = HookCanvasDocument::read(&session).expect("second snapshot");

        assert_ne!(first.snapshot.revision, second.snapshot.revision);
    }
}
```

- [ ] **Step 2: Run the tests and confirm RED**

Run:

```powershell
cargo test --locked -p loom-daemon hook_canvas -- --nocapture
```

Expected: compilation fails because `HookCanvasDocument`, snapshot types, constants, and helpers do not exist.

- [ ] **Step 3: Implement the normalized model and adapter**

Add the following public-within-crate contract:

```rust
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

pub(crate) const MIN_NODE_SIZE: f64 = 24.0;
pub(crate) const DEFAULT_NODE_SIZE: f64 = 96.0;
pub(crate) const MAX_PREVIEW_BYTES: u64 = 20 * 1024 * 1024;

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
```

Implement `HookCanvasDocument::read` using these exact rules:

1. Missing file returns `available = false`, revision `missing`, empty arrays, and no error.
2. Hash the exact session bytes with `DefaultHasher` and return a 16-character lowercase hex revision.
3. Accept the real flat fields `x`, `y`, `w`, `h`, `src`, `previewSrc`, `artId`, and `type`.
4. Also accept nested broadcast fields `position`, `measured`, and `data` so the adapter remains compatible with Hook Bridge payload shapes.
5. Require a non-empty string node ID; skip invalid nodes and add a warning.
6. Replace non-finite/missing coordinates with `0`; replace non-positive sizes with `DEFAULT_NODE_SIZE`; clamp sizes to at least `MIN_NODE_SIZE`.
7. Classify `type = art` or non-empty `artId` as `Art`, `type = sticker` as `Screenshot`, otherwise `Unknown`.
8. Use labels `截图节点`, `Art 节点`, and `未知节点`; never use the raw ID as the primary label.
9. Resolve relative image paths against the session directory and keep paths only in `preview_paths`, never in serialized nodes.
10. Set `previewUrl` to `/v1/hook-bridge/canvas/nodes/<node-id>/preview` only when the resolved file exists.
11. Keep only edges whose source and target IDs both exist.
12. Sort nodes and edges by ID before hashing/serialization-sensitive assertions.
13. Compute bounds from normalized node rectangles; use zero bounds for an empty canvas.
14. Set `updatedAt` to session modified milliseconds since Unix epoch as a decimal string.

Add:

```rust
impl HookCanvasDocument {
    pub(crate) fn preview_path(&self, node_id: &str) -> Option<&Path> {
        self.preview_paths.get(node_id).map(PathBuf::as_path)
    }

    pub(crate) fn preview_root(&self) -> Option<&Path> {
        self.preview_root.as_deref()
    }
}
```

Declare `mod hook_canvas;` in `apps/daemon/src/lib.rs`.

- [ ] **Step 4: Run focused tests to GREEN**

Run:

```powershell
cargo fmt --all
cargo test --locked -p loom-daemon hook_canvas -- --nocapture
```

Expected: all `hook_canvas` tests pass.

- [ ] **Step 5: Commit the adapter**

```powershell
git add apps/daemon/src/hook_canvas.rs apps/daemon/src/lib.rs
git commit -m "feat: normalize Hook canvas sessions"
```

---

### Task 2: Expose the canvas JSON and safe binary preview endpoints

**Files:**
- Modify: `apps/daemon/src/lib.rs:740-855`
- Modify: `apps/daemon/src/lib.rs:1546-1900`
- Modify: `apps/daemon/src/lib.rs:8106-8135`
- Test: `apps/daemon/src/lib.rs`

- [ ] **Step 1: Add failing daemon endpoint tests**

Add tests under the existing daemon test module:

```rust
#[test]
fn daemon_exposes_hook_canvas_snapshot_contract() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let appdata = unique_temp_dir("hook-canvas-appdata");
    let session_dir = appdata.join("com.vmjcv.arthook-next");
    let images = session_dir.join("images");
    fs::create_dir_all(&images).expect("create session images");
    fs::write(
        session_dir.join("session.json"),
        r#"{"stickers":[{"id":"capture","type":"sticker","src":"images/capture.png","x":20,"y":30,"w":320,"h":180}],"links":[]}"#,
    )
    .expect("write Hook session");
    fs::write(images.join("capture.png"), test_png_bytes()).expect("write preview");
    let previous = std::env::var("APPDATA").ok();
    std::env::set_var("APPDATA", &appdata);

    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    let address = daemon.local_addr().expect("address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve"));

    let canvas = http_json_get(address.port(), "/v1/hook-bridge/canvas");
    assert_eq!(canvas["available"], true);
    assert_eq!(canvas["nodes"][0]["id"], "capture");
    assert_eq!(canvas["nodes"][0]["kind"], "screenshot");
    assert_eq!(
        canvas["nodes"][0]["previewUrl"],
        "/v1/hook-bridge/canvas/nodes/capture/preview"
    );

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("join");
    restore_env("APPDATA", previous);
    fs::remove_dir_all(appdata).expect("cleanup");
}

#[test]
fn daemon_serves_only_registered_hook_canvas_preview_images() {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let appdata = unique_temp_dir("hook-canvas-preview-appdata");
    let session_dir = appdata.join("com.vmjcv.arthook-next");
    let images = session_dir.join("images");
    fs::create_dir_all(&images).expect("create session images");
    let png = test_png_bytes();
    fs::write(images.join("capture.png"), &png).expect("write registered preview");
    fs::write(appdata.join("outside.png"), &png).expect("write outside preview");
    fs::write(
        session_dir.join("session.json"),
        r#"{
          "stickers": [
            {"id":"capture","type":"sticker","src":"images/capture.png","x":20,"y":30,"w":320,"h":180},
            {"id":"escape","type":"sticker","src":"../outside.png","x":400,"y":30,"w":320,"h":180}
          ],
          "links": []
        }"#,
    )
    .expect("write Hook session");
    let previous = std::env::var("APPDATA").ok();
    std::env::set_var("APPDATA", &appdata);

    let daemon = LoomDaemon::bind(DaemonConfig::localhost(0)).expect("bind daemon");
    let address = daemon.local_addr().expect("address");
    let (shutdown_tx, shutdown_rx) = mpsc::channel();
    let server = thread::spawn(move || daemon.serve_until(shutdown_rx).expect("serve"));

    let registered = http_get_bytes(
        address.port(),
        "/v1/hook-bridge/canvas/nodes/capture/preview",
    );
    let (registered_headers, registered_body) = split_http_bytes(&registered);
    assert!(registered_headers.starts_with("HTTP/1.1 200 OK"));
    assert!(registered_headers.contains("Content-Type: image/png"));
    assert_eq!(registered_body, png.as_slice());

    let unknown = http_get_bytes(
        address.port(),
        "/v1/hook-bridge/canvas/nodes/unknown/preview",
    );
    let (unknown_headers, unknown_body) = split_http_bytes(&unknown);
    assert!(unknown_headers.starts_with("HTTP/1.1 404 Not Found"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(unknown_body)
            .expect("unknown preview json")["error"]["code"],
        "preview_not_found"
    );

    let escaped = http_get_bytes(
        address.port(),
        "/v1/hook-bridge/canvas/nodes/escape/preview",
    );
    let (escaped_headers, escaped_body) = split_http_bytes(&escaped);
    assert!(escaped_headers.starts_with("HTTP/1.1 404 Not Found"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(escaped_body)
            .expect("escaped preview json")["error"]["code"],
        "preview_not_found"
    );

    shutdown_tx.send(()).expect("shutdown");
    server.join().expect("join");
    restore_env("APPDATA", previous);
    fs::remove_dir_all(appdata).expect("cleanup");
}
```

Add a byte-oriented test helper:

```rust
fn test_png_bytes() -> Vec<u8> {
    let image = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(1, 1, vec![10, 20, 30, 255])
        .expect("test png image");
    let mut png = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut png, ImageFormat::Png)
        .expect("encode test png");
    png.into_inner()
}

fn http_get_bytes(port: u16, path: &str) -> Vec<u8> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect daemon");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set timeout");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .expect("write request");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("read response");
    response
}

fn split_http_bytes(response: &[u8]) -> (&str, &[u8]) {
    let separator = b"\r\n\r\n";
    let index = response
        .windows(separator.len())
        .position(|window| window == separator)
        .expect("HTTP header separator");
    let headers = std::str::from_utf8(&response[..index]).expect("HTTP headers are UTF-8");
    (headers, &response[index + separator.len()..])
}
```

- [ ] **Step 2: Run endpoint tests and confirm RED**

```powershell
cargo test --locked -p loom-daemon daemon_exposes_hook_canvas -- --nocapture
cargo test --locked -p loom-daemon daemon_serves_only_registered_hook_canvas -- --nocapture
```

Expected: both fail because the routes and binary response path do not exist.

- [ ] **Step 3: Add the JSON route and help contract**

Add to the daemon help text:

```text
GET  /v1/hook-bridge/canvas
GET  /v1/hook-bridge/canvas/nodes/{nodeId}/preview
```

Add the route arm next to the existing Hook Bridge session route:

```rust
("GET", "/v1/hook-bridge/canvas") => hook_canvas_snapshot(),
```

Implement:

```rust
fn hook_canvas_snapshot() -> Result<(u16, String)> {
    let document = hook_canvas::HookCanvasDocument::read(&arthook_session_path())?;
    Ok((200, serde_json::to_string(&document.snapshot)?))
}
```

Map malformed-session errors to a structured `200` degraded snapshot only when a previous UI snapshot can remain useful; the endpoint itself must return `500 hook_canvas_error` for unreadable/invalid current storage so the desktop can retain its last good in-memory snapshot.

- [ ] **Step 4: Add a binary route response without rewriting all text routes**

Keep the existing `route(...) -> Result<(u16, String)>` contract. Add an outer response enum only around `route_request`:

```rust
enum RouteResponse {
    Text { status: u16, body: String },
    Binary {
        status: u16,
        content_type: &'static str,
        body: Vec<u8>,
    },
}

fn route_request(runtime: &DaemonRuntime, request: &ParsedHttpRequest) -> RouteResponse {
    let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if let Some(node_id) = hook_canvas_preview_node_id(&request.method, &request.path) {
            if let Some(token) = runtime.auth_token.as_deref() {
                if !request.has_bearer(token) {
                    let (status, body) = structured_error(
                        401,
                        json!({"code":"unauthorized","message":"missing or invalid Loom bearer token"}),
                    )?;
                    return Ok(RouteResponse::Text { status, body });
                }
            }
            return hook_canvas_preview_response(node_id);
        }
        route_with_runtime(runtime, request)
            .map(|(status, body)| RouteResponse::Text { status, body })
    }));

    match response {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            eprintln!("loom request routing failed: {error:#}");
            let (status, body) = request_worker_failed_response();
            RouteResponse::Text { status, body }
        }
        Err(_) => {
            let (status, body) = request_worker_failed_response();
            RouteResponse::Text { status, body }
        }
    }
}
```

Add `write_route_response_safely` and retain the existing text-only `write_response_safely` for queue/read rejection paths:

```rust
fn write_route_response_safely(mut stream: TcpStream, response: RouteResponse) {
    let result = match response {
        RouteResponse::Text { status, body } => write_response(&mut stream, status, &body),
        RouteResponse::Binary { status, content_type, body } => {
            write_binary_response(&mut stream, status, content_type, &body)
        }
    };
    if let Err(error) = result {
        eprintln!("loom response write failed: {error:#}");
    }
}
```

Update only `handle_parsed_request` and `handle_request_job` to use this function.

- [ ] **Step 5: Implement safe preview resolution**

Add:

```rust
fn hook_canvas_preview_node_id<'a>(method: &str, path: &'a str) -> Option<&'a str> {
    if method != "GET" {
        return None;
    }
    path_id_with_suffix(
        path.split('?').next().unwrap_or(path),
        "/v1/hook-bridge/canvas/nodes/",
        "/preview",
    )
}
```

`hook_canvas_preview_response` must:

1. Reload the current session through `HookCanvasDocument`.
2. Resolve the node ID through `preview_path` only.
3. Canonicalize both the preview file and `preview_root`.
4. Require the file to remain under `preview_root`.
5. Reject files larger than `MAX_PREVIEW_BYTES` with `413`.
6. Detect PNG (`89 50 4e 47`), JPEG (`ff d8 ff`), and WebP (`RIFF....WEBP`) by bytes.
7. Return `404 preview_not_found` for unknown/missing nodes.
8. Return `415 unsupported_preview_type` for unsupported content. Add `415 Unsupported Media Type` to the response reason mapping.

Add preview GETs to `RequestConcurrencyClass::Concurrent` because they are read-only and must not block serialized mutation routes.

- [ ] **Step 6: Run daemon tests to GREEN**

```powershell
cargo fmt --all
cargo test --locked -p loom-daemon hook_canvas -- --nocapture
cargo test --locked -p loom-daemon daemon_exposes_hook_canvas -- --nocapture
cargo test --locked -p loom-daemon daemon_serves_only_registered_hook_canvas -- --nocapture
cargo test --locked -p loom-daemon daemon_reports_hook_bridge_status_contract -- --nocapture
```

Expected: all pass; the existing Hook session endpoint remains unchanged.

- [ ] **Step 7: Commit the API**

```powershell
git add apps/daemon/src/lib.rs apps/daemon/src/hook_canvas.rs
git commit -m "feat: expose Hook canvas preview API"
```

---

### Task 3: Add typed desktop canvas data and deterministic layout math

**Files:**
- Create: `apps/desktop/src/services/hookCanvas.ts`
- Create: `apps/desktop/src/services/hookCanvas.test.ts`
- Modify: `apps/desktop/src/services/loomApi.ts:560-595`

- [ ] **Step 1: Write failing service tests**

Create `hookCanvas.test.ts`:

```ts
import assert from "node:assert/strict";
import test from "node:test";

import {
  edgeEndpoints,
  fitHookCanvas,
  keepNewestHookCanvasSnapshot,
  retainHookCanvasSelection,
  resolveHookCanvasPreviewUrl,
  type HookCanvasSnapshot,
} from "./hookCanvas.ts";

const snapshot: HookCanvasSnapshot = {
  available: true,
  revision: "rev-1",
  updatedAt: "1",
  workflowId: "hook-live",
  bounds: { x: 100, y: 200, width: 500, height: 250 },
  nodes: [
    {
      id: "capture",
      kind: "screenshot",
      label: "截图节点",
      artId: null,
      x: 100,
      y: 200,
      width: 500,
      height: 250,
      previewAvailable: true,
      previewUrl: "/v1/hook-bridge/canvas/nodes/capture/preview",
      status: "ready",
    },
  ],
  edges: [],
  warnings: [],
};

test("fits Hook nodes into a stable virtual viewport", () => {
  const layout = fitHookCanvas(snapshot, { width: 1000, height: 620, padding: 32, minimumNodeSize: 24 });
  assert.equal(layout.nodes[0].x, 32);
  assert.equal(layout.nodes[0].y, 76);
  assert.equal(layout.nodes[0].width, 936);
  assert.equal(layout.nodes[0].height, 468);
});

test("keeps the previous object when revision is unchanged", () => {
  assert.equal(keepNewestHookCanvasSnapshot(snapshot, { ...snapshot }), snapshot);
});

test("replaces the previous object when revision changes", () => {
  const next = { ...snapshot, revision: "rev-2" };
  assert.equal(keepNewestHookCanvasSnapshot(snapshot, next), next);
});

test("resolves preview paths against the daemon origin", () => {
  assert.equal(
    resolveHookCanvasPreviewUrl("http://127.0.0.1:8765/", snapshot.nodes[0]),
    "http://127.0.0.1:8765/v1/hook-bridge/canvas/nodes/capture/preview",
  );
});

test("does not resolve unavailable previews", () => {
  assert.equal(
    resolveHookCanvasPreviewUrl("http://127.0.0.1:8765/", {
      ...snapshot.nodes[0],
      previewAvailable: false,
    }),
    null,
  );
});

test("resolves edge endpoints from fitted node centers", () => {
  const target = {
    ...snapshot.nodes[0],
    id: "art",
    x: 700,
    width: 100,
  };
  const graph = {
    ...snapshot,
    bounds: { x: 100, y: 200, width: 700, height: 250 },
    nodes: [snapshot.nodes[0], target],
    edges: [{
      id: "edge",
      sourceNodeId: "capture",
      sourcePortId: null,
      targetNodeId: "art",
      targetPortId: null,
    }],
  };
  const layout = fitHookCanvas(graph, {
    width: 1000,
    height: 620,
    padding: 32,
    minimumNodeSize: 24,
  });
  const endpoints = edgeEndpoints(layout, graph.edges[0]);
  assert.ok(endpoints);
  assert.equal(Number.isFinite(endpoints.source.x), true);
  assert.equal(Number.isFinite(endpoints.target.x), true);
  assert.equal(endpoints.source.x < endpoints.target.x, true);
});

test("missing edge nodes and stale selections degrade to null", () => {
  const layout = fitHookCanvas(snapshot, {
    width: 1000,
    height: 620,
    padding: 32,
    minimumNodeSize: 24,
  });
  assert.equal(
    edgeEndpoints(layout, {
      id: "missing",
      sourceNodeId: "capture",
      sourcePortId: null,
      targetNodeId: "missing",
      targetPortId: null,
    }),
    null,
  );
  assert.equal(retainHookCanvasSelection("missing", snapshot), null);
  assert.equal(retainHookCanvasSelection("capture", snapshot), "capture");
});

test("empty and degenerate bounds never produce NaN", () => {
  const empty = fitHookCanvas(
    { ...snapshot, bounds: { x: 0, y: 0, width: 0, height: 0 }, nodes: [] },
    { width: 1000, height: 620, padding: 32, minimumNodeSize: 24 },
  );
  assert.deepEqual(empty.nodes, []);
  assert.equal(Number.isFinite(empty.scale), true);
});

test("negative coordinates preserve relative placement and minimum size", () => {
  const graph = {
    ...snapshot,
    bounds: { x: -200, y: -100, width: 400, height: 200 },
    nodes: [{ ...snapshot.nodes[0], x: -200, y: -100, width: 1, height: 1 }],
  };
  const layout = fitHookCanvas(graph, {
    width: 1000,
    height: 620,
    padding: 32,
    minimumNodeSize: 24,
  });
  assert.equal(layout.nodes[0].x, 32);
  assert.equal(layout.nodes[0].y, 76);
  assert.equal(layout.nodes[0].width, 24);
  assert.equal(layout.nodes[0].height, 24);
});
```

- [ ] **Step 2: Run the test and confirm RED**

```powershell
node --test apps/desktop/src/services/hookCanvas.test.ts
```

Expected: FAIL because `hookCanvas.ts` does not exist.

- [ ] **Step 3: Export a generic daemon GET helper**

In `loomApi.ts`, add:

```ts
export async function getLoomDaemonJson<T>(baseUrl: string, path: string): Promise<T> {
  return await getJson<T>(baseUrl, path);
}
```

Do not duplicate Tauri/browser fallback logic in `hookCanvas.ts`.

- [ ] **Step 4: Implement the typed service and layout helper**

Create the exact public contract in `hookCanvas.ts`:

```ts
import { getLoomDaemonJson } from "./loomApi.ts";

export type HookCanvasNodeKind = "screenshot" | "art" | "unknown";
export type HookCanvasNodeStatus = "ready" | "processing" | "error" | "unknown";

export interface HookCanvasBounds { x: number; y: number; width: number; height: number }
export interface HookCanvasNode {
  id: string;
  kind: HookCanvasNodeKind;
  label: string;
  artId: string | null;
  x: number;
  y: number;
  width: number;
  height: number;
  previewAvailable: boolean;
  previewUrl: string | null;
  status: HookCanvasNodeStatus;
}
export interface HookCanvasEdge {
  id: string;
  sourceNodeId: string;
  sourcePortId: string | null;
  targetNodeId: string;
  targetPortId: string | null;
}
export interface HookCanvasSnapshot {
  available: boolean;
  revision: string;
  updatedAt: string | null;
  workflowId: string | null;
  bounds: HookCanvasBounds;
  nodes: HookCanvasNode[];
  edges: HookCanvasEdge[];
  warnings: string[];
}
export interface HookCanvasLayoutOptions {
  width: number;
  height: number;
  padding: number;
  minimumNodeSize: number;
}
export interface HookCanvasLayoutNode extends HookCanvasNode {
  x: number;
  y: number;
  width: number;
  height: number;
}
export interface HookCanvasLayout {
  width: number;
  height: number;
  scale: number;
  nodes: HookCanvasLayoutNode[];
}
export interface HookCanvasPoint { x: number; y: number }
export interface HookCanvasEdgeEndpoints {
  source: HookCanvasPoint;
  target: HookCanvasPoint;
}

export async function readHookCanvasSnapshot(baseUrl: string): Promise<HookCanvasSnapshot> {
  return await getLoomDaemonJson<HookCanvasSnapshot>(baseUrl, "/v1/hook-bridge/canvas");
}

export function keepNewestHookCanvasSnapshot(
  previous: HookCanvasSnapshot | null,
  next: HookCanvasSnapshot,
): HookCanvasSnapshot {
  return previous?.revision === next.revision ? previous : next;
}

export function resolveHookCanvasPreviewUrl(
  baseUrl: string,
  node: HookCanvasNode,
): string | null {
  if (!node.previewAvailable || !node.previewUrl) {
    return null;
  }
  return new URL(node.previewUrl, baseUrl.endsWith("/") ? baseUrl : `${baseUrl}/`).toString();
}

export function retainHookCanvasSelection(
  selectedNodeId: string | null,
  snapshot: HookCanvasSnapshot,
): string | null {
  return selectedNodeId && snapshot.nodes.some((node) => node.id === selectedNodeId)
    ? selectedNodeId
    : null;
}
```

Implement `fitHookCanvas` with one uniform scale:

```ts
export function fitHookCanvas(
  snapshot: HookCanvasSnapshot,
  options: HookCanvasLayoutOptions,
): HookCanvasLayout {
  const usableWidth = Math.max(1, options.width - options.padding * 2);
  const usableHeight = Math.max(1, options.height - options.padding * 2);
  const sourceWidth = Math.max(1, snapshot.bounds.width);
  const sourceHeight = Math.max(1, snapshot.bounds.height);
  const scale = Math.min(usableWidth / sourceWidth, usableHeight / sourceHeight);
  const contentWidth = sourceWidth * scale;
  const contentHeight = sourceHeight * scale;
  const offsetX = (options.width - contentWidth) / 2;
  const offsetY = (options.height - contentHeight) / 2;

  return {
    width: options.width,
    height: options.height,
    scale,
    nodes: snapshot.nodes.map((node) => ({
      ...node,
      x: offsetX + (node.x - snapshot.bounds.x) * scale,
      y: offsetY + (node.y - snapshot.bounds.y) * scale,
      width: Math.max(options.minimumNodeSize, node.width * scale),
      height: Math.max(options.minimumNodeSize, node.height * scale),
    })),
  };
}
```

Add the edge helper using fitted node centers:

```ts
export function edgeEndpoints(
  layout: HookCanvasLayout,
  edge: HookCanvasEdge,
): HookCanvasEdgeEndpoints | null {
  const source = layout.nodes.find((node) => node.id === edge.sourceNodeId);
  const target = layout.nodes.find((node) => node.id === edge.targetNodeId);
  if (!source || !target) {
    return null;
  }
  return {
    source: { x: source.x + source.width / 2, y: source.y + source.height / 2 },
    target: { x: target.x + target.width / 2, y: target.y + target.height / 2 },
  };
}
```

The tests above cover negative coordinates, minimum node size, missing edge endpoints, preview degradation, stale selection, and revision replacement.

- [ ] **Step 5: Run frontend service tests and typecheck**

```powershell
node --test apps/desktop/src/services/hookCanvas.test.ts
npm --prefix apps/desktop test
npm --prefix apps/desktop run typecheck
```

Expected: all pass.

- [ ] **Step 6: Commit the service layer**

```powershell
git add apps/desktop/src/services/hookCanvas.ts apps/desktop/src/services/hookCanvas.test.ts apps/desktop/src/services/loomApi.ts
git commit -m "feat: add Hook canvas layout service"
```

---

### Task 4: Change Hook Bridge refresh semantics to invalidate without forced navigation

**Files:**
- Modify: `apps/desktop/src/services/hookBridgeWorkflowSync.ts`
- Modify: `apps/desktop/src/services/hookBridgeWorkflowSync.test.ts`
- Modify: `apps/desktop/src/App.tsx:4168-4190`

- [ ] **Step 1: Rewrite tests for the approved event behavior**

Update the options contract to include `invalidateHookCanvas`. Add tests that assert:

1. `art_hook/instantiate` refreshes state, invalidates the canvas, and opens the Hook workflow.
2. Matching `art_loom/workflow_updated` refreshes and invalidates but does not navigate.
3. `art_loom/arts_updated` refreshes and invalidates but does not navigate.
4. Multiple events inside the debounce window produce one refresh/invalidation.
5. An unrelated workflow ID does nothing.
6. `dispose()` cancels the pending timer.

Use a short injectable debounce for deterministic tests:

```ts
const handle = startHookBridgeWorkflowSync({
  client,
  refresh: async () => events.push("refresh"),
  invalidateHookCanvas: () => events.push("invalidate"),
  openHookWorkflow: () => events.push("open"),
  debounceMs: 1,
});
```

- [ ] **Step 2: Run tests and confirm RED**

```powershell
node --test apps/desktop/src/services/hookBridgeWorkflowSync.test.ts
```

Expected: FAIL because current workflow and Art update events call `openHookWorkflow` every time and are not debounced.

- [ ] **Step 3: Implement event invalidation and debounce**

Use this options shape:

```ts
export interface HookBridgeWorkflowSyncOptions {
  client?: HookBridgeBrowserClient;
  refresh: () => Promise<unknown> | unknown;
  invalidateHookCanvas: () => void;
  openHookWorkflow: () => void;
  debounceMs?: number;
}
```

Implement one pending timer. The scheduled callback must call `refresh`, then `invalidateHookCanvas`. `art_hook/instantiate` additionally calls `openHookWorkflow` once after the scheduled refresh completes. Workflow and Art updates never navigate by themselves.

Do not subscribe to a nonexistent overwrite-specific method; overwrite already emits `art_loom/workflow_updated`.

- [ ] **Step 4: Add retained canvas state to App**

Add state and a refresh callback near the existing Loom snapshot state:

```ts
const [hookCanvas, setHookCanvas] = useState<HookCanvasSnapshot | null>(null);
const [hookCanvasLoading, setHookCanvasLoading] = useState(false);
const [hookCanvasError, setHookCanvasError] = useState<string | null>(null);
const [hookCanvasRefreshVersion, setHookCanvasRefreshVersion] = useState(0);

const refreshHookCanvas = useCallback(async (baseUrl = snapshot.baseUrl) => {
  setHookCanvasLoading(true);
  try {
    const next = await readHookCanvasSnapshot(baseUrl);
    setHookCanvas((previous) => keepNewestHookCanvasSnapshot(previous, next));
    setHookCanvasError(null);
  } catch (error) {
    setHookCanvasError(error instanceof Error ? error.message : "无法读取 Hook 画布。");
  } finally {
    setHookCanvasLoading(false);
  }
}, [snapshot.baseUrl]);
```

Increment `hookCanvasRefreshVersion` from `invalidateHookCanvas`, and have one effect call `refreshHookCanvas` when the version or daemon base URL changes. Keep the previous `hookCanvas` object when refresh fails so the UI can show an offline snapshot.

- [ ] **Step 5: Run tests and typecheck**

```powershell
npm --prefix apps/desktop test
npm --prefix apps/desktop run typecheck
```

Expected: all pass.

- [ ] **Step 6: Commit event semantics**

```powershell
git add apps/desktop/src/services/hookBridgeWorkflowSync.ts apps/desktop/src/services/hookBridgeWorkflowSync.test.ts apps/desktop/src/App.tsx
git commit -m "fix: refresh Hook canvas without forced navigation"
```

---

### Task 5: Render the real thumbnail and full visual Hook canvas

**Files:**
- Create: `apps/desktop/src/components/hook/HookCanvasNode.tsx`
- Create: `apps/desktop/src/components/hook/HookCanvasThumbnail.tsx`
- Create: `apps/desktop/src/components/hook/HookCanvasView.tsx`
- Modify: `apps/desktop/src/App.tsx:351-370`
- Modify: `apps/desktop/src/App.tsx:468-1100`
- Modify: `apps/desktop/src/App.tsx:3328-3525`
- Modify: `apps/desktop/src/App.tsx:4110-4330`
- Modify: `apps/desktop/src/styles.css:540-770`
- Modify: `apps/desktop/src/styles.css:820-845`
- Test: `scripts/tests/Test-HookCanvasUiContract.ps1`

- [ ] **Step 1: Add a failing static UI contract**

Create `scripts/tests/Test-HookCanvasUiContract.ps1` and assert that the planned component files and stable test IDs exist:

```powershell
$ErrorActionPreference = "Stop"

function Assert-True {
    param([bool]$Condition, [string]$Message)
    if (-not $Condition) { throw $Message }
}

function Assert-PathExists {
    param([string]$Path)
    Assert-True (Test-Path -LiteralPath $Path -PathType Leaf) "Missing required file: $Path"
}

function Assert-Contains {
    param([string]$Needle, [string]$Haystack, [string]$Message)
    Assert-True $Haystack.Contains($Needle) $Message
}

function Assert-NotContains {
    param([string]$Needle, [string]$Haystack, [string]$Message)
    Assert-True (-not $Haystack.Contains($Needle)) $Message
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))
$appPath = Join-Path $repoRoot "apps\desktop\src\App.tsx"
$thumbnailPath = Join-Path $repoRoot "apps\desktop\src\components\hook\HookCanvasThumbnail.tsx"
$viewPath = Join-Path $repoRoot "apps\desktop\src\components\hook\HookCanvasView.tsx"
$nodePath = Join-Path $repoRoot "apps\desktop\src\components\hook\HookCanvasNode.tsx"

Assert-PathExists $thumbnailPath
Assert-PathExists $viewPath
Assert-PathExists $nodePath

$app = Get-Content -Raw -Encoding UTF8 $appPath
$thumbnail = Get-Content -Raw -Encoding UTF8 $thumbnailPath
$view = Get-Content -Raw -Encoding UTF8 $viewPath

Assert-Contains 'data-testid="nav-hook-bridge"' $app "Hook navigation needs a stable UI smoke target."
Assert-Contains 'data-testid="hook-canvas-thumbnail"' $thumbnail "Screenshot Sync must render a real Hook canvas thumbnail."
Assert-Contains 'data-testid="hook-canvas-node"' $node "Hook canvas nodes need stable smoke targets."
Assert-Contains 'data-testid="hook-canvas-view"' $view "Hook workflow must render a full visual canvas."
Assert-Contains '打开可视化工作流' $thumbnail "Thumbnail must expose the visual workflow entry."
```

- [ ] **Step 2: Run the contract and confirm RED**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-HookCanvasUiContract.ps1
```

Expected: FAIL because the component files do not exist.

- [ ] **Step 3: Implement the shared node component**

`HookCanvasNode.tsx` must receive a precomputed layout rectangle and render a stable absolute-positioned node:

```tsx
interface HookCanvasNodeProps {
  node: HookCanvasLayoutNode;
  baseUrl: string;
  selected: boolean;
  interactive: boolean;
  onSelect?: (nodeId: string) => void;
}

export function HookCanvasNode({ node, baseUrl, selected, interactive, onSelect }: HookCanvasNodeProps) {
  const previewUrl = resolveHookCanvasPreviewUrl(baseUrl, node);
  const className = [
    "hook-canvas-node",
    `hook-canvas-node--${node.kind}`,
    selected ? "hook-canvas-node--selected" : "",
  ].filter(Boolean).join(" ");

  return (
    <button
      className={className}
      data-testid="hook-canvas-node"
      data-node-id={node.id}
      type="button"
      disabled={!interactive}
      onClick={() => onSelect?.(node.id)}
      style={{
        left: `${(node.x / 1000) * 100}%`,
        top: `${(node.y / 620) * 100}%`,
        width: `${(node.width / 1000) * 100}%`,
        height: `${(node.height / 620) * 100}%`,
      }}
    >
      {previewUrl ? <img src={previewUrl} alt="" /> : <span className="hook-canvas-node__placeholder">预览不可用</span>}
      <span className="hook-canvas-node__label"><strong>{node.label}</strong><small>{node.status}</small></span>
    </button>
  );
}
```

Do not expose `artId`, raw filesystem paths, or node IDs as the primary visible label.

- [ ] **Step 4: Implement the thumbnail**

`HookCanvasThumbnail` must:

- use a fixed virtual viewport `1000 x 620` and stable CSS `aspect-ratio`;
- call `fitHookCanvas` once per snapshot;
- draw edges in an SVG using `edgeEndpoints`;
- show live/offline/sync-paused status;
- show 3 metrics: node count, edge count, connection state;
- show a visual empty state instead of YAML;
- call `onOpen(nodeId?)` from the canvas or node;
- include `data-testid="hook-canvas-thumbnail"` and `data-revision`.

Use this public interface:

```tsx
interface HookCanvasThumbnailProps {
  snapshot: HookCanvasSnapshot | null;
  baseUrl: string;
  loading: boolean;
  error: string | null;
  hookConnected: boolean;
  onRefresh: () => void;
  onOpen: (nodeId?: string) => void;
}
```

- [ ] **Step 5: Implement the full canvas**

`HookCanvasView` uses the same layout data at a larger aspect ratio and adds selection:

```tsx
interface HookCanvasViewProps {
  snapshot: HookCanvasSnapshot;
  baseUrl: string;
  selectedNodeId: string | null;
  onSelectNode: (nodeId: string) => void;
  onRunWorkflow: () => void;
}
```

Render the selected-node inspector beside the canvas. If the selected node also exists in `workflowGraph`, connect selection to the existing node draft/editor. If it does not exist, show read-only canvas metadata and keep execution disabled for that node.

- [ ] **Step 6: Integrate navigation and retained node selection**

Replace the string-only workflow request with:

```ts
interface WorkflowOpenRequest {
  workflowId: string;
  selectedNodeId?: string;
}
```

Update `openHookLiveWorkflow`, `openWorkflowInStudio`, and `WorkflowStudioPanel` so clicking a thumbnail node opens `hook-live` and selects that node after the workflow bundle loads.

Pass the retained `hookCanvas`, loading/error state, and refresh callback into `HookBridgePanel`. Pass `hookCanvas` into `WorkflowStudioPanel` and render `HookCanvasView` before the generic graph when the open workflow is Hook live.

- [ ] **Step 7: Replace the current Hook flow strip**

In `HookBridgePanel`, remove the fake three-chip flow strip as the primary representation. The first major surface after start/stop controls must be `HookCanvasThumbnail`.

Keep `启动截图同步`, `停止截图同步`, manual refresh, and bridge status. Move protocol methods and raw session details to Task 6's advanced section.

- [ ] **Step 8: Add responsive visual styling**

Add focused classes for:

```text
.hook-canvas-thumbnail
.hook-canvas-surface
.hook-canvas-grid
.hook-canvas-edges
.hook-canvas-node
.hook-canvas-node--screenshot
.hook-canvas-node--art
.hook-canvas-node--selected
.hook-canvas-node__placeholder
.hook-canvas-node__label
.hook-canvas-status
.hook-canvas-metrics
.hook-canvas-workspace
.hook-canvas-inspector
```

Requirements:

- stable `aspect-ratio` and no layout shift during preview load;
- nodes use `position: absolute` and never resize the canvas;
- edge SVG is non-interactive and covers the full surface;
- visible focus styles for interactive nodes;
- desktop workspace uses canvas plus inspector columns;
- mobile uses one column and keeps all labels inside their containers;
- no nested decorative cards;
- no oversized text inside compact panels.

- [ ] **Step 9: Run UI contract, tests, typecheck, and build**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-HookCanvasUiContract.ps1
npm --prefix apps/desktop test
npm --prefix apps/desktop run typecheck
npm --prefix apps/desktop run build
```

Expected: all pass.

- [ ] **Step 10: Commit the visual canvas**

```powershell
git add apps/desktop/src/components/hook apps/desktop/src/App.tsx apps/desktop/src/styles.css scripts/tests/Test-HookCanvasUiContract.ps1
git commit -m "feat: render Hook canvas thumbnail and visual view"
```

---

### Task 6: Hide YAML and compatibility details behind an advanced disclosure

**Files:**
- Modify: `apps/desktop/src/App.tsx:120-130`
- Modify: `apps/desktop/src/App.tsx:468-1100`
- Modify: `apps/desktop/src/App.tsx:3290-3650`
- Modify: `apps/desktop/src/styles.css`
- Modify: `scripts/tests/Test-HookCanvasUiContract.ps1`

- [ ] **Step 1: Add failing copy and disclosure assertions**

Extend `Test-HookCanvasUiContract.ps1`:

```powershell
Assert-Contains 'data-testid="advanced-technical-information"' $app "Technical workflow formats must be in an explicit disclosure."
Assert-Contains '保存工作流' $app "Normal save action must not require YAML wording."
Assert-Contains '打开工作流' $app "Normal load action must not require YAML wording."
Assert-NotContains 'eyebrow: "YAML 存储"' $app "Navigation must not advertise YAML to normal users."
Assert-NotContains '>加载 YAML<' $app "Saved workflow action must use visual language."
```

Also assert that protocol chips and `sessionPath` occur after the advanced disclosure marker in the Hook panel source block.

- [ ] **Step 2: Run the contract and confirm RED**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-HookCanvasUiContract.ps1
```

Expected: FAIL on current YAML-first copy and missing disclosure.

- [ ] **Step 3: Reorder Workflow Studio around the visual canvas**

The normal order becomes:

```text
Visual canvas
Selected node properties
Save/run/tool actions
Saved workflows
Advanced Technical Information
```

Rename normal actions:

```text
保存工作流 YAML -> 保存工作流
加载 YAML -> 打开工作流
工作流 YAML -> YAML 源定义 (advanced only)
查看工作流 JSON -> advanced only
YAML 存储 -> 可视化工作流
```

The generic workflow graph remains available for non-Hook workflows. Move the YAML textarea, cURL raw request importer, raw JSON previews, protocol method chips, `sessionPath`, IPC probes, package checks, and shared-memory probes into:

```tsx
<details className="advanced-technical-information" data-testid="advanced-technical-information">
  <summary>高级技术信息</summary>
  <div className="advanced-technical-information__body">...</div>
</details>
```

The `details` element must not have the `open` attribute by default.

- [ ] **Step 4: Preserve advanced workflows**

Verify that expanding the disclosure still allows:

- YAML edit/import/export;
- cURL smart import;
- workflow JSON inspection;
- ArtHook raw session inspection;
- IPC/shared-memory/package compatibility probes.

No daemon API is removed.

- [ ] **Step 5: Run frontend and contract validation**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-HookCanvasUiContract.ps1
npm --prefix apps/desktop test
npm --prefix apps/desktop run typecheck
npm --prefix apps/desktop run build
```

Expected: all pass.

- [ ] **Step 6: Commit the UX cleanup**

```powershell
git add apps/desktop/src/App.tsx apps/desktop/src/styles.css scripts/tests/Test-HookCanvasUiContract.ps1
git commit -m "refactor: hide workflow technical formats by default"
```

---

### Task 7: Add isolated WebView2 UI smoke and release integration

**Files:**
- Create: `scripts/Inspect-LoomWebView.mjs`
- Create: `scripts/Invoke-LoomHookCanvasUiSmoke.ps1`
- Modify: `scripts/verify-release.ps1:264-274`
- Modify: `.github/workflows/ci.yml:36-47`
- Modify: `scripts/tests/Test-GitHubActionsContract.ps1`
- Test: `scripts/tests/Test-HookCanvasUiContract.ps1`

- [ ] **Step 1: Add contract assertions for the smoke tooling**

Extend `Test-HookCanvasUiContract.ps1` to require both scripts and stable safety markers:

```powershell
Assert-PathExists (Join-Path $repoRoot "scripts\Inspect-LoomWebView.mjs")
Assert-PathExists (Join-Path $repoRoot "scripts\Invoke-LoomHookCanvasUiSmoke.ps1")
Assert-Contains 'WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS' $smoke "Smoke must use an isolated CDP port."
Assert-Contains 'LOOM_CONTROL_PLANE_ROOT' $smoke "Smoke must isolate Loom data."
Assert-Contains 'APPDATA' $smoke "Smoke must isolate the Hook session."
Assert-Contains 'ExpectedExecutablePath' $smoke "Smoke cleanup must validate exact process paths."
```

- [ ] **Step 2: Run the contract and confirm RED**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-HookCanvasUiContract.ps1
```

Expected: FAIL because the smoke scripts do not exist.

- [ ] **Step 3: Implement the CDP inspector**

`Inspect-LoomWebView.mjs` accepts:

```text
--debug-port <port>
--output <json-path>
--screenshot <png-path>
```

It must:

1. Read `http://127.0.0.1:<port>/json/list`.
2. Connect to the `http://tauri.localhost/` page WebSocket.
3. Use `Runtime.evaluate` to click `[data-testid="nav-hook-bridge"]`.
4. Wait until `[data-testid="hook-canvas-thumbnail"]` exists.
5. Record visible text, node count, edge count, revision, advanced disclosure state, and YAML visibility.
6. Click the thumbnail.
7. Wait until `[data-testid="hook-canvas-view"]` exists.
8. Capture a PNG with `Page.captureScreenshot`.
9. Write structured JSON before closing the socket and call `process.exit(0)`.

The output contract is:

```json
{
  "thumbnailVisible": true,
  "thumbnailNodeCount": 3,
  "thumbnailEdgeCount": 1,
  "yamlVisible": false,
  "advancedOpen": false,
  "fullCanvasVisible": true,
  "offlineTextVisible": false
}
```

- [ ] **Step 4: Implement the isolated packaged UI smoke**

`Invoke-LoomHookCanvasUiSmoke.ps1` accepts:

```powershell
param(
    [Parameter(Mandatory = $true)][string]$PackageDir,
    [string]$EvidenceRoot = ".\target\runtime-smoke"
)
```

Follow exact PID/path helpers from `Invoke-LoomRunPersistenceSmoke.ps1`. The smoke must:

1. Resolve `Loom.exe` and `runtime\loom-daemon.exe` with `Get-LoomReleaseLayout`.
2. Record baseline processes for those exact executable paths.
3. Allocate separate daemon and WebView2 debug ports.
4. Create isolated `APPDATA`, `LOCALAPPDATA`, control-plane, configuration, logs, and WebView2 roots.
5. Write a three-node/one-edge Hook session under isolated AppData.
6. Write two valid PNG previews and leave one preview missing to verify node-level degradation.
7. Set `LOOM_DAEMON_URL`, `LOOM_CONTROL_PLANE_ROOT`, `LOOM_CONFIGURATION_ROOT`, `APPDATA`, `LOCALAPPDATA`, `WEBVIEW2_USER_DATA_FOLDER`, and `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` only for the child process.
8. Start the desktop, verify its daemon child PID and exact candidate paths, then wait for `/health`, `/status`, and `/v1/hook-bridge/canvas`.
9. Run `Inspect-LoomWebView.mjs` and assert its output contract.
10. Rewrite the isolated session with a fourth node and POST `/v1/artloom-compat/ipc/instantiate-workflow` to trigger `art_hook/instantiate`; assert the DOM revision/node count updates.
11. Save API JSON, CDP JSON, screenshot, and process evidence.
12. Stop only exact candidate PIDs and assert no candidate process remains beyond the baseline.

- [ ] **Step 5: Add the smoke to release verification**

In `verify-release.ps1`, keep the existing standalone smoke and then run:

```powershell
$hookCanvasSmokePath = Join-Path $repoRoot "scripts\Invoke-LoomHookCanvasUiSmoke.ps1"
$hookCanvasOutput = & powershell.exe -NoProfile -ExecutionPolicy Bypass `
    -File $hookCanvasSmokePath `
    -PackageDir $packageFullPath `
    -EvidenceRoot $evidenceRoot 2>&1
if ($LASTEXITCODE -ne 0) {
    throw "Hook canvas UI smoke failed: $($hookCanvasOutput -join [Environment]::NewLine)"
}
```

Change the result to include:

```powershell
hookCanvasSmoke = $hookCanvasSmokeStatus
```

- [ ] **Step 6: Wire the static contract into Windows CI**

Add `Test-HookCanvasUiContract.ps1` to the pre-generated-output validation block in `.github/workflows/ci.yml`. Add corresponding assertions to `Test-GitHubActionsContract.ps1`.

- [ ] **Step 7: Run contracts and frontend validation**

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-HookCanvasUiContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-GitHubActionsContract.ps1
npm --prefix apps/desktop test
npm --prefix apps/desktop run typecheck
npm --prefix apps/desktop run build
```

Expected: all pass. The packaged UI smoke itself runs after Task 8 builds a candidate.

- [ ] **Step 8: Commit smoke tooling**

```powershell
git add scripts/Inspect-LoomWebView.mjs scripts/Invoke-LoomHookCanvasUiSmoke.ps1 scripts/verify-release.ps1 scripts/tests/Test-HookCanvasUiContract.ps1 scripts/tests/Test-GitHubActionsContract.ps1 .github/workflows/ci.yml
git commit -m "test: add Hook canvas desktop smoke"
```

---

### Task 8: Document, verify, publish the branch, and build the test Release

**Files:**
- Create: `docs/progress/phase-44-hook-canvas-thumbnail.md`
- Modify: `docs/progress/MASTER.md`
- Modify: `README.md`

- [ ] **Step 1: Add progress documentation**

Create Phase 44 documentation covering:

```text
normalized Hook canvas API
safe preview endpoint
real thumbnail and full visual canvas
event-driven refresh semantics
advanced-only YAML/technical details
isolated WebView2 smoke evidence
```

Add Phase 44 to `docs/progress/MASTER.md` and document the user workflow in README:

```text
Open Screenshot Sync -> inspect live Hook canvas -> click to open visual workflow.
Advanced Technical Information is optional and collapsed by default.
```

- [ ] **Step 2: Run focused validation**

```powershell
cargo fmt --all -- --check
cargo test --locked -p loom-daemon hook_canvas -- --nocapture
cargo test --locked -p loom-daemon daemon_exposes_hook_canvas -- --nocapture
cargo test --locked -p loom-daemon daemon_serves_only_registered_hook_canvas -- --nocapture
npm --prefix apps/desktop test
npm --prefix apps/desktop run typecheck
npm --prefix apps/desktop run build
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-HookCanvasUiContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-GitHubActionsContract.ps1
```

Expected: all pass.

- [ ] **Step 3: Run full repository validation**

```powershell
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
cargo test --locked --manifest-path apps\desktop\src-tauri\Cargo.toml --lib
cargo check --locked --manifest-path apps\desktop\src-tauri\Cargo.toml
npm --prefix apps\desktop test
npm --prefix apps\desktop run typecheck
npm --prefix apps\desktop run build
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-StandaloneLayout.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-StandaloneReleaseContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-ReleaseIntegrityTamper.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-GitHubActionsContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-HookCanvasUiContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\Test-LoomRunPersistenceSmokeContract.ps1
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\Test-LoomDaemonConcurrencySmokeContract.ps1
```

Expected: all pass with zero failures.

- [ ] **Step 4: Commit documentation**

```powershell
git add README.md docs/progress/MASTER.md docs/progress/phase-44-hook-canvas-thumbnail.md
git commit -m "docs: record Hook canvas visual workflow phase"
```

- [ ] **Step 5: Perform final code review**

Use `requesting-code-review` against the complete diff from `0b20589` to `HEAD`. Fix all Critical and Important findings, rerun affected tests, and commit fixes separately.

- [ ] **Step 6: Build a fresh Release in the required output root**

```powershell
$shortSha = (git rev-parse --short=7 HEAD).Trim()
$versionId = "20260722-hook-canvas-$shortSha"
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release.ps1 `
  -VersionId $versionId `
  -OutputRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom'
```

Expected: a new versioned directory containing exactly one root `Loom.exe`, `runtime\loom-daemon.exe`, the desktop ZIP, the CLI ZIP, manifest, and checksums.

- [ ] **Step 7: Verify the Release including the new UI smoke**

```powershell
$releaseDir = Join-Path 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom' $versionId
powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\verify-release.ps1 `
  -PackageDir $releaseDir `
  -RunSmoke
```

Expected JSON:

```json
{
  "filesChecked": 32,
  "smoke": "passed",
  "hookCanvasSmoke": "passed"
}
```

- [ ] **Step 8: Confirm cleanup and repository state**

```powershell
git status --short --branch
git rev-parse HEAD
Get-Process -Name Loom,loom-daemon -ErrorAction SilentlyContinue |
  Select-Object Id, ProcessName, Path
```

Expected:

- worktree clean;
- no process from the new Release candidate remains after smoke;
- any pre-existing user Loom/Hook processes retain their original PID/path;
- the new Release manifest reports `gitDirty = false` and the final `HEAD`.

- [ ] **Step 9: Push the completed branch and verify the remote SHA**

```powershell
git push origin feat/single-entry-release
git rev-parse HEAD
git rev-parse origin/feat/single-entry-release
```

Expected: local and remote SHAs match exactly.

---

## Execution Notes

- Execute tasks in order because desktop contracts depend on the daemon wire format.
- Keep each task as one coherent commit; do not combine unrelated cleanup.
- Use isolated data roots and dynamic ports for every smoke run.
- Never stop or replace the user's existing Hook or Loom processes by name alone.
- Do not modify `C:\Users\vmjcv\AppData\Roaming\com.vmjcv.arthook-next\session.json` during tests.
- Do not generate Release output outside `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom`.
