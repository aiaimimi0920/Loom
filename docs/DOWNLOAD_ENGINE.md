# Loom Download Engine

Loom manages authenticated AssetLibrary downloads through a daemon-owned download subsystem (`apps/daemon/src/asset_library/`). Remote URLs, bearer tokens, and short-lived download tickets remain inside the daemon process and are never exposed to WebViews, CLI arguments, or external child processes.

## Runtime backends

Loom selects a platform-appropriate download backend automatically:

1. **Windows x64 in-process `libaria2` (`aria2_native.rs`)**:
   - `native/libaria2/bridge.cpp` exposes a narrow C ABI over `aria2` `1.37.0` built without BitTorrent, Metalink, WebSocket, NLS, or post-download hook execution, using native Windows Schannel TLS.
   - A dedicated download worker thread loads `runtime/download-engine/loom_aria2.dll` after verifying its absolute path, regular-file status, byte size, and pinned SHA-256 from `security/libaria2-runtime.json`.
   - While loaded, Loom holds a share-read file handle that denies concurrent writes or deletion, restricts DLL dependency resolution to system directories, initializes the library once, drains session events per task, and cleanly shuts down the library on normal daemon exit.
   - No `aria2c.exe` child process or JSON-RPC listener is started on Windows.
2. **Linux and portable fallback (`aria2_portable.rs`)**:
   - Non-Windows hosts use an in-process bounded `reqwest` HTTP downloader, with Range resume and the same loopback relay and digest verification contract. It does not launch `aria2c`.

## Localhost ticket relay and security boundary

- All remote AssetLibrary byte fetches are mediated by a short-lived loopback relay (`aria2_relay.rs`).
- Only validated `Range` and `If-Range` headers are forwarded; remote HTTP redirects are rejected, proxy auto-discovery and `.netrc` loading are disabled, and each task uses up to four connections in a bounded sequential queue.
- Standard package downloads (`art`, `capability`, `mcp`) enforce an 8 MiB payload ceiling and a 75-second transfer deadline; the Windows native wrapper has a 90-second result-wait budget. Dedicated `application-large-zip-format-v1` archives allow up to 96,000,000 bytes with chunked SHA-256 verification and exclusive file-handle staging.
- Completed downloads are verified against both exact byte length and expected SHA-256 before atomic handoff to the target directory or package installer. Cross-process persistent resume is not enabled.

## Download control API

Administrators control active downloads through:

```http
POST /v1/asset-library/download-control
Authorization: Bearer <admin-token>
Content-Type: application/json

{
  "taskId": "<uuid-v4>",
  "action": "status | pause | resume | cancel"
}
```

- `/v1/asset-library/download` and `/v1/asset-library/install` accept an optional `taskId` (UUID v4) and allocate one automatically when omitted.
- Up to eight active tasks are tracked per daemon instance, scoped to the control-plane root and current NAccount token digest.
- `cancel` is terminal: once a task completes length and SHA-256 verification it is atomically sealed so a late cancel cannot corrupt a finished file, whereas an in-flight cancel deletes only the two known task-staging files without recursive directory deletion.
- Logging out of or refreshing the NAccount session automatically cancels in-flight downloads associated with the previous session.

## Build, packaging, and GPL source-delivery policy

- `security/libaria2-runtime.json` pins the upstream `aria2-1.37.0` archive digest, `loom_aria2.dll` SHA-256, and `releaseApproved` gate.
- `scripts/Build-LoomLibAria2.ps1` compiles the bridge and DLL in an isolated WSL/MinGW directory and emits `build-receipt.json` (binding `build.sh`, `bridge.cpp`, `link.map`, compiler metadata, and output SHA-256).
- `scripts/Install-LoomLibAria2.ps1` verifies the build receipt and deploys `loom_aria2.dll`, `COPYING`, and metadata into `runtime/download-engine/`, which is tracked by release `supportFiles`, `manifest.json`, `checksums.sha256`, and top-level SBOMs.
- Because `aria2` is licensed under `GPL-2.0-or-later`, any combined distribution bundling `loom_aria2.dll` must provide complete corresponding source materials rather than relying on dynamic linking to waive GPL obligations:
  - Loom source snapshot (`scripts/source-delivery.mjs`, exporting an exact manifest-verified worktree allowlist free of credentials or user data);
  - `aria2` `1.37.0` upstream archive, `COPYING`, `native/libaria2/bridge.cpp`, `native/libaria2/build.sh`, and `native/libaria2/SOURCE_BUILD.md` (`scripts/New-LoomLibAria2SourceBundle.ps1`);
  - pinned MinGW/GCC static runtime and toolchain source archives (`native/libaria2/toolchain-sources.json` and `scripts/Get-LoomNativeSourceMaterials.ps1`).
- Loom's own source files retain their `MIT OR Apache-2.0` dual license. Formal release packaging refuses unapproved `libaria2` builds while `releaseApproved` is `false`; isolated internal candidates must pass `-DevelopmentOnly` with an `internal-` version prefix (see [release provenance](release-provenance.md)).
