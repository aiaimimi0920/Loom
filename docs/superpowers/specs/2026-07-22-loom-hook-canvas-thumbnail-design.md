# Loom Hook Canvas Thumbnail and Visual Workflow Design

- Date: 2026-07-22
- Status: Approved for implementation
- Scope: Loom only
- Related product: Hook screenshot and node canvas integration

## 1. Context

Loom currently exposes the Hook screenshot synchronization path, but the
user-facing surface is too close to the implementation contract. The page
shows protocol details, session metadata, and workflow YAML-oriented controls
instead of showing the Hook node canvas that users actually need to
understand.

The target user is a UI-oriented operator. They should be able to recognize a
screenshot node, an Art processing node, and the connection between them
without reading YAML, JSON, filesystem paths, or protocol method names.

The current Hook session already contains the data required for a visual
representation: node identifiers, node types, coordinates, dimensions, image
sources, Art identifiers, and links. The missing capability is a stable Loom
view model and a safe preview boundary.

## 2. Goals

1. Make a live Hook canvas thumbnail the primary content of the Loom
   Screenshot Sync page.
2. Preserve the Hook canvas spatial relationship, node imagery, node types,
   and links in the thumbnail.
3. Open the Loom visual workflow canvas when the user clicks the thumbnail.
4. Keep the thumbnail read-only while making the full Loom canvas the visual
   editing and execution surface.
5. Update the thumbnail from Hook Bridge events without relying on fixed
   interval polling.
6. Keep YAML as an internal persistence and compatibility format while hiding
   it from the normal user path.
7. Provide a safe, daemon-owned image preview boundary that never exposes
   arbitrary filesystem access to the frontend.
8. Degrade at node or snapshot level instead of replacing the whole UI with
   raw technical errors.

## 3. Non-goals

1. Do not modify the Hook repository or Hook's session persistence format.
2. Do not create a second persistent source of truth for the Hook canvas.
3. Do not replace the existing Hook Bridge compatibility methods.
4. Do not build a second independent workflow editor inside the thumbnail.
5. Do not remove YAML support for advanced users, import/export, diagnostics,
   or compatibility.
6. Do not add a fixed-interval polling loop as the primary synchronization
   mechanism.

## 4. Product Decisions

### 4.1 YAML visibility

YAML is hidden by default across normal workflow surfaces. The visual canvas,
node properties, save, load, and run actions must be usable without opening a
YAML editor.

YAML, JSON, filesystem paths, compatibility commands, and protocol method
names remain available inside a collapsed Advanced Technical Information
section. This preserves support and migration workflows without imposing
implementation details on ordinary users.

### 4.2 Thumbnail behavior

The Screenshot Sync page shows one live read-only thumbnail for the current
Hook canvas. The thumbnail:

- fits all valid nodes into a stable container;
- preserves relative positions and aspect ratios;
- renders real node previews when available;
- draws links in an SVG overlay;
- shows readable labels and status rather than raw IDs;
- keeps a minimum visible/clickable size for very small nodes;
- shows a node-level placeholder when one preview is missing;
- provides a manual refresh action as a recovery fallback.

### 4.3 Full canvas behavior

Clicking the thumbnail opens the Loom visual workflow canvas for the Hook live
workflow. The selected thumbnail node, when applicable, becomes the initial
selected node in the full canvas.

The full canvas supports visual node selection, parameter editing, node
execution, workflow execution, and existing workflow save/update behavior.
The YAML representation is generated and persisted internally.

The thumbnail itself does not support drag, resize, or link editing. This keeps
the Screenshot Sync page a reliable status and preview surface while avoiding
two competing editors.

## 5. Architecture

The daemon owns translation from Hook storage to a stable Loom view model:

```text
Hook session.json
        |
        v
HookCanvasAdapter
        |
        v
HookCanvasSnapshot
        |
        +--> Screenshot Sync thumbnail
        +--> Full visual workflow canvas
```

The existing `/v1/hook-bridge/session` endpoint remains available for
compatibility and advanced diagnostics. Normal desktop UI components use the
new normalized canvas endpoint instead of parsing raw `stickers` and `links`.

