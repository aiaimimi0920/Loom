# Neuro Surface SDK 1.0 (`loom.surface.v1`)

`neuro-surface.d.ts` defines the JavaScript/TypeScript Surface API provided inside Otter's sandboxed WebView2 Surface host. TypeScript is used at build/authoring time only; packaged Arts must ship compiled JavaScript, and the Surface sandbox exposes no Node.js, TypeScript, Python, Deno, QuickJS, or general WASM runtime.

## Minimal entrypoint

```ts
/// <reference path="./neuro-surface.d.ts" />

NeuroSurface.define({
    mount({ root, snapshot, emit }) {
        const button = document.createElement("button");
        button.textContent = String(snapshot.authoritativeState ?? "Refresh");
        button.addEventListener("click", () => {
            emit({
                nodeId: "refresh",
                event: "click",
                action: "refresh",
                class: "discrete",
                payload: {},
            });
        });
        root.replaceChildren(button);
        return () => button.remove();
    },
    update({ snapshot }) {
        // Reconcile existing DOM against the new authoritative snapshot; business execution stays in Loom.
        void snapshot;
    },
});
```

## Multi-view declarations and logical `fullSize`

A Surface Art may declare multiple user-selectable views under `metadata.capabilities.surface`. Every view requires a unique safe `id`, a non-empty `label`, and its own logical `fullSize`; whenever `views` is present, `defaultViewId` must name one of those views:

```json
{
  "protocolVersion": "loom.surface.v1",
  "apiVersion": "1.0",
  "variants": [{ "runtime": "javascript", "entry": "surface/main.js" }],
  "views": [
    { "id": "full", "label": "Full View", "fullSize": { "width": 960, "height": 820 } },
    { "id": "price", "label": "Compact Quote View", "fullSize": { "width": 620, "height": 620 } }
  ],
  "defaultViewId": "full"
}
```

- Creating a new Art node mounts `defaultViewId` at its declared `fullSize` and passes the active view ID as `snapshot.viewId`.
- Pressing `Ctrl+E` opens the Art editor bar to switch views, resizing the node to the target view's `fullSize`.
- `Ctrl + Mouse Wheel` scales the node proportionally: the Surface lays out within its logical `fullSize` while Otter scales typography, graphics, and pointer coordinates uniformly.
- `loom-plugin validate` enforces both JSON Schema presence rules and cross-field uniqueness/membership of `defaultViewId` in `views`.

## Sandbox security boundaries

- Network APIs (`fetch`, `XMLHttpRequest`, `WebSocket`, `EventSource`, `sendBeacon`) are blocked.
- Node APIs, Tauri IPC, filesystem paths, host DOM handles, and native plugins are unavailable.
- Each attachment runs inside an isolated `iframe` enforced by CSP, a `MessageChannel` capability token, heartbeat liveness, event-rate caps, timer caps, and DOM node budgets.
- `resources` and `NeuroSurface.resource()` resolve only host-verified, lease-bound local resource URLs.
- User interactions emit typed events via `emit()`; authoritative state updates, high-risk confirmations, and formal result commits are owned by Loom and the Otter host.

Validate Surface manifests, declarative scenes, action bindings, confirmation flags, and entry paths before packing:

```powershell
loom-plugin validate <ART_DIR>
loom-plugin pack <ART_DIR> <OUTPUT_ZIP>
```
