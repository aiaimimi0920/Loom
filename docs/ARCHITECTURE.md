# Loom Architecture

Loom is an independent Rust workspace and daemon-first personal/home control hub, Art/Capability/MCP runtime, Otter companion coordinator, and AI Gateway relay. Reusable Rust crates own runtime behavior and security invariants, while `loom-daemon`, `loom` (CLI), `loom-plugin`, the embedded Web management console (`/ui/`), and the Windows Tauri desktop shell (`apps/desktop`) act as thin operational frontends.

## System boundaries

- **Loom (`loom-daemon` and workspace crates)**: owns Art, Framework, Capability Plugin, and MCP server package lifecycles; outbound MCP server exposure (`/mcp` and `loom mcp-stdio`); Otter workflow synchronization, hybrid cross-end workflow relay, live streams, QR/peer projection, and Tile Walls; companion application configuration ownership; durable SQLite run/event evidence; and global AI Gateway relay calls. Cloud Relay configuration is reserved; cloud transport is not implemented.
- **Neuro Gateway**: owns hosted model-provider routing, upstream credentials, quota accounting, and provider adapters. Loom forwards OpenAI-compatible requests via `loom_gateway` without embedding upstream provider secrets in capability child processes.
- **NAccount & Neuro Platform**: own user authentication (`https://naccount-auth.aiaimimi.com`), AssetLibrary cloud package distribution (`https://assetlibrary.aiaimimi.com`), and central QR projection v2 coordination.
- **Otter**: owns foreground desktop screen capture, canvas UI interaction, local `otter.art.v1` QuickJS execution under Loom-issued Ed25519 grants, and physical Tile display rendering.

Loom enforces canonical-only wire contracts and publisher-qualified package layouts (`publisher/id`). Persisted daemon settings (`<control-plane>/settings/settings.json`) use canonical `snake_case`, while Tauri/Web DTOs use `camelCase` at the UI boundary. Historical NeuroLoom/ArtLoom migration mappings remain archived in Git (`ade6992:docs/MIGRATION_MAP.md` and `cleanup-base-20260928`).

## Editions and deployment topologies

Loom builds from one codebase into three explicit editions (`scripts/build-release.ps1 -Edition desktop|web|api` and `Dockerfile` `ARG LOOM_EDITION=web|api`):

| Edition | Binaries & UI Assets | Typical Deployment |
| --- | --- | --- |
| `desktop` | `Loom.exe` (Tauri shell) + `runtime/loom-daemon.exe` + bundled Framework/MCP/Art ZIP catalogs | Windows single-machine desktop or remote client workbench |
| `web` | `loom-daemon` (compiled with `management-ui` embedding `loom_embedded_ui` at `/ui/`) + `loom` CLI | Home/Linux/Docker server or NAS with browser-accessible management console |
| `api` | Headless `loom-daemon` (`runtime-defaults` only, `/ui/*` disabled) + `loom` CLI | Minimal headless server, container, or appliance deployment |

The Tauri shell supports **local** sidecar and **remote HTTP** profiles. Native HTTPS is explicitly rejected without changing the profile. The `/ui/` Web console instead uses its browser origin; access an HTTPS server through its own `/ui/` behind an authenticated TLS terminator. Login submits the token in a `POST /ui/` form body, never in a login URL. An HttpOnly cookie does not encrypt plaintext LAN transport.

## Workspace crates and applications

### Core library crates (`crates/`)