The adapter does not write the Hook session and does not create a second
persistent canvas store.

## 6. API Contract

### 6.1 Canvas snapshot

Add:

```text
GET /v1/hook-bridge/canvas
```

Response shape:

```ts
interface HookCanvasSnapshot {
  available: boolean;
  revision: string;
  updatedAt: string | null;
  workflowId: string | null;
  bounds: {
    x: number;
    y: number;
    width: number;
    height: number;
  };
  nodes: HookCanvasNode[];
  edges: HookCanvasEdge[];
  warnings: string[];
}

interface HookCanvasNode {
  id: string;
  kind: "screenshot" | "art" | "unknown";
  label: string;
  artId: string | null;
  x: number;
  y: number;
  width: number;
  height: number;
  previewAvailable: boolean;
  previewUrl: string | null;
  status: "ready" | "processing" | "error" | "unknown";
}

interface HookCanvasEdge {
  id: string;
  sourceNodeId: string;
  sourcePortId: string | null;
  targetNodeId: string;
  targetPortId: string | null;
}
```

The exact wire format may use the repository's existing serde naming
conventions, but the semantic fields above are required.

`workflowId` is nullable because a Hook session can exist before a workflow
has been persisted. The desktop may use the existing Hook live workflow ID as
the visual workflow destination when no explicit ID is present.

### 6.2 Preview endpoint

Add:

```text
GET /v1/hook-bridge/canvas/nodes/{nodeId}/preview
```

The daemon resolves `nodeId` against the currently loaded Hook session and
only reads the image path registered for that node. The request must not
accept a caller-supplied filesystem path.

Required behavior:

- Return the image bytes with a validated image MIME type.
- Reject unknown node IDs.
- Reject paths that are missing, directories, or outside the allowed session
  image boundary.
- Apply a maximum response size.
- Return a node-level error without invalidating the canvas snapshot.
- Use cache validation keyed by the canvas revision where practical.

### 6.3 Compatibility

Existing Hook Bridge methods and `/v1/hook-bridge/session` remain compatible.
The new endpoints are additive. Existing Hook clients do not need to know
about the normalized canvas model.

## 7. Runtime Data Flow

On page entry:

1. The desktop requests `/v1/hook-bridge/canvas`.
2. The daemon reads and normalizes the current session.
3. The desktop lays out the snapshot using a stable fit-to-bounds transform.
4. Node preview URLs are loaded independently.
5. A preview failure affects only that node.

The desktop subscribes to existing Hook Bridge events:

```text
art_hook/instantiate
art_loom/workflow_updated
art_loom/arts_updated
```

Events arriving within a short debounce window of approximately 150-250ms
are merged into one snapshot refresh. A refresh is applied only when the
returned revision differs from the displayed revision.

Workflow overwrite operations already emit `art_loom/workflow_updated`; the
design does not introduce a separate overwrite-only subscription channel.

The manual refresh button performs the same snapshot request and is retained
for recovery when an event is missed.

When the full visual canvas changes a node parameter, topology, or workflow
state, Loom continues to use the existing visual workflow and Hook Bridge
write paths. YAML serialization remains an internal implementation detail.

## 8. Error and Empty States

### 8.1 No session

Show a visual empty canvas with:

```text
Hook 中还没有截图节点
[打开 Hook] [重新读取]
```

Do not show an empty YAML document or raw file-not-found error.

### 8.2 Hook disconnected with a valid snapshot

Keep the last successful snapshot visible and label it as an offline snapshot.
View-only operations remain available. Mutations that require Hook Bridge are
disabled until the connection returns.

### 8.3 Invalid or temporarily unreadable session

Keep the last successful in-memory snapshot during the current UI lifetime,
show a synchronized-paused state, and offer retry. Detailed parsing errors
appear only in Advanced Technical Information.

### 8.4 Missing node preview

Keep the node geometry, label, status, and links. Render a neutral preview
placeholder for that node and continue rendering the rest of the canvas.

### 8.5 Invalid geometry or links

Normalize invalid geometry to bounded defaults. Ignore links that reference
unknown nodes and add a warning to the snapshot. Do not fail the entire
canvas.

