# Phase 64: Python Art Color Transfer installable art

## Status

Complete.

## Why this phase exists

`Color Transfer (RBF)` still exists in the live Hook/Loom control-plane, but it
is currently hanging off the old ArtLoom-compatible script path instead of the
new installable framework line.

That leaves three concrete gaps:

- the node is not yet packaged as a formal Loom-managed `python_art` Art;
- Hook shader prefetch still needs to align with Loom's python-art runtime
  instead of silently depending on an ambient local Python; and
- there is no repo-owned installer path that provisions the framework runtime,
  art payload, and Color Transfer dependencies under Loom's control-plane.

This phase closes that gap by moving Color Transfer onto the installable
`python_art` framework while preserving the existing user-facing Art id and
shader-style editing flow.

## Implemented

- Added installed-`python_art` path rewriting in
  `crates/loom_tool_registry/src/install.rs` so Loom now rewrites:
  - `execution.type = "python_art"` -> absolute control-plane `artPath`
  - Hook-facing compat metadata `artPath`
  - Hook-facing compat metadata `pythonPath`
- Added a regression test:
  - `installs_python_art_package_rewrites_art_paths_for_runtime_and_hook_compat`
- Added Hook-side installed-art precedence and Loom-first shader prefetch in
  `Hook/src-tauri/src/mock_artloom.rs`:
  - Hook now prefers a Loom-installed control-plane Art over a colliding legacy
    ArtNexus-local definition with the same id.
  - `prefetch_shader` now tries Loom's
    `/v1/python-arts/shader/prefetch` route before falling back to local Python.
- Added a Hook regression test:
  - `merge_prefers_loom_control_plane_art_over_legacy_local_collision`
- Added repo-owned installer:
  - `scripts/Install-LoomColorTransferArt.ps1`
- The installer now:
  - vendors `Art_ColorTransfer` from the reviewed ArtLoom source;
  - downloads Windows CPython 3.12 wheels for `numpy==1.26.4` and
    `Pillow==11.1.0`;
  - stages a portable `python_art.zip` runtime bundle;
  - builds the formal Art ZIP for the production tool id
    `custom-1770131241684`;
  - publishes both ZIPs into `.loom-art-store-data`;
  - installs the framework runtime into
    `%APPDATA%\Loom\control-plane\framework-runtimes\python_art`; and
  - installs the Art into
    `%APPDATA%\Loom\control-plane\arts\custom-1770131241684`.
- Updated `apps/daemon/src/lib.rs` so shader-prefetch compatibility now
  unwraps raw shader payloads instead of returning text-wrapped JSON, which
  keeps Hook's shader preview contract aligned with the installed
  `python_art` runtime path.
- Updated `README.md` with the repo-owned rebuild/install path for Color
  Transfer.

## Verification

Commands run:

```powershell
cargo test --manifest-path Loom\Cargo.toml -p loom_tool_registry `
  installs_python_art_package_rewrites_art_paths_for_runtime_and_hook_compat `
  -- --nocapture

cargo test --manifest-path Loom\Cargo.toml -p loom-daemon `
  unwrap_prefetch_shader_payload_promotes_text_wrapped_shader_json `
  -- --nocapture

cargo test --manifest-path Loom\Cargo.toml -p loom-daemon `
  daemon_hook_bridge_executes_mcp_backed_art_node `
  -- --nocapture

cargo test --manifest-path Hook\src-tauri\Cargo.toml `
  merge_prefers_loom_control_plane_art_over_legacy_local_collision `
  -- --nocapture

npm --prefix Hook test -- __tests__/integration/ColorTransferShaderContract.test.ts

npm --prefix Hook run typecheck

powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\Loom\scripts\Install-LoomColorTransferArt.ps1

powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\scripts\build-release.ps1 `
  -Apps Hook `
  -VersionId 20260730-color-transfer-python-art-r2 `
  -Force

powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\Loom\scripts\build-release.ps1 `
  -VersionId 20260730-color-transfer-python-art-r2 `
  -OutputRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom

powershell -NoProfile -ExecutionPolicy Bypass `
  -File .\Loom\scripts\verify-release.ps1 `
  -PackageDir C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260730-color-transfer-python-art-r2 `
  -RunSmoke
```

Additional live local proof:

- Reinstalled the production Color Transfer Art into the real Loom
  control-plane.
- Started a temporary debug daemon on `http://127.0.0.1:8878`.
- Called `POST /v1/tools/custom-1770131241684/readiness` and confirmed:
  - `frameworkInstalled = true`
  - `ready = true`
  - `framework = "python_art"`
- Called `POST /v1/python-arts/shader/prefetch` with:
  - `artId = custom-1770131241684`
  - installed control-plane `artPath`
  - real `source.png` and `reference.png`
- Confirmed the response returned:
  - `compatCommand = "prefetch_shader"`
  - `result.type = "shader"`
  - non-empty `vertex_shader`
  - non-empty `fragment_shader`
  - `textures.lut` beginning with `data:image/png;base64,`
  - `uniforms.strength = 100.0`

Results:

- The installed tool contract now resolves to the Loom-managed control-plane
  Color Transfer Art instead of a stale legacy path.
- Hook-side shader prefetch compilation was restored by enabling
  `reqwest`'s `blocking` feature for the new Loom HTTP fallback path.
- Hook's MCP/text-output Art-node success path regression was repaired during
  final packaged smoke validation, so the formal release verification chain is
  green again.
- Formal standalone package verification passed with:
  - `filesChecked = 32`
  - `smoke = passed`
  - `hookCanvasSmoke = passed`
  - `hookErrorPreviewSmoke = passed`
  - `frameworkArtStoreHookSmoke = passed`

## Release outputs

- Loom standalone package:
  - `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260730-color-transfer-python-art-r2`
- Hook package:
  - `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Hook\20260730-color-transfer-python-art-r2`