- `loom_core`: shared identifiers, structured error codes, version constants, and run/session status types.
- `loom_durable`: synchronous `RunEvidenceStore` trait with in-memory and WAL-mode bundled SQLite (`runs/loom-runs.sqlite3`) implementations, atomic run/event transactions, and startup `daemon_restarted` recovery.
- `loom_embedded_ui`: compile-time asset table bundling `apps/desktop/dist/web` for the `web` server edition under `/ui/*` with path-traversal rejection and SPA route fallback.
- `loom_local_data`: platform control-plane root resolution and client-server connection profile validation (`local` vs. `remote` daemon endpoints).
- `loom_configuration`: schema-driven UI configuration sections (`UiSection`), ownership claims, and file-backed document persistence for Loom-managed companion apps.
- `loom_gateway`: bounded OpenAI-compatible HTTP client supporting loopback HTTP/HTTPS, private LAN (RFC1918/ULA) HTTP/HTTPS with system-proxy bypass, and remote HTTPS.
- `loom_protocol`: language-neutral Rust types and embedded v1 JSON Schemas (`protocol/schemas/*.json`) for Frameworks, Arts, Capabilities, Extensions, Surfaces, Device Sessions, Otter Bridge (`loom.hook.v1`), Local Art grants/packages, Live Screenshot (`loom.live.v1`), QR Projection, and Tile Walls (`loom.wall.v1`).
- `loom_plugin_security` & `loom_security`: canonical ZIP/directory SHA-256 digests, safe archive extraction, Ed25519 package and `local_art` grant signing, publisher trust stores, key revocation, DPAPI/file `CredentialStore`, and outbound network/IP validation.
- `loom_process`: cross-platform managed child-process execution, output caps, timeouts, Windows Job Object containment, and Linux Bubblewrap + `pidfd` namespace isolation (see [runtime security](RUNTIME_SECURITY.md)).
- `loom_tool_registry`: publisher-qualified Framework and Art registries, staged atomic installation, dependency lockfiles, SemVer child Art rebinding, rollback, and crash-journal recovery.
- `loom_capability_runtime`: signed Capability Plugin lifecycle, length-prefixed `loom.capability.runtime.v1` stdio framing, `InvocationControl` cancellation and 4 KiB bounded progress slots, host-side translation model brokering, and resource staging.
- `loom_mcp`: stdio and HTTP MCP client/package verification, schema-checked tool invocation, and cross-platform `entry.platforms` resolution.
- `loom_workflow_store` & `loom_workflow_runtime`: `loom.workflow.v1` YAML/JSON graph storage, topological validation, dynamic placement (`local`/`server`/`auto`), terminal step relay hooks, and DAG execution.
- `loom_native_image`, `loom_image_io`, & `loom_shared_image`: bounded `core.image.*` operators, PNG validation/transcoding, and local shared-memory/file image transport.
- `loom_otter_bridge` & `loom_local_channel`: native-only TLS 1.3 loopback WebSocket transport with pinned per-start certificates, private discovery manifests, and exporter-bound connection proofs.
- `loom_projection`: central and peer-to-peer QR Projection v2 state machines and Iroh/HTTPS transport coordination.
- `loom_ocr`: RapidOCR/PP-OCR ONNX inference, CTC timestep span alignment, conservative text/list-dash rescue, QR/barcode decoding (`rxing`), and golden-dataset quality evaluation (see [OCR contract](OCR_CONTRACT.md)).
- `loom_perf`: monotonic timing and bounded diagnostic helpers.

### Applications (`apps/`)

- `apps/daemon` (`loom-daemon`): primary HTTP/HTTPS/WSS runtime server, bounded worker executor, outbound `/mcp` server, LAN listener, and local Otter bridge host.
- `apps/cli` (`loom`): lightweight operator CLI (`status`, `health`, `frameworks list`, `arts list`, `workflows list`, `mcp list`, `ai status`, and `mcp-stdio` bridge).
- `apps/plugin-cli` (`loom-plugin`): standalone Plugin SDK CLI for scaffolding, signing, validating, packing, conformance testing, and trust-store management.
- `apps/art-store` (`loom-art-store`): local/self-hosted multi-kind package registry (`art`, `capability`, `mcp_server`) with `NU` publisher key rotation.
- `apps/ocr-host` & `apps/text-translation-host`: out-of-process Capability Plugin executables for `neuro.official/ocr` and `neuro.official/text-translation`.
- `apps/desktop`: React + `@neuro/ui` workbench compiled both into the Windows Tauri `Loom.exe` shell and into the embedded Web console (`npm run build:web`).