### 8.6 Loom daemon offline

Use the existing local service recovery UI. Do not expose raw JSON, YAML, or
HTTP error bodies in the primary workflow surface.

## 9. Desktop Component Boundaries

Add focused modules instead of adding all rendering logic to `App.tsx`:

```text
apps/desktop/src/components/hook/HookCanvasThumbnail.tsx
apps/desktop/src/components/hook/HookCanvasView.tsx
apps/desktop/src/components/hook/HookCanvasNode.tsx
apps/desktop/src/services/hookCanvas.ts
```

Responsibilities:

- `HookCanvasThumbnail`: read-only fit-to-bounds preview, status, and click
  navigation.
- `HookCanvasView`: full visual workflow canvas integration.
- `HookCanvasNode`: shared node rendering for screenshot, Art, and unknown
  node kinds.
- `hookCanvas.ts`: TypeScript contracts, response normalization, layout math,
  revision handling, and API helpers.

Existing workflow UI should be reorganized so the visual canvas appears before
the advanced YAML surface. Existing save, load, delete, and execute behavior
must remain available through user-facing visual controls.

## 10. Security and Privacy

1. The daemon remains loopback-only.
2. The frontend never receives an unrestricted local file path as an image
   authority.
3. Preview requests are resolved by node ID against the current session.
4. Path canonicalization and allowed-root checks happen before file reads.
5. Preview errors do not reveal arbitrary filesystem contents.
6. Real user session data is not modified by the Loom UI read path.
7. Tests use copied session data and isolated AppData roots.

## 11. Test Plan

### 11.1 Rust tests

Add tests for:

- normalization of a realistic multi-node, multi-link Hook session;
- preservation of relative positions and dimensions;
- bounds calculation, including negative coordinates;
- screenshot, Art, and unknown node classification;
- empty and missing sessions;
- malformed sessions and structured warnings;
- invalid link filtering;
- safe preview MIME detection and response size limits;
- missing image behavior;
- unknown node IDs and path traversal rejection.

### 11.2 TypeScript tests

Add pure logic tests for:

- fit-to-bounds coordinate transformation;
- aspect ratio preservation;
- minimum clickable node size;
- empty and degenerate bounds;
- revision deduplication;
- selection clearing when a node disappears;
- per-node preview failure handling;
- default-collapsed advanced technical information.

### 11.3 Real UI and release verification

Use isolated `APPDATA`, `LOCALAPPDATA`, daemon port, WebView2 data folder, and
Hook session fixture to verify:

1. Screenshot Sync initially shows the real canvas thumbnail.
2. The default page contains no YAML editor or raw protocol details.
3. Real node images, relative positions, and links are visible.
4. Clicking the thumbnail opens the full visual canvas.
5. The initial node selection is preserved.
6. A Hook Bridge update refreshes the snapshot revision.
7. A missing image degrades only one node.
8. Offline mode preserves the last valid snapshot.
9. Advanced Technical Information reveals the optional YAML view.
10. Release packaging still has one root `Loom.exe` and the daemon under
    `runtime`.

All smoke instances must be stopped by exact PID and path. The user's running
Loom or Hook instances must not be stopped or replaced.

## 12. Acceptance Criteria

The work is accepted when:

- Screenshot Sync's primary visual is the Hook node canvas thumbnail.
- The thumbnail shows real screenshots, Art nodes, relative placement, and
  links where data is available.
- Clicking it opens the Loom visual workflow canvas.
- A normal user can complete visual load, edit, save, and run flows without
  seeing YAML.
- YAML and technical protocol details remain available only after explicitly
  opening Advanced Technical Information.
- Hook and Loom failures degrade locally and preserve useful visual state.
- Existing Hook Bridge compatibility tests remain green.
- New daemon, desktop, frontend, and release smoke tests pass.
- The final Release is generated under the requested Neuro release folder.

## 13. Release and Change Boundary

Source changes are limited to:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\Loom
```

Release artifacts are limited to:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom
```

The Hook repository is read-only reference context for this task. No Hook
source or user session file is modified.
