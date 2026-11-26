# AssetLibrary, NAccount, and Unified Store Integration

Loom integrates with NAccount (`https://naccount-auth.aiaimimi.com`) and the AssetLibrary Workers service (`https://assetlibrary.aiaimimi.com`) alongside Loom's local multi-kind package store (`loom-art-store` and `GET /v1/store/catalog`). All cloud tokens, download tickets, and cryptographic package verifications stay inside `loom-daemon`; WebViews and external callers receive only sanitized status, catalog DTOs, and progress updates.

## 1. NAccount native authentication (`/v1/naccount/*`)

NAccount credentials are stored independently from Neuro Platform projection-device credentials (`/v1/account/*` in [ACCOUNT_LOGIN.md](../protocol/ACCOUNT_LOGIN.md)):

- **OIDC issuer**: `https://naccount-auth.aiaimimi.com`
- **Public SPA client**: `Loom Desktop` (`LOOM_NACCOUNT_CLIENT_ID` overrides the default client ID only when registered for the exact same loopback callback and accepted by AssetLibrary).
- **Loopback callback**: `GET http://127.0.0.1:8765/v1/naccount/callback`. Starting a login when the daemon is not bound to `http://127.0.0.1:8765` fails closed with `409 / naccount_callback_port_required`. Top-level browser OAuth redirects (`Sec-Fetch-Site: cross-site` document navigation) are permitted solely on this exact GET callback route for PKCE `state`/`code` verification.
- **Flow**: Authorization Code + `S256` PKCE (`openid profile offline_access`, 10-minute pending state TTL). The daemon exchanges the authorization code, resolves subject identity (`authId`) via `/oauth2/v1/userinfo`, and persists tokens under `<control-plane>/naccount-login` via `CredentialStore` (DPAPI on Windows).
- **Admin control routes**: `POST /v1/naccount/status`, `POST /v1/naccount/start`, `POST /v1/naccount/refresh`, and `POST /v1/naccount/logout` (body `{}`). `logout` attempts remote refresh-token revocation before clearing local credentials and cancels any in-flight AssetLibrary downloads bound to that session.

## 2. AssetLibrary catalog, downloads, and native installation (`/v1/asset-library/*`)

All local `/v1/asset-library/*` routes require Loom administrator authentication:

| Local Route | Request Body | Upstream / Runtime Behavior |
| --- | --- | --- |
| `POST /v1/asset-library/catalog` | `{ "after": "" }` | Reads public `GET /v1/catalog` publications (`nextAfter` cursor pagination). |
| `POST /v1/asset-library/resources` | `{ "after": "" }` | Reads signed-in author `GET /v1/me/resources` (`nextCursor` pagination). |
| `POST /v1/asset-library/library` | `{ "after": "" }` | Reads entitled `GET /v1/me/library` items. |
| `POST /v1/asset-library/create` | `{ "operationId": "<uuid>", "kind": "art\|capability\|application", "title": "..." }` | Creates upstream resource metadata via `POST /v1/resources` with `Idempotency-Key`. |
| `POST /v1/asset-library/download` | `{ "publicationId": "<uuid-v4>", "taskId"?: "<uuid-v4>" }` | Resolves publication metadata, requests a short-lived download ticket, and streams verified bytes into `<control-plane>/asset-library-downloads` via the [download engine](DOWNLOAD_ENGINE.md). |
| `POST /v1/asset-library/download-control` | `{ "taskId": "<uuid-v4>", "action": "status\|pause\|resume\|cancel" }` | Controls active download tasks (up to 8 concurrent tasks). |
| `POST /v1/asset-library/install` | `{ "publicationId": "<uuid-v4>", "kind": "art\|capability\|mcp", "sha256": "<hex>", "taskId"?: "<uuid-v4>" }` | Re-queries publication metadata, verifies `loom-{kind}-zip-*` format policy and SHA-256, downloads, re-verifies on disk, and invokes Loom's native package installer. |

### Native installation invariants

- `/v1/asset-library/install` accepts `kind` values `art`, `capability`, and `mcp`. It never accepts caller-supplied local file paths, `bundledCatalog` flags, automatic permission grants, or automatic plugin activation.
- Cloud scan metadata never substitutes for publisher trust, Ed25519 signature validation, framework readiness, or host permission approval:
  - **Arts** install through `install_art` with an empty bundled-allowlist and broadcast updated `loom.hook.v1` capabilities.
  - **Capabilities** install into `<control-plane>/capabilities/<publisher>/<id>/` without auto-enabling or bypassing digest-bound permission approval.
  - **MCP server packages** install through `install_mcp_server_package`, preserving existing server-scoped `CredentialStore` bindings and enabled state across version upgrades while rolling back `active` pointers atomically if configuration validation fails.
- Standard `art`, `capability`, and `mcp` archives are capped at 8 MiB; `application-large-zip-format-v1` archives allow up to 96,000,000 bytes for standalone portable software downloads (without performing live self-replacement of a running Loom process).

## 3. Unified multi-platform store (`GET /v1/store/catalog` & `loom-art-store`)

In addition to cloud AssetLibrary publications, Loom provides a unified multi-platform catalog endpoint for local and self-hosted ecosystems:

- `GET /v1/store/catalog` (admin-authenticated) returns:
  - `hostPlatform` (`windows-x64`, `windows-arm64`, `linux-x64`, `linux-arm64`, `macos-x64`, `macos-arm64`, or `universal`) and `supportedPlatforms`;
  - `installed`: unified inventories of installed `arts`, `capabilities`, and `mcpServers`;
  - `remote`: optional catalog entries fetched from `LOOM_ART_STORE_URL` (`apps/art-store`, default `http://127.0.0.1:8790/catalog`), supporting `art`, `capability`, and `mcp_server` package kinds and platform compatibility matrices.

## 4. Native package ZIP path compatibility

AssetLibrary preflight and Linux hosts reject backslashes in ZIP entry names (`ZIP_PATH_INVALID`). Repository package builders (`scripts/Build-LoomSampleArtPackages.ps1`, `scripts/NativePackageArchive.ps1`) write file-only ZIP entries with normalized forward-slash (`/`) relative paths after Ed25519 signing, using `FileMode::CreateNew`, refusing symlinks/reparse points, and preserving exact payload and manifest bytes. Verify archive conformance with:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\Test-NativePackageArchive.ps1 -OutputRoot .\.tmp\native-package-archive-test
```

## 5. NAccount 3-tier Cloud Relay (`/v1/cloud-relay/*`)

When signed in with NAccount, Loom supports authenticated three-tier Internet projection relay (`Otter -> Loom 1 -> NAccount Cloud Relay -> Loom 2 -> Otter`) and optional remote MCP tunneling (`apps/daemon/src/cloud_relay.rs`):

- `GET /v1/cloud-relay/status`: returns relay enablement, configured `relayUrl`, projection/MCP exposure flags, and signed-in NAccount readiness.
- `PUT /v1/cloud-relay/config` (admin-only): persists `settings/cloud-relay.json` (`enabled`, validated HTTPS/loopback `relayUrl`, `exposeMcp`, `exposeProjectionRelay`).
- `POST /v1/cloud-relay/projection/send` & `GET /v1/cloud-relay/projection/inbox`: exchanges bounded (512 KiB max, 128-envelope ring) NAccount-scoped projection envelopes across paired Loom hubs.
