# Otter Native Art Authoring SDK (`otter.art.v1`)

This directory defines the TypeScript authoring declarations and runnable JavaScript examples for `otter.art.v1` Local Arts. Keep `otter.art.v1` separate from `sdk/surface` (`loom.surface.v1`): do not mix both global declaration sets in the same TypeScript project. Otter owns the QuickJS execution environment, while Loom owns immutable packaging, per-device approvals, Ed25519 grant signing, and authenticated capability services (see [LOCAL_ART_EXECUTION.md](../../protocol/LOCAL_ART_EXECUTION.md)).

## Authoring and typechecking

JavaScript entries can reference `/// <reference path="../native-art.d.ts" />` and TypeScript projects can import the ambient declarations directly. Shipped Art entries must be plain ES2022 JavaScript with no runtime `import`/`require` statements, invoking `NeuroSurface.define(...)` directly. Type declarations are compile-time only and never grant runtime permissions; do not add `lib.dom` or `@types/node` to mask unsupported host APIs.

From the Loom repository root, validate the positive and `@ts-expect-error` negative contracts plus example syntax using the TypeScript compiler installed under `apps/desktop`:

```powershell
node apps/desktop/node_modules/typescript/lib/tsc.js -p sdk/otter-art/tests/tsconfig.json
node --check sdk/otter-art/examples/packaged-image-service.js
node --check sdk/otter-art/examples/service-image-result.js
node --check sdk/otter-art/examples/live-trigger.js
```

The declarations cover only the verified native node tree, button/input events, and 2D `ArtCanvas` subset (simple native selectors only; no `quadraticCurveTo`, `ellipse`, `getImageData`, `toDataURL`, or self-drawing a `Canvas` onto itself).

## Host API surface and runnable examples

- **Package resources & images (`examples/packaged-image-service.js`)**:
  - `NeuroSurface.resource(id)` returns verified UTF-8 text (`text/plain` / `application/json`) or Data URLs (`application/octet-stream`) from `snapshot.state.package.<id>`.
  - `NeuroSurface.image(resourceId)` returns an opaque session-scoped image handle for verified package or input PNGs (shared session budget: <= 8 images, <= 8 MiB encoded PNG, <= 16 MiB decoded pixels, <= 32 MiB `ArtCanvas`).
- **Capability service calls**:
  - `NeuroSurface.service(serviceId, input, timeoutMs?)`: bounded JSON request/response (`<= 64 KiB` JSON, `1..=60000` ms timeout, at most one in-flight service call per operation).
  - `NeuroSurface.serviceImages(serviceId, input, imageHandles, timeoutMs?)`: passes up to 4 installed `NeuroSurface.image(...)` handles (`<= 4 MiB` each / `8 MiB` total) to a `trusted` Capability command granted `hook.unit.image.read`.
  - `NeuroSurface.serviceCanvases(serviceId, input, canvases, timeoutMs?)`: snapshots the intrinsic bitmap of up to 4 non-empty `ArtCanvas` instances (`<= 2 MiB` each / `3 MiB` total PNG) over Otter's private native pipe without exposing raw pixel bytes to JS.
  - `NeuroSurface.serviceImage(serviceId, input, { images?, canvases?, timeoutMs? })` (`examples/service-image-result.js`): returns `{ output, image }` where `image` (`<= 2 MiB` PNG, `<= 4096` px per side) can be drawn onto an `ArtCanvas` or passed to subsequent `serviceImages` calls.
  - `NeuroSurface.serviceWithConfirmation(serviceId, input, { timeoutMs?, images?, canvases?, imageOutput? })`: requests host-owned native user confirmation for `requiresUserGesture` commands (displaying full parameters and image thumbnails in a non-modal native prompt; defaults to deny).
- **Formal output commits**:
  - `NeuroSurface.commitResult(outputs)` queues a single-slot `otter.art.result.v1` formal result when the package and approval include `workspace_result`. Returning `true` indicates local queuing, while Otter's Results/Save/Checkpoint pipeline owns durable persistence.
- **Automatic Live observation callbacks (`examples/live-trigger.js`)**:
  - `onLiveTrigger(event)` fires only after the user attaches the Local Art host to a Live session and explicitly enables **Allow automatic callback** in Otter's native menu (see [LOCAL_ART_EXECUTION.md](../../protocol/LOCAL_ART_EXECUTION.md#5-local-art-live-viewer-input-control-and-automatic-triggers)). Events deliver `{ sessionId, epoch, delivery }` once per claimed trigger slot without synthesizing user clicks or gestures.
