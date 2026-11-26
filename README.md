# Loom

[![CI](https://github.com/aiaimimi0920/Loom/actions/workflows/ci.yml/badge.svg)](https://github.com/aiaimimi0920/Loom/actions/workflows/ci.yml)
[![Build Windows](https://github.com/aiaimimi0920/Loom/actions/workflows/build-windows.yml/badge.svg)](https://github.com/aiaimimi0920/Loom/actions/workflows/build-windows.yml)
[![Docker](https://github.com/aiaimimi0920/Loom/actions/workflows/docker.yml/badge.svg)](https://github.com/aiaimimi0920/Loom/actions/workflows/docker.yml)

Loom is Neuro's daemon-first personal and home control hub, Art/Capability/MCP runtime, Otter companion coordinator, and AI Gateway relay.

Repository: https://github.com/aiaimimi0920/Loom

Loom owns the runtime contracts for:

- **Package & Plugin Lifecycles**: publisher-qualified Art, Framework, Capability Plugin, and cross-platform MCP server packages under immutable versioned storage, plus a unified multi-platform store (`GET /v1/store/catalog` and AssetLibrary integration).
- **Inbound & Outbound MCP**: consuming packaged/external MCP servers inside Arts and exposing Loom's own authorized Arts, capabilities, devices, projections, and live screen contexts to external AI clients via `POST /mcp` and `loom mcp-stdio` with Scoped Share Tokens.
- **Otter & Companion Coordination**: `loom.hook.v1` / `loom.surface.v1` native bridge, optional LAN HTTPS listener (`8766`, disabled by default; see [LAN connection](protocol/LAN_CONNECTION.md)), two-level Host Device -> App Sessions hierarchy (`GET /v1/devices/hierarchy`), multi-device Otter workflow sync and history archive (`/v1/otter-sync/*`), intra-workflow hybrid terminal relay (`/v1/otter/relay/*`), 3-tier NAccount Cloud Relay (`/v1/cloud-relay/*`), Live Screenshot streaming, QR Projection, Tile Walls, and the Companion App Settings Hub (`/v1/configuration/companion-apps*`).
- **Global AI Gateway Relay & Durable Evidence**: OpenAI-compatible `/v1/ai/*` relay and `ai.chat.v1` / `loom.runtime.probe.v1` invocation backed by atomic SQLite run/event evidence (`runs/loom-runs.sqlite3`), plus an optional IP-loopback local model adapter for `text-translation`.

Loom is not a replacement for Neuro Gateway (which owns upstream hosted-provider routing, credentials, and quotas) or Otter (which owns foreground desktop capture, canvas UI, and local `otter.art.v1` execution). Loom and Otter enforce canonical-only protocols and `publisher/id` package layouts; retired legacy aliases are rejected. See [public protocols](protocol/README.md), [architecture](docs/ARCHITECTURE.md), and [Otter integration](docs/OTTER_INTEGRATION.md).

## Workspace layout

```text
./
├── apps/
│   ├── daemon/                # loom-daemon (HTTP/HTTPS/WSS hub, outbound /mcp, LAN & bridge host)
│   ├── cli/                   # loom (operator HTTP CLI & mcp-stdio bridge)
│   ├── plugin-cli/            # loom-plugin (standalone Plugin SDK CLI)
│   ├── art-store/             # loom-art-store (multi-kind package store)
│   ├── ocr-host/              # neuro.official/ocr capability runtime
│   ├── text-translation-host/ # neuro.official/text-translation capability runtime
│   └── desktop/               # React + @neuro/ui workbench & Windows Tauri shell (Loom.exe)
├── crates/                    # Reusable runtime, protocol, security, MCP, workflow & UI crates
├── protocol/                  # Public JSON Schemas, fixtures, and wire specifications
├── sdk/                       # Authoring types and templates (capability, surface, otter-art)
└── docs/                      # Architecture, security, development, and subsystem manuals
```

## Build editions and client/server modes

Loom supports three deployment editions from a single codebase:

1. **Desktop (`-Edition desktop`)**: Windows `Loom.exe` Tauri workbench paired with `runtime\loom-daemon.exe` and independent Framework, MCP server, and sample Art ZIP catalogs. `Loom.exe` can auto-start its local sidecar daemon on `http://127.0.0.1:8765` (**Local mode**) or connect to a remote `loom-daemon` hub (**Remote client mode**).
2. **Server Web (`-Edition web` / Docker default `LOOM_EDITION=web`)**: `loom-daemon` compiled with `management-ui` (`loom_embedded_ui`), serving the full browser management console at `/ui/` alongside `loom` CLI—ideal for Linux/NAS/home servers without Tauri dependencies.
3. **Server API (`-Edition api` / Docker `LOOM_EDITION=api`)**: minimal headless `loom-daemon` (`runtime-defaults` only, `/ui/*` disabled) plus `loom` CLI.

### Quick start (CLI, daemon, and desktop)

Build the core Rust binaries:

```powershell
cargo build --locked -p loom-daemon -p loom-cli
```

Run an isolated local daemon and query it with `loom`:

```powershell
$env:LOOM_DAEMON_HOST = "127.0.0.1"
$env:LOOM_DAEMON_PORT = "48766"
.\target\debug\loom-daemon.exe
```

```powershell
.\target\debug\loom.exe status --daemon-url http://127.0.0.1:48766
.\target\debug\loom.exe frameworks list --daemon-url http://127.0.0.1:48766
.\target\debug\loom.exe arts list --daemon-url http://127.0.0.1:48766
.\target\debug\loom.exe workflows list --daemon-url http://127.0.0.1:48766
.\target\debug\loom.exe mcp list --daemon-url http://127.0.0.1:48766
.\target\debug\loom.exe ai status --daemon-url http://127.0.0.1:48766
```

Non-public routes require `Authorization: Bearer <token>` (from `LOOM_DAEMON_TOKEN` or auto-generated `<control-plane>/daemon-token`) or a scoped `Authorization: Device <session>` / MCP share token where permitted. Non-loopback binds require an authenticated TLS terminator (`LOOM_TLS_TERMINATED=1`) or explicit LAN management configuration.

Install, test, and build the frontend workbench (`apps/desktop`):

```powershell
Push-Location .\apps\desktop
npm ci
npm test
npm run typecheck
npm run build
Pop-Location

cargo check --locked --manifest-path .\apps\desktop\src-tauri\Cargo.toml
```

## Frameworks, Arts, Capabilities, and MCP packages

Loom treats Art execution engines as installable Framework packages (`process`, `cloud_api`, `mcp`, `workflow`, or third-party frameworks implementing `loom.framework.v1`). None of these packages are statically linked into `loom-daemon` or `Loom.exe`, and a fresh standalone daemon starts with zero pre-installed frameworks. In packaged desktop releases, independent ZIPs under `packages\frameworks`, `packages\mcp-servers`, and `packages\arts` are verified against `.sha256` sidecars and bootstrapped once per catalog hash into the writable control plane via the public install APIs.

Build or install repo-owned packages explicitly with:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\Build-LoomArtFrameworkPackages.ps1 -OutputRoot .\.loom-art-store-data\frameworks
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\Build-LoomSampleArtPackages.ps1 -OutputRoot .\.loom-art-store-data\arts
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\Install-LoomSampleArtPackage.ps1 -PackageName image-compress
```

- **Cross-platform MCP server packages**: `neuro.official/neuro-image-search` (`runtime/image-search-mcp.ps1` on Windows, `runtime/image-search-mcp.mjs` on Linux; see [Image Search](docs/IMAGE_SEARCH.md)) and `neuro.official/stock-api` (`=2.9.0` with pinned Node runtime; see [Stock Monitor](docs/STOCK_MONITOR.md)) store API credentials exclusively in server-scoped `CredentialStore` bindings (`PUT /v1/mcp/servers/{id}/credentials`).
- **Outbound MCP server & Scoped Share Tokens**: external MCP clients can connect to `POST /mcp` or spawn `loom mcp-stdio --daemon-url <url> --mcp-token <loom_mcp_...>` using scoped tokens created via `POST /v1/mcp-share/tokens` (`arts:execute`, `capabilities:invoke`, `screen:read`, `devices:control`, `projection:control`).
- **Official OCR & Translation Capabilities**: `neuro.official/ocr` (text OCR + QR/barcode recognition; see [OCR contract](docs/OCR_CONTRACT.md)) and `neuro.official/text-translation` (see [translation hybrid mode](docs/TRANSLATION_HYBRID_MODE.md)) install as signed, digest-approved Capability Plugins without bundling model weights into the base Loom installer.
- **Plugin SDK & Trust**: `Loom-Plugin-SDK-<version>-windows-x64.zip` ships `loom-plugin.exe`, all public v1 JSON Schemas, templates, and conformance harnesses. See [plugin development](docs/plugin-development.md), [plugin security](docs/plugin-security.md), [plugin permissions](docs/plugin-permissions.md), and [signing and trust](docs/plugin-signing-and-trust.md).

## Global AI Gateway relay and durable runs

Configure an OpenAI-compatible AI Gateway via `LOOM_GATEWAY_*` environment variables or dynamically via `PUT /v1/ai/config` (loopback HTTP/HTTPS, private LAN RFC1918/ULA HTTP/HTTPS, or remote HTTPS):

```powershell
$env:LOOM_GATEWAY_MODEL = "planner-model"
$env:LOOM_GATEWAY_BASE_URL = "http://127.0.0.1:4200"
$env:LOOM_GATEWAY_TOKEN = "<gateway bearer token>"
$env:LOOM_GATEWAY_TIMEOUT_SECS = "60"
.\target\debug\loom-daemon.exe
```

Direct relay endpoints (`GET /v1/ai/status`, `GET /v1/ai/models`, `POST /v1/ai/chat/completions`) and unified capability execution (`POST /v1/invoke` with `ai.chat.v1` or `loom.runtime.probe.v1`) run on bounded concurrent workers (`LOOM_DAEMON_WORKERS=4`, `LOOM_DAEMON_QUEUE_CAPACITY=32`). Run lifecycles (`GET /v1/runs/{id}`, `GET /v1/runs/{id}/events`, `POST /v1/runs/{id}/stop`, `POST /v1/runs/{id}/retry`) persist atomically in `<LOOM_CONTROL_PLANE_ROOT>\runs\loom-runs.sqlite3` without logging prompts or upstream bearer tokens. See [Gateway integration](docs/GATEWAY_INTEGRATION.md).

## Otter, multi-device sync, and Companion App Settings Hub

- **Otter Canvas & Multi-Device Sync**: inspect live Otter graphs (`GET /v1/hook-bridge/canvas`), track active workflow execution across paired devices, and archive/restore up to 200 reusable workflows (`/v1/otter-sync/*`).
- **Device Hierarchy & Display Language**: view Host Devices and their nested App Sessions (`GET /v1/devices/hierarchy`) and update an approved online Otter's display language (`POST /v1/devices/{id}/locale-requests`) with revision-checked confirmation.
- **Companion App Settings Hub**: centralize configuration claims (`GET /v1/configuration/claims?app=tea`) and schema-driven revisioned settings (`GET/PUT /v1/configuration/companion-apps/{appId}`) for companion desktop tools. See [Otter integration](docs/OTTER_INTEGRATION.md).

## Container image and release packaging

Build and run the server container (`LOOM_EDITION=web` by default, or `--build-arg LOOM_EDITION=api`):

```powershell
docker build -t loom:local .
docker run --rm -p 48766:8765 loom:local
```

Build and verify standalone Windows desktop or server release packages:

```powershell
.\scripts\build-release.ps1 -VersionId local-release
.\scripts\verify-release.ps1 -PackageDir .\release\Loom\local-release

# Server editions (web or api):
.\scripts\build-release.ps1 -Edition web -VersionId local-web
.\scripts\build-release.ps1 -Edition api -VersionId local-api
```

Passing `-RunSmoke` to `verify-release.ps1` executes the full packaged Windows smoke suite (`smoke-release.ps1`, `Invoke-LoomOtterCanvasUiSmoke.ps1`, `Invoke-LoomOtterErrorPreviewSmoke.ps1`, `Invoke-LoomFrameworkArtStoreOtterSmoke.ps1`, `Invoke-LoomPluginBoundarySmoke.ps1`, `Invoke-LoomSurfacePrototypeSmoke.ps1`, and `Invoke-LoomAuthoredArtCreationSmoke.ps1`). See [release provenance](docs/release-provenance.md).

## Validation

```powershell
cargo fmt --all -- --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace
node .\scripts\effective-code-lines.mjs --mode strict --json .\.tmp\effective-code-lines.json
```

## Documentation

- [Documentation index](docs/README.md)
- `docs/DEVELOPMENT.md`
- `docs/DEPENDENCY_SECURITY.md`
- `docs/ARCHITECTURE.md`
- `docs/OTTER_INTEGRATION.md`
- `docs/WORKFLOW_CONTRACT.md`
- `docs/GATEWAY_INTEGRATION.md`
- `docs/ASSET_LIBRARY_INTEGRATION.md`
- `docs/DOWNLOAD_ENGINE.md`
- `docs/RUNTIME_SECURITY.md`