## Key runtime subsystems

### 1. Bounded HTTP executor and route isolation

Production `loom-daemon` runs a bounded worker pool (`LOOM_DAEMON_WORKERS=4`, `LOOM_DAEMON_QUEUE_CAPACITY=32` by default) over a single `TcpListener`:

- `/health` and `/status` bypass the normal queue as reserved liveness probes.
- Concurrent-safe routes (`/v1/capabilities`, `/v1/ai/*`, `/v1/invoke`, `/v1/invoke/cancel`, `/v1/runs/*`, `/v1/mcp/registry`, `/v1/hook-bridge/canvas*`, `/v1/asset-library/download*`) execute in parallel across workers.
- Long-polling `/v1/surfaces/stream` and `/v1/live/sessions/*/events` run on a dedicated stream pool so they never starve request workers.
- Queue saturation fails fast with HTTP `503` (`daemon_busy`, `retryable: true`) without blocking the accept loop. Header-only upload preflight is detailed in [runtime security](RUNTIME_SECURITY.md).

### 2. Package model: Frameworks, Arts, Capabilities, and MCP servers

All installable extensions live under `<LOOM_CONTROL_PLANE_ROOT>` (`frameworks/`, `arts/`, `capabilities/`, `mcp-servers/`) with read-only `versions/<version>-<digest>/` directories, atomic `active.json` pointers, lockfiles, and mutable `state/`/`cache/`/`outputs/` roots separated from code. Installing, executing, upgrading, rolling back, or uninstalling a package never modifies Loom or Otter source trees.

### 3. Outbound MCP server and Scoped Share Tokens

`loom-daemon` acts as an outbound Model Context Protocol server (`POST /mcp` JSON-RPC 2.0 and `loom mcp-stdio` via `apps/cli`) so external AI assistants and automation clients can invoke authorized Loom tools and inspect live screen state:

- Administrators manage tokens via `GET /v1/mcp-share/status`, `GET /v1/mcp-share/tokens`, `POST /v1/mcp-share/tokens`, and `DELETE /v1/mcp-share/tokens/{id}` (`apps/daemon/src/mcp_share_tokens.rs`).
- Tokens (`loom_mcp_<random>`, stored only as SHA-256 digests in `settings/mcp-share-tokens.json`, max 64 active tokens, optional TTL) enforce explicit scopes: `arts:execute`, `capabilities:invoke`, `screen:read`, `devices:control`, and `projection:control`.

### 4. Otter integration, multi-device sync, and companion hub

Loom coordinates local and remote Otter instances across:

- Native loopback TLS 1.3 bridge (`loom.hook.v1` / `loom.surface.v1`), optional RFC1918 LAN HTTPS listener (`8766`), and peer-to-peer projection. Cloud Relay exposes configuration/status only: projection send/inbox fail with `501 cloud_relay_transport_unavailable`; no envelope is sent or queued. The cloud-domain MCP route dispatches locally and is not a cloud tunnel.
- Two-level Host Device -> App Sessions hierarchy (`GET /v1/devices/hierarchy`), live multi-device execution tracking, and 200-entry workflow history archiving (`/v1/otter-sync/*`).
- Intra-workflow hybrid terminal relay (`/v1/otter/relay/*`) and schema-driven Companion App Settings Hub (`/v1/configuration/companion-apps*`). See [Otter integration](OTTER_INTEGRATION.md) and [protocol specifications](../protocol/README.md).

### 5. Global AI Gateway relay

`apps/daemon/src/ai_gateway.rs` and `loom_gateway` expose runtime-configurable (`PUT /v1/ai/config` or `LOOM_GATEWAY_*` env vars) OpenAI-compatible model listing (`GET /v1/ai/models`), chat completions (`POST /v1/ai/chat/completions`), and durable `ai.chat.v1` capability runs (`POST /v1/invoke`). See [Gateway integration](GATEWAY_INTEGRATION.md).
