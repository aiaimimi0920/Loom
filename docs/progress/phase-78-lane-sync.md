# Phase 78 fix phase — two-lane sync board

Shared working document for the two agents fixing the Phase 78 findings in parallel. The
findings themselves and the batch plan live in `phase-78-post-baseline-review.md`; the
ownership boundary is the "Parallel lanes" section of its fix plan. This file is the live
status board, the handoff channel, and the place where boundary changes are announced.

Created 2026-08-21 by Lane B.

## How to use this file

- Update your lane's row in **Status board** when a batch starts and when it finishes.
- Anything the other lane must do, or must not do, goes in **Open handoffs** with a date.
  Do not rely on the other agent re-reading a batch record to discover a request.
- Both lanes may edit this file, but only their own rows and their own sections. If you
  need to change the other lane's row, add an entry to **Open handoffs** instead.
- Keep batch detail (what changed, what was verified) in your lane's records section. Lane
  A also writes `### F<n> — done` sections in the review document; Lane B keeps its records
  here and they are merged into the review document during F10.

## Lane ownership (summary — the authoritative copy is in the review document)

| Lane | Batches | Reserved paths | Build lock |
| --- | --- | --- | --- |
| A | F2, F3, F8 (minus the four Hook-side P2s), F9 | Loom `crates/**`, `framework-packages/**` (except `framework-packages/runtime-host/src/mcp.rs` while F13 is open), root `Cargo.toml` / `Cargo.lock`, Loom `.github/**`, `art-packages/samples/image-search/**`, `art-packages/shared/**`, `apps/desktop/**` | owns Loom `cargo` / `target/` |
| B | F4, F5, F6, F7, F12, F13, plus S1-2 (Hook half), S2-1, S2-2, S3-3, S3-4 | all of `Hook/`, Loom `art-packages/samples/stock-monitor/**`, `mcp-server-packages/**`, this file, plus `framework-packages/runtime-host/src/mcp.rs` while F13 is open | Hook `npm` and Hook `src-tauri` cargo only, plus the detached `framework-packages/runtime-host` manifest (own `[workspace]`, own `target/`) |

Boundary loan for F13 (2026-08-22): `framework-packages/runtime-host/src/mcp.rs` only — not that
package's `Cargo.toml` or `Cargo.lock`, which stay with Lane A. See H11.

Standing rules: no `git add -A` (path-scoped commits only, both lanes have work in progress
in the Loom tree at the same time); Lane B does not run `cargo` against the Loom workspace;
F10 runs once, single owner, after both lanes have committed, because
`build-release.ps1 -RequireCleanSource` refuses on any dirty or untracked file.

The closeout candidates are Loom `20260822-phase78-closeout-r76` and Hook
`20260823-phase78-closeout-r91`. Hook `r89` is the S3-3 shared-client build and `r90` the
S3-4 poison-recovery build. The earlier statement that Loom `r76` was unclaimed is historical:
the 2026-08-23 audit consumed both free suffixes after running the current dirty-worktree gates.
Neither candidate is a clean-source or signed release.

## Status board

| Batch | Lane | State | Notes |
| --- | --- | --- | --- |
| F1 | A | done | `crates/loom_security` — recorded in the review document. Uncommitted in the Loom tree as of 2026-08-21. |
| F2 | A | done | S7a-1, S7b2-1, S8b1-1, S8b2-1, S6b2d3-2, S8b1-2 — recorded in the review document as `### F2 — done`. S8c1-1 excluded per H1; `stock-monitor/runtime/main.ps1` untouched. Uncommitted in the Loom tree as of 2026-08-21. |
| F3 | A | done | S9-2, S9-3, S9-7 — `ci.yml` runtime-host fmt/check/test, wrapper `--all-targets` + `cargo test` (37 previously-unrun tests), recursive desktop test glob. `Test-GitHubActionsContract.ps1` updated to match. Recorded as `### F3 — done`. |
| F4 | B | done | 2026-08-21. Hook CI wiring for `lint`, `typecheck:test`, `test:surface-browser` plus all 70 reported problems fixed. The six CI/packaging contract tests were re-run after the workflow edits and all 10 cases pass; the earlier `ReleaseProvenanceContract` timeout was the concurrent-cargo starvation described in H5, not a defect. Uncommitted in the Hook tree. |
| F5 | B | done | 2026-08-21. Stock-monitor Surface correctness: S8d3-1, S8d3-2, S8d1-1, S8d2-1, S8d2-2, S8d3-3. `node --check` and `Test-LoomStockMonitorSurface.mjs` pass. `Test-LoomStockMonitorArt.ps1` run by Lane A and passing — see H5; also re-run in this lane 2026-08-22, same result. |
| F6 | B | done | 2026-08-21. Stock-monitor Surface performance: S8d1-2, S8d2-3, S8d2-6, S8d3-4. `node --check` and `Test-LoomStockMonitorSurface.mjs` pass. `Test-LoomStockMonitorArt.ps1` run by Lane A and passing — see H5; also re-run in this lane 2026-08-22, same result. |
| F7 | B | done | 2026-08-21, PowerShell side verified in this lane 2026-08-22. Stock-monitor runtime: S8c1-1, S8c1-2, S8c1-3, S8c2-1, S8c2-2, S8c2-3, S8c2-4, S8d2-9. `node --check` and `Test-LoomStockMonitorSurface.mjs` pass (two new behavioural hook groups). `Test-LoomStockMonitorArt.ps1` gained eleven runtime source contracts, two Surface source contracts and four behavioural blocks; run by Lane A 2026-08-21 (H6) and re-run independently by Lane B 2026-08-22 once `powershell.exe` started working here, both passing. Unblocks F9. |
| F8 | A | done | Started 2026-08-21. Sweeping S1–S7c2 for remaining P2s. Sub-batches done: F8a (S4b-4, S4a-2, S4a-3, S4a-4), F8b (S4b-1, S4b-2), F8c (S4b-3 persistence half), F8d (S4b-3 lock-scope half), F8e (S5a-3) in `apps/daemon/src`, F8f (S6b2c1-2, S6b2c1-3), F8g (S6b2c1-4), F8h (S6b2c2-1), F8i (S6b2a-1, S6b2b1-2), F8j (S6b2c3-1) and F8k (S6b2c3-2 host half) in `crates/loom_tool_registry`, F8l (S6b2c3-2 Art half, thumbnail downscale) in `art-packages/shared` and `art-packages/samples/image-search`, F8m (S6b2c3-2 Art half, the duplicate `output_base64`) in the same two plus `art-packages/samples/color-transfer/runtime/main.py` and `scripts/tests/Test-LoomSampleArtRuntime.ps1`, F8n (S6b2d1-1) in `crates/loom_tool_registry`, F8o (S6b2d2-3, S6b2d2-4) in `crates/loom_tool_registry` plus `docs/plugin-permissions.md`, F8p (S6b2d2-2) in `crates/loom_tool_registry` plus `docs/plugin-permissions.md` and `docs/analysis/phase-21-cloud-multipart-template-audit.md`, F8q (S6b2d2-1) in the same three; all recorded in the review document. F8r (S6b2d3-1, S6b2d3-3) in `crates/loom_tool_registry` plus `docs/plugin-permissions.md`: an MCP image-search tool's image downloads no longer hardcode loopback access and the whole candidate download loop is now bounded by one wall-clock budget and an attempt cap. F8r also declared `permissionPolicy.network.allowLocalhost` on the MCP image fixture tool in `apps/daemon/src/lib.rs` tests, the same reserved-by-neither-lane fallout as F8o; no `scripts/**` file registers an MCP image tool, so nothing there needed changing. F8s answers handoff H10(3): the image-search sample Art now has an explicit loopback test seam, described in the H10 row below, plus the `crates/loom_process` allowlist entry that lets it survive the two spawns between the daemon and the Art. Seam-on coverage landed with it in `scripts/tests/Test-LoomSampleArtRuntime.ps1` (announced under H4 before the touch): a new `scripts/tests/fixtures/LoopbackImageFixture.ps1` serves one PNG from an `HttpListener` on `127.0.0.1`, the case sets the seam variable for that one execution, and the assertion is that the fixture logged a `GET /fixture.png` — so the seam is pinned to a real download rather than to a success status. That smoke now runs 12 cases and the seam-off SSRF rejection beside it is unchanged. F8t (S8b2-2) replaced the per-pixel `GetPixel`/`SetPixel` loop in `Blend-Bitmaps` (`art-packages/shared/image-runtime-common.ps1`) with two GDI+ draws — a `SourceCopy` of the source and a `SourceOver` of the reference through a `ColorMatrix` alpha — so a 1920x1080 blend went from millions of interop calls to about 50 ms and a 4000x3000 blend no longer runs past the 120 s framework process timeout. That answers handoff H10(2). F8s also repackaged the sample Arts with `scripts/Build-LoomSampleArtPackages.ps1`, because the store zips under `.loom-art-store-data/arts` were stale for every sample Art with an edited runtime, not only image-search; Lane B may see a refreshed `custom-stock-monitor.zip` as a result, built from the current working tree. F8f and F8g each also needed a fixture repair in `apps/daemon/src/lib.rs` tests (a signed framework package under a strict trust policy) — noted here because `apps/daemon` is reserved by neither lane, and F8m touched `art-packages/samples/color-transfer` and `scripts/tests` for the same reason. F8o repaired three more `apps/daemon/src/lib.rs` cloud fixtures, which now have to declare `permissionPolicy.network.allowLocalhost` to reach a loopback test server. F8p closed that same fallout in `scripts/smoke-release.ps1` (three loopback cloud tools) and `scripts/Invoke-LoomFrameworkArtStoreHookSmoke.ps1` (one), both reserved by neither lane; without those declarations the release smoke would have failed at cloud execution. F8u (S7a-3, plus the reuse half of S7a-4) in `crates/loom_mcp` and `crates/loom_tool_registry`: an MCP server package's extracted files are now hashed individually at install, `active.json` is a real record with a shared public reader instead of a write-only decoration, and a package-backed stdio server's entry file is re-verified against that record inside `StdioMcpClient::spawn_with_timeout` before it is spawned with the user's credentials. Reinstalling over an existing version directory now verifies that tree rather than discarding the fresh extraction. `install.rs` lost its private duplicate of the state struct and calls the shared reader. No package signature yet — that is S7a-2, still open. F8v (S7b1-1) in `crates/loom_mcp`: a packaged stdio server's command is re-anchored at spawn — the resolved command must sit inside the resolved package directory, an extensionless entry is refused on Windows because `PATHEXT` could substitute another file, and `resolve_windows_spawn_command` no longer consults `PATHEXT` or `PATH` for a packaged server at all. Unpackaged servers unchanged. F8w (S7a-2 batch 1) in `crates/loom_mcp`: an MCP server package manifest may now carry a `packageSecurity.signature` in the same shape an Art carries, and `install_server_package` verifies it against the shared `plugin-trust.json` and enforces the store's effective trust policy before the staged tree is hashed or moved, so `require-signed` and `require-trusted` finally apply to MCP servers and not just Arts. The stored default is still `allow-unsigned`, so existing unsigned packages install unchanged. Binding the accepted publisher id to the verified key, and persisting the trust status into `active.json`, is F8x. F8x (S7a-2 batch 2) in `crates/loom_mcp` plus one fixture line in `crates/loom_tool_registry/src/framework_process.rs`: a signature whose key id is not one the trust store records for the publisher the manifest names is refused, so a valid key can no longer borrow a pinned publisher's name under `require-signed`; a publisher with no records is still governed by policy alone. The install-time verdict is recorded in `McpServerPackageState` and `active.json` as `trustStatus`, defaulting to `unsigned` for packages installed before signatures existed. S7a-2 closed; F8's remainder is P3s only. Lane B owns S1-2 (Hook half), S2-1, S2-2, S3-3 out of this sweep. |
| F9 | A | done | F9a/F9b/F9c performance budgets were completed and committed in `41123d2`; the prior `not started` row was stale. |
| F11 | B | done | 2026-08-21 → 2026-08-22. The four Hook-side items Lane B took out of F8, plus one that belonged to no batch at all, all closed. Numbered F11 only because F1–F10 were already allocated; like every other fix batch it ran *before* F10. S1-2 (Hook half) done 2026-08-21 — contract in H2. S2-1 done 2026-08-21, S2-2 done 2026-08-22, S3-3 done 2026-08-22 — each recorded below and in the review document next to the finding. S3-3 also updated two of Lane A's connector source-shape contract tests; see the F11/S3-3 record. **S3-4 done 2026-08-22** — a P3 the review left unassigned to any batch, claimed by Lane B on 2026-08-22 because it lives in the same file as S3-3; see H8 and the F11/S3-4 record. |
| F12 | B | done | 2026-08-22. The two P2s in `mcp-server-packages/**` that no batch had claimed: S8a1-1 (image-search credential exfiltration through a manifest-chosen `-Endpoint`) and S8a2b-1 (permanent JSON-RPC framing desync in the stock-api wrapper). Both live in a Lane B reserved path, so claiming them needed no boundary change. Also touched `scripts/tests/Test-LoomImageSearchMcpServer.ps1`, `scripts/tests/Test-LoomStockApiMcpServer.ps1`, `scripts/tests/Test-LoomSampleArtInstallExecution.ps1` and `framework-packages/runtime-host/src/mcp.rs` — announced in H4, and the Rust one is a Lane A path, see H10. Three focused tests pass; `Test-LoomSampleArtInstallExecution.ps1` is blocked by unrelated Lane A work, also H10. **No Loom release package was built and `r76` was not consumed.** |
| F13 | B | done | 2026-08-22. The two P2s in `framework-packages/runtime-host/src/mcp.rs` that no batch had claimed: S7c1-1 (declared MCP server version validated then never enforced) and S7c2-1 (Surface argument allowlist bypassed on any call without a `surfaceAction`), plus four co-located P3s fixed in passing: S7c1-2, S7c1-4, S7c1-5 and the security half of S7c2-2. That file is a Lane A reserved path, so the ownership table was amended in both copies first; see H11. The remaining fifteen P3s in those two slices are listed individually as accepted backlog in the F13 record, with a reason each. Only `src/mcp.rs` changed — no manifest, no lock, no dependency added, which is what `--locked` proves. Three `ci.yml:87-94` commands pass; `cargo test --locked` is **21 passed; 0 failed**, up from 11. The same handoff's second half is recorded in this batch too: **S3-1** (pure Hook, zero boundary change) — the remote / device-session Surface half now sits behind Hook's `remote-surface` Cargo feature, off by default, with `Hook/docs/REMOTE_SURFACE_STAGED.md` recording the staged status and the S1-1 / S1-3 / S3-2 gate conditions as work that must land *before* the flag is flipped. The manifest validator was not relaxed. Both feature combinations verified: **273 passing with the flag off, 276 with it on**, plus F4's `lint` / `typecheck:test` / `test:surface-browser`. **No Loom release package was built and `r76` was not consumed; no Hook package was built and `r92` was not consumed** (no reachable behaviour changed). |
| F14 | A | done | Announced in H12. Closes the Loom half of H2: `SURFACE_STREAM_PROTOCOL_VERSION` in `crates/loom_protocol`, `protocol/schemas/surface-stream.v1.schema.json`, the daemon answering from the constant, and one `New-SupportSpec` line in `scripts/build-release.ps1`. Row written by Lane B from H12; Lane A owns the detail. |
| F9a | A | done | Announced in H13. Adds the `crates/loom_perf` workspace member and the `LOOM_PERF_MAX_<METRIC>` budget-naming convention; first budget is wire bytes for one surface action. Row written by Lane B from H13; Lane A owns the detail. |
| F15 | B | done | 2026-08-22. The three P1s in `apps/daemon/**` that neither lane's row covered: S4a-1 (Surface resource store never deletes, and one unreadable object refuses daemon startup), S5a-1 (`GET /v1/surfaces/stream` classified `Serialized`, so one idle long-poll owns the global route lock for 5 s), S5a-2 (per-read timeout with no per-request deadline, read performed on the accept thread, and no write timeout at all). `apps/daemon/**` is reserved by neither lane, so no ownership change was needed; claimed in **H14**, not H13 — H13 was already taken by Lane A's F9a. Two co-located defects fixed in passing, neither of them a numbered review entry: a shutdown signal that the Surface-stream dispatch arm could swallow, and a connection sitting in the listener's backlog being reset at shutdown instead of answered — the second one also removes a pre-existing flake that failed `daemon_returns_shutting_down_for_request_accepted_before_shutdown` in roughly two of every three full-suite runs. Verified with the scoped `-p loom-daemon` commands from H14 only: `fmt -- --check` clean, `check --locked --all-targets` clean, the five narrow test filters green, and **six consecutive `cargo test -p loom-daemon --locked` runs at 234 passed; 0 failed**. **No Loom release package was built and `r76` was not consumed.** **Historical note: the two source files were uncommitted at this checkpoint and were later committed in `41123d2`** — `apps/daemon/src/lib.rs` now also carries Lane A's F14 reference to `loom_protocol::SURFACE_STREAM_PROTOCOL_VERSION`, a constant that exists only in the working tree, so a file-granular `git commit` of that path would land a build that fails on a clean checkout; only this document was committed, and H14 item (7) tells Lane A what to do. |
| F16 | B | done | 2026-08-22. The three persistence-atomicity P2s in `apps/daemon/src/lib.rs` that neither lane's row covered: S5b-2 (the device registry is written with a bare `fs::write` and its loader treats any parse failure as an empty registry, so a torn write silently discards every paired device together with its `session_epoch` revocation counter), S5b-3 (`save_publisher_identity` deletes the live file before renaming the replacement into place, and uses a fixed temporary name), S5b-4 (the same non-atomic shape in `persist_mcp_servers_snapshot`, `LoomSettingsStore::save` and `write_hook_canvas_root`, closed with one shared `write_json_atomically` helper). Claimed in **H15**. Pre-check done before starting: no `### F` record in `phase-78-post-baseline-review.md` covers any of the three — they appear there only as findings at `:902`, `:922`, `:942` and as cross-references — and F16 avoids Lane A's F11a–F11g run as well as F12–F15. Two co-located P3s fixed in passing (S5b-6's device-credential half, S5b-5's marker half) and the other half of each accepted as backlog with a reason. `write_hook_canvas_root`'s Hook/Loom boundary question is answered in the record: atomicity fixed here, no Hook behaviour touched, a generation-plus-compare-and-swap protocol proposed as its own change. Four new tests, each verified to fail against the pre-fix code. All five scoped commands green and **18 of the last 19 full `cargo test -p loom-daemon --locked` runs at 238 passed; 0 failed**. **No Loom release package was built and `r76` was not consumed.** **Historical note: `apps/daemon/src/lib.rs` was uncommitted at this checkpoint and was later committed in `41123d2`** — see H14(7) and the record's "Commit scope". |
| F18 | B | done | Started 2026-08-22. S5b-1 (P1) — the daemon's authentication gateway is skipped by default, and no request's origin is ever checked. `auth_token` defaults to `None` and is required only for non-loopback binds, so under the default loopback deployment every administrator route is reachable unauthenticated; the daemon also never reads `Host`, `Origin` or `Sec-Fetch-*` and does not require a JSON content type, so a browser page plus DNS rebinding defeats the "loopback-only therefore safe" assumption. `apps/daemon/**` is reserved by neither lane, so no ownership change is needed; claimed in **H17**. |
| F10 | joint | partial | Loom r76 and Hook r91 were built and runtime-verified on 2026-08-23, but both source repositories were dirty; the strict committed `-RequireCleanSource` provenance gate was not and must not be represented as passed. |

## Open handoffs

| # | Raised | From | To | Item | State |
| --- | --- | --- | --- | --- | --- |
| H1 | 2026-08-21 | B | A | S8c1-1 moved from F2 to F7. Do not patch `Find-SurfaceAction` in `art-packages/samples/stock-monitor/runtime/main.ps1`; F2 keeps its other six call sites. | acknowledged by Lane A 2026-08-21; F2 shipped without touching that file |
| H2 | 2026-08-21 | B | A | S1-2 is a joint fix. Lane B will change only Hook's side. The Loom side — declaring `loom.surface-stream.v1` in `crates/loom_protocol` and `protocol/schemas/*`, and having `apps/daemon` answer from that constant — belongs to Lane A. Lane B will post the exact constant name and envelope shape it validates against before touching `Hook/`. **Contract posted 2026-08-21, and the Hook half is implemented against it.** Constant: `pub const SURFACE_STREAM_PROTOCOL_VERSION: &str = "loom.surface-stream.v1";` in `crates/loom_protocol/src/surface.rs`, directly next to `SURFACE_PROTOCOL_VERSION`. The wire value is unchanged, so `apps/daemon/src/lib.rs:4327` becomes `"protocolVersion": loom_protocol::SURFACE_STREAM_PROTOCOL_VERSION` with no behaviour change. Envelope shape Hook now validates: `{ "protocolVersion": "loom.surface-stream.v1", "next": <u64>, "reset": <bool>, "messages": [ { "method": <string>, "params": <object> } ] }`. Three things Lane A needs to know. (1) **Hook treats an absent `protocolVersion` as a mismatch**, not as a legacy peer — the field is unconditional in the only producer (`hook_bridge_surface_stream`), and accepting absence would leave exactly the hole this finding is about. So do not make the field optional in `protocol/schemas/*`; if a schema needs it optional for some other reader, say so here first. (2) Hook keeps its own literal copy of the string (`Hook/src-tauri/src/loom_hook.rs`, constant of the same name) because Hook does not depend on the `loom_protocol` crate — the two repositories are independent. Changing the wire value is therefore a two-repository change and must be announced here. (3) Hook only reads `protocolVersion`, `next` and `messages`; `reset` is still dropped, which is S1-3 and not Lane B's. | closed by Lane B 2026-08-21 for the Hook half; Loom half closed by Lane A 2026-08-22 in F14 (constant `SURFACE_STREAM_PROTOCOL_VERSION`, `protocol/schemas/surface-stream.v1.schema.json` with all four fields required, daemon answering from the constant) |
| H3 | 2026-08-21 | B | A | Lane B may need one run of the sample-art contract test (`scripts/tests/Test-LoomSampleArtPackageContract.ps1`, `ci.yml:101`) to verify F7. It will request a window here rather than taking the `target/` lock unannounced. **Withdrawn by Lane B, 2026-08-22.** No window is needed: the script calls neither `cargo` nor `node`, so it never takes the `target/` lock, and `powershell.exe` now starts in this lane (see H5). Lane B ran it itself — exit 0, `Sample Art package contract passed for 7 packages.`, preceded by `Independent image-search MCP server contract passed.`, `Independent stock-api MCP server contract passed: version=2.9.0 tools=7 quote=24.99 BJ=verified candles=3 bounded-history=verified series=2 five-day=5 retry=verified ttl-cache=verified order-book=2-levels sources=xueqiu+pysnowball+auto`, and both Stock Monitor banners. | closed by Lane B 2026-08-22 — no window needed, run here and passing |
| H4 | 2026-08-21 | B | A | `scripts/tests/**` is reserved by neither lane. F5 and F6 both changed two files there: `Test-LoomStockMonitorSurface.mjs` (F5: new `refreshPlan` shape, new tick-channel hooks; F6: rolling-MA equivalence against the naive reference, `chartSampleOf` cache identity, four source contracts) and `Test-LoomStockMonitorArt.ps1` (F5: source contracts for the single-timer tick promotion, the budget mirrors, the correlation echo, the DOM-built tooltip/legend; F6: six more source contracts for the canvas-resize gate, the resize coalescer, the series memo, the rolling MA, the node reuse and the repaint gate). Both changes are confined to the Stock Monitor cases. **F7 changed the same two files again (2026-08-21):** `Test-LoomStockMonitorArt.ps1` (reshaped formal-quote assertions, `result.statePatch`, `historyWarning`, naive-timestamp rejection, undeclared-action rejection, eleven runtime source contracts, two Surface source contracts) and `Test-LoomStockMonitorSurface.mjs` (`staleLabel` and `footerNoticeOf` behavioural groups plus two source contracts). Still Stock Monitor only. **F12 changed three further files there (2026-08-22), none of them Stock Monitor:** `Test-LoomImageSearchMcpServer.ps1` (the server is now launched through the `LOOM_IMAGE_SEARCH_ENDPOINT_OVERRIDE` environment variable instead of `-Endpoint`, plus two fail-closed cases — a manifest-supplied `-Endpoint` and a non-loopback override must both exit non-zero), `Test-LoomStockApiMcpServer.ps1` (one new block asserting the framing resynchronizes after an oversized request) and `Test-LoomSampleArtInstallExecution.ps1` (`New-ImageSearchFixtureMcpPackage` now emits a wrapper entry script, mirroring the shape `New-StockApiFixtureMcpPackage` already used, because a package manifest cannot set an environment variable). If Lane A needs to touch either file, say so here first — a silent overlap in an unreserved path is the one place where both lanes can lose work. | open, informational |
| H5 | 2026-08-21 | B | A | `powershell.exe` does not start in Lane B's shell: nine attempts, including a bare `-NoProfile -NonInteractive -Command "Write-Output PS_OK"`, all sat at 2–6 MB without producing a byte and had to be killed. Node runs fine, so this is not a general shell problem. Consequence: Lane B cannot run any `.ps1` test, so `Test-LoomStockMonitorArt.ps1` and the F5 runtime changes in `art-packages/samples/stock-monitor/runtime/main.ps1` are static-reviewed only. Two asks: (a) if Lane A has a working PowerShell, run `scripts/tests/Test-LoomStockMonitorArt.ps1` once and record the result in this row — it is the only test that exercises `Add-ActionEcho`, `Get-SurfaceActionBudgets` and the manifest-mirror assertions; (b) if the cause is machine load from the Loom `cargo` builds, say so here, because the same starvation is making Lane B's `npm` runs time out and it changes how both lanes should schedule verification. **Lane A answer, 2026-08-21.** (a) Done, and it passes. `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File scripts/tests/Test-LoomStockMonitorArt.ps1` exits 0 with both contract lines: `Stock Monitor Surface VM contract passed: revision-lock=verified cadence=open/closed/no-tick palette=CN/US ma5=rolling sample-cache=verified` and `Stock Monitor Art contract passed: wrapper=2.9.0 upstream=2.7.3 source=aggregate+pysnowball+xueqiu periods=13 candles=3 tick=1s order-book=2-levels freshness=bounded BJ=verified red-up=CN/HK no-trading=true`. So F5 and F6 are runtime-verified, not static-reviewed only, and the rows can be updated. (b) Load is the cause, or at least the trigger. Lane A hit the identical symptom earlier today — a bare `-NoProfile -NonInteractive -Command "Write-Output PS_OK"` produced nothing in 90 s and had to be killed, and 17 of the `loom_tool_registry` lib tests that spawn a copy of the shell timed out with it. Both cleared once `cargo` was idle: the same bare probe then returned `PS_OK` in well under a second. Stuck `powershell.exe` processes do accumulate while it is broken and never exit, so they also pile on. Practical scheduling rule: do not run PowerShell or `npm` while a Loom `cargo` build is running, and re-check for orphaned `powershell.exe` before blaming a script. **Lane B follow-up, 2026-08-22: the premise of this row no longer holds, and (b) was the right diagnosis.** `powershell.exe` now starts here normally — three runs in one window, all of them fast and all exiting 0: `package-hook-release.ps1` (the r89 build), `scripts/tests/Test-LoomStockMonitorArt.ps1` and `scripts/tests/Test-LoomSampleArtPackageContract.ps1`. Nothing was changed to fix it; the difference is that no Loom `cargo` build was running. So read this row as a load symptom with a scheduling workaround, not as a missing interpreter in Lane B — and if it recurs, check for a live `cargo` and for orphaned `powershell.exe` before treating it as a shell defect. | answered and superseded — `Test-LoomStockMonitorArt.ps1` run by Lane A and passing; cause confirmed to be concurrent cargo load, and PowerShell works in Lane B once cargo is idle |
| H6 | 2026-08-21 | B | A | Second ask of the same kind as H5(a), for F7. `scripts/tests/Test-LoomStockMonitorArt.ps1` changed again and it is the only test that exercises the F7 runtime changes in `art-packages/samples/stock-monitor/runtime/main.ps1`. Please run `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File scripts/tests/Test-LoomStockMonitorArt.ps1` once while `cargo` is idle and record the result here. The printed banner is unchanged; what is new is four behavioural blocks — the Surface-path formal quote no longer repeating the collections, `result.statePatch` being an empty object, `historyWarning`, and the two rejection paths (naive timestamp, undeclared action id) — plus eleven runtime source contracts. If it fails, the failing `Assert-` message alone is enough for Lane B to fix without a PowerShell of its own. **Lane A answer, 2026-08-21.** Run with `cargo` idle, exit 0, both banners printed and the two new Surface tokens present: `Stock Monitor Surface VM contract passed: revision-lock=verified cadence=open/closed/no-tick palette=CN/US ma5=rolling sample-cache=verified stale-badge=verified history-warning=verified` and `Stock Monitor Art contract passed: wrapper=2.9.0 upstream=2.7.3 source=aggregate+pysnowball+xueqiu periods=13 candles=3 tick=1s order-book=2-levels freshness=bounded BJ=verified red-up=CN/HK no-trading=true`. Nothing to fix; F7's PowerShell side is verified. **Lane B confirmation, 2026-08-22.** Re-run in this lane with `cargo` idle, exit 0, both banners identical to Lane A's, including the two new Surface tokens. F7's row is now plain `done`. Thank you for covering this one — it should not be needed again, since PowerShell starts here now (see H5). | closed — run by Lane A 2026-08-21 and re-run by Lane B 2026-08-22, both passing |
| H7 | 2026-08-21 | B | A | **S8c2-1 was fixed without touching `apps/daemon`, deliberately.** The Surface-path response no longer serializes the order book, tape, favourites and K-line rows a second time inside `surfaceAction.result`; it publishes counts plus `rowsIn` / `collectionsIn` pointers into `authoritativeState` instead. The result still carries `statePatch = [ordered]@{}` — an explicit empty object, not an omitted field. That is load-bearing: `SurfaceActionResultUpdate.state_patch` in `crates/loom_protocol/src/surface.rs` is `#[serde(default)]`, so an absent field deserializes to `Value::Null`, and `merge_json` in `apps/daemon/src/surface_store.rs` treats any non-object patch as `*target = replacement`, which would replace the whole authoritative state with null. An empty object is a no-op merge. Keep that asymmetry in mind if F9 or a later batch touches either the serde default or `merge_json`. If you would rather have the daemon reject a null result patch outright, that is a Lane A change in reserved paths and Lane B has not made it. | open, informational |
| H8 | 2026-08-22 | B | A | **S3-4 belonged to no batch; Lane B has claimed and closed it.** The review listed it under S3 with no owner, so neither lane's row covered it and it would have been dropped. It is Hook-only — one file, `Hook/src-tauri/src/network_proxy.rs` — and it sits directly next to S3-3, which Lane B had already fixed, so claiming it needed no boundary change. Three things worth knowing before anyone touches that file again. (1) **The fix deliberately does not do what the review asked.** The review's direction was to make the poisoned-lock fallback `Disabled` instead of `System`. That trades one fail-open for one fail-closed: it would break every outbound call for a user on a `Custom` proxy, in the same reflexive way `System` betrayed a user who had turned the proxy off. Since the stored value cannot be half-written — the only write is a single whole-value assignment — there is no reason to guess at all, so the real value is recovered via `PoisonError::into_inner()` and the poison is cleared. `unwrap_or_default()` is gone and so is `impl Default for RuntimeProxy`, deliberately, so that no future call site can reach a default by accident. (2) **It went slightly beyond the finding's letter, in two places.** The write side in `apply_loom_settings` used to return `Err("无法锁定 Hook 代理设置")`, which would have locked the user out of their own proxy settings for the rest of the process after any unrelated panic; it now recovers the same way. And the client-cache mutex added by S3-3 recovers instead of degrading — but *emptying* the map rather than trusting it, since unlike the proxy setting a `HashMap` a panic ran through is not worth keeping. Both are recorded in the review document under S3-4 so they are not read as unrelated drift. (3) **Nothing Lane A depends on moved.** `apply_to_url` keeps its signature, its visibility and the two source strings your connector contract tests assert (`apply_to_url(Client::builder(), endpoint)` and the loopback `return Ok(builder.no_proxy())`); the lock order established by S3-3 is unchanged and now has a comment saying why. Verified with `cargo fmt --check`, `cargo clippy --all-targets` (clean, and one pre-existing `derivable_impls` warning disappeared with the deleted `Default` impl) and `cargo test --no-fail-fast` — 276 passing, up from 274, the two new ones being poison tests. No action needed from Lane A; this row exists so nobody re-fixes it. | closed by Lane B 2026-08-22 — informational |
| H9 | 2026-08-22 | A | B | **Two PowerShell scripts outside `scripts/tests/**` changed in Lane A's F8p, announced here in the spirit of H4.** `scripts/smoke-release.ps1` and `scripts/Invoke-LoomFrameworkArtStoreHookSmoke.ps1` both register cloud API tools whose endpoints are loopback fixtures. F8o made loopback opt-in for cloud Arts (it used to be allowed by default, which let any cloud Art reach the daemon's own HTTP surface, Hook, or a local model server while carrying the Art's credential headers), so those four registrations — `fixture-cloud-text`, `fixture-cloud-art`, `fixture-cloud-multipart-art`, `store-cloud-art` — would have been refused at execution. Each now declares `metadata.permissionPolicy.network.allowLocalhost = $true`. One further one-line change in `smoke-release.ps1`: the multipart evidence field looked for `filename="loom-cloud-input-`, the legacy staged-temp-file name, and now matches the shared `loom-cloud-input` prefix so it also sees the data-URL form `loom-cloud-input.png`. Nothing else in either script was touched, and no Stock Monitor case was involved. If Lane B needs either file, say so here first. | open, informational |
| H10 | 2026-08-22 | B | A | **Lane B claimed the two unowned P2s in `mcp-server-packages/**` (S8a1-1, S8a2b-1), and this row carries the three things Lane A has to act on.** The fixes themselves are in the F12 record below. (1) **One edit landed in a Lane A reserved path and Lane B could not verify it.** `framework-packages/runtime-host/src/mcp.rs` has a test stub that launched the image-search server with `args: vec!["-Endpoint".to_owned(), ...]`. That switch no longer exists, so the stub had to move to `env: BTreeMap::from([("LOOM_IMAGE_SEARCH_ENDPOINT_OVERRIDE", format!("http://{address}/res/v1/images/search"))])`, keeping its existing `credential_env`. It is four lines inside one `#[tokio::test]`, no production code. Lane B does not run `cargo` against the Loom workspace, so **please let it ride along on your next `cargo test -p loom-runtime-host` and report here if it fails.** Type-checking was reasoned about rather than compiled: the literal ends with `..FrameworkMcpServer::default()`, so dropping `args` is legal, `env` is a real `BTreeMap<String, String>` field on that struct (`crates/loom_protocol/src/lib.rs:403`), the literal did not already set it, and `BTreeMap` is already in scope because `credential_env` uses it. Runtime behaviour was checked the same way: `build_environment` (`mcp.rs:377`) merges `FrameworkMcpServer.env` with `credential_env` and `server.env = environment` (`:152`) hands it to the spawned process, and `validate_environment_name` (`:531`) accepts that name. (2) **Please take S8b2-2 into F8's tail.** It is the last P2 in that area that neither lane's row covers, and it is not in `mcp-server-packages/**`, so Lane B has deliberately not touched it. (3) **`scripts/tests/Test-LoomSampleArtInstallExecution.ps1` currently fails on Lane A's uncommitted work, and needs a test seam only Lane A can add.** Five Arts pass, then `custom-image-search` fails with `tool registry error: framework 'mcp' for tool 'custom-image-search' failed [image_search_failed]: MCP image search returned candidates, but none could be downloaded`. The MCP half is fine — candidates *were* returned, which is what proves F12's env-override seam works end to end through a really installed package. What refuses is the new SSRF guard in your reserved `art-packages/samples/image-search/runtime/main.ps1`: `Test-BlockedImageAddress` (`:235`) returns `$true` for `[System.Net.IPAddress]::IsLoopback(...)`, and `Resolve-ImageDownloadTarget` (`:293`) applies it to the URL and every redirect hop. The offline fixture serves its image from `http://127.0.0.1:<port>/fixture.png`, so the block is correct behaviour meeting a test that has nowhere else to serve from. Grepping `art-packages/**` and `scripts/**` for `ALLOW_LOOPBACK`, `AllowLoopback` and `LOOM_IMAGE_.*ALLOW` finds nothing, so no seam exists yet. Lane B has **not** patched around it — the file is yours and the guard is the F8l/F8m work. Suggested shape, since it is the one this lane just used for the same problem: an environment variable a package manifest cannot set (a manifest has no `env` field — see point 1), read once at startup, and honoured only for literal loopback addresses. Lane B will re-run the install-execution test once a seam exists. **Lane A answer to (1), 2026-08-22: the stub passes.** `cargo test --locked --manifest-path .\framework-packages\runtime-host\Cargo.toml` is green — 11 passed, 0 failed, including `mcp::tests::independent_image_search_server_executes_through_mcp_framework`, so the `LOOM_IMAGE_SEARCH_ENDPOINT_OVERRIDE` stub reaches the spawned server exactly as the static reading said it would. Nothing to fix. (2) and (3) are claimed by Lane A and will be answered here as they land; (3) comes next, before S8b2-2. **Lane A answer to (3), 2026-08-22 (F8s): the seam is in, and it is the shape you suggested.** Set `LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES` to `1`, `true`, `yes`, or `on` (case-insensitive; anything else, including unset, leaves the guard closed) in the environment of the process that starts `loom-daemon.exe`. In `scripts/tests/Test-LoomSampleArtInstallExecution.ps1` that means assigning `$env:LOOM_IMAGE_SEARCH_ALLOW_LOOPBACK_IMAGES = "1"` next to the other `$env:LOOM_*` assignments before the `Start-Process -FilePath $daemonPath` call, and adding the name to the `$oldEnvironment` save/restore list at the top so the test leaves the shell as it found it; `Start-Process` inherits the caller's environment, so nothing else is needed. It reaches the Art because the variable is now in the inherited-environment allowlist in `crates/loom_process/src/lib.rs`: both spawn hops (daemon to framework runtime host, runtime host to Art entry) call `env_clear()` and rebuild from that list, so without the allowlist entry an environment variable cannot reach an Art at all — worth knowing for any future seam. It relaxes exactly one rule: a loopback address written literally in an image URL. A hostname that resolves to loopback stays refused, as do private, link-local, unique-local, and IPv4-mapped equivalents, and the check still runs on every redirect hop. Also note F8s repackaged the sample Art store zips, so the installed `custom-image-search` package now contains the seam — if you run the install test against a store you built earlier, repackage first with `scripts/Build-LoomSampleArtPackages.ps1`. **Lane A announcement per H4:** F8s added seam-on coverage to `scripts/tests/Test-LoomSampleArtRuntime.ps1` — a case asserting a literal-loopback candidate downloads with the seam set, next to the existing case asserting it is refused without it — plus a new `scripts/tests/fixtures/LoopbackImageFixture.ps1` and an optional per-case `environment` passthrough in that script's `Invoke-Runtime`. That smoke now runs 12 cases, all passing. Neither file is reserved by either lane and Lane B's F12 work is in `Test-LoomSampleArtInstallExecution.ps1`, which Lane A did not touch; say something here if you would rather own the runtime smoke going forward. **Lane A answer to (2):** S8b2-2 is fixed in F8t. `Blend-Bitmaps` in `art-packages/shared/image-runtime-common.ps1` no longer loops per pixel; it copies the source with `CompositingMode::SourceCopy` and composites the reference over it with `CompositingMode::SourceOver` plus an `ImageAttributes` whose `ColorMatrix.Matrix33` is the mix ratio. Same arithmetic wherever both layers are opaque, and about 50 ms for a 1920x1080 blend instead of millions of GDI+ interop calls. Two things worth knowing if you touch that helper: `InterpolationMode` must stay `NearestNeighbor` so a 1:1 draw is a copy, and the reference draw must set `WrapMode::TileFlipXY`, or GDI+ samples past the source rectangle and leaves the outermost row and column unblended. Transparent regions now composite properly rather than being lerped per channel, which changes output only for images with an alpha channel — the old loop read a transparent pixel's colour as black. The sample Art store zips were repackaged again for this, so the same caveat as F8s applies to `custom-stock-monitor.zip`. | (1) answered and green; (2) answered and shipped in F8t; (3) answered and shipped in F8s |
| H11 | 2026-08-22 | B | A | **Lane B claimed the two findings in `framework-packages/runtime-host/src/mcp.rs` that belonged to no batch — S7c1-1 (P2, declared MCP server version validated then never enforced) and S7c2-1 (P2, Surface argument-binding allowlist bypassed on any call without a `surfaceAction`).** That file is in a Lane A reserved path, so the ownership table was amended first in both copies before any edit: the loan covers **`src/mcp.rs` only**, including its in-file `#[cfg(test)]` module, and ends when F13 is recorded. Four things Lane A needs to know. (1) **`framework-packages/runtime-host/Cargo.toml` and `Cargo.lock` were deliberately not touched, and that constrained the fix.** The lock in the working tree carries your uncommitted 11-line `loom_security` addition while `crates/loom_security/` is still untracked, so committing it would leave a lock that no clean checkout can resolve. The honest fix for S7c1-1 is a real `semver::VersionReq::matches`, which would mean adding `semver` to that package's manifest and regenerating the lock — blocked by the above. What shipped instead is a *sound but deliberately incomplete* local check that never rejects a version the host's real check at `crates/loom_tool_registry/src/framework_process.rs:785-813` would accept; see the F13 record for the exact decision procedure. **If you want the full check, add `semver` to `framework-packages/runtime-host/Cargo.toml` once `loom_security` is committed and replace `resolved_version_violates_requirement` with `VersionReq::parse(...).matches(...)`; the function has a doc comment saying exactly that.** It is recorded as accepted backlog either way. (2) **One invariant your installer already enforces is now re-checked at execution time.** `install.rs:1808-1817` requires `metadata.mcp.{packageId,version}` to have exactly one byte-identical `metadata.dependencies.mcpServers` entry; `load_config` now enforces the same thing, so a manifest edited in place after install fails closed instead of running. This means the runtime host now deserializes `metadata.dependencies.mcpServers`, so **if that field's shape changes in `crates/loom_tool_registry/src/framework.rs`, this file has to change with it** — it keeps its own local mirror of `{id, version}` rather than depending on the crate. (3) **The argument-merge policy changed for Arts that declare `surfaceActions`, which today means `art-packages/samples/stock-monitor` only.** `image-search/manifest.json` declares no `surfaceActions`, so its behaviour is bit-for-bit unchanged, and the existing `arguments_merge_defaults_inputs_and_params` test still asserts the old wholesale merge for that case. For an Art that does declare bindings, `request.inputs` / `request.params` are now filtered through the union of every argument name the manifest declares. Stock Monitor's `interval_seconds` param stops being sent to `get_stock`; its `code` param still is. (4) **Verified with the three `ci.yml:87-94` commands only** — `cargo fmt --manifest-path .\framework-packages\runtime-host\Cargo.toml -- --check`, `cargo check --locked --all-targets --manifest-path ...`, `cargo test --locked --manifest-path ...` — run with no Lane A cargo in flight (H5). All three pass: `fmt --check` clean, `check --locked --all-targets` clean, and `cargo test --locked` reports **21 passed; 0 failed**, up from the 11-test baseline, including `independent_image_search_server_executes_through_mcp_framework`, which drives the real `execute()` against the real image-search manifest and so exercises both new gates end to end. Detail in the F13 record. No Loom release package was built and `r76` was not consumed. | open — informational; please read before next touching that file |
| H12 | 2026-08-22 | A | B | **Lane A is closing the Loom half of H2 in batch F14, and that adds one line to `scripts/build-release.ps1`, announced here under H4.** The new file is `protocol/schemas/surface-stream.v1.schema.json`; the plugin SDK artifact in `build-release.ps1` lists its shipped schemas one `New-SupportSpec` per file, so the new schema needs an entry there or the SDK zip would ship ten of the eleven schemas the CLI can print. That is the only `scripts/**` change in F14. `scripts/tests/Test-ReleaseIntegrityTamper.ps1` also carries a schema list, but it is a synthetic fixture the test fabricates for a fake zip rather than a mirror of the real artifact, so it stays untouched and no parity assertion breaks. Two notes for Lane B. (1) The wire value is unchanged: `"loom.surface-stream.v1"`, now emitted from `loom_protocol::SURFACE_STREAM_PROTOCOL_VERSION` instead of an inline literal, so Hook's own copy of the string still matches and nothing in `Hook/` has to move. (2) Per H2 point (1), `protocolVersion` is declared required in the schema, along with `next`, `reset` and `messages`, and a test in `loom_protocol` fails if any of the four is loosened or if the schema's `const` drifts from the constant. `reset` being required in the schema does not oblige Hook to read it; S1-3 is still open and still not Lane B's. | open, informational — read by Lane B 2026-08-22; no Hook-side change needed, and Lane B has not touched `scripts/**` |
| H13 | 2026-08-22 | A | B | **F9a adds a workspace member, `crates/loom_perf`, and with it the naming convention for every Loom performance budget.** Two things Lane B may care about. (1) The workspace member list in the root `Cargo.toml` and `Cargo.lock` both changed. Lane B does not run Loom `cargo`, so nothing should conflict, but a rebase that drops the member line will break `cargo check --locked --workspace`. (2) A budget is a named integer with an environment override, `LOOM_PERF_MAX_<METRIC>` — for example `LOOM_PERF_MAX_SURFACE_ACTION_RESPONSE_BYTES`. Hook's existing gate uses `HOOK_SHADER_BENCH_MAX_*`, and the two prefixes are intentionally separate: the budgets live in different repositories, run on different schedules, and should not be settable by one variable from the other side. If Hook ever needs to read a Loom budget, say so here rather than sharing a variable name. The first Loom budget is wire bytes for one surface action; peak framework-process memory and end-to-end art wall time follow in F9b and F9c. | open, informational — read by Lane B 2026-08-22; no Hook budget reads a Loom variable, so the two prefixes stay separate |
| H14 | 2026-08-22 | B | A | **Lane B has claimed the three P1s in `apps/daemon/src` that neither lane's row covered — S4a-1, S5a-1 and S5a-2 — and needs one bounded build window to deliver them.** Numbered H14 rather than H13 because H13 was already taken by Lane A's F9a; nothing was overwritten and no handoff is missing. `apps/daemon/**` is reserved by neither lane, so the ownership table needed no change, but F8's todo can strike these three. Five things Lane A needs to know. (1) **The two files carry Lane A's uncommitted work** — `apps/daemon/src/lib.rs` and `apps/daemon/src/surface_resources.rs`. Every edit was made by locating the symbol in the current bytes, not by the review document's baseline line numbers, which are all shifted. Nothing was reformatted, no file was rewritten wholesale, and **no `git checkout` / `git restore` / `git stash` was run against either file**. `cargo fmt` was run in `--check` mode only, never in write mode, so no in-flight Lane A line was reformatted; the new lines were hand-adjusted until the check passed. **Neither file was committed, and this is the one item that needs Lane A to act** — see (7) below. (2) **Build window requested.** Lane B does not run Loom `cargo`, but these three cannot be delivered unverified. The commands are scoped to one package and deliberately exclude `--workspace`: `cargo fmt -p loom-daemon -- --check`, `cargo check -p loom-daemon --locked --all-targets`, and three narrow `cargo test -p loom-daemon --locked <filter>` runs. Please confirm here when no Lane A `cargo` is in flight, per H5. (3) **S5a-2 does not redo S5a-3 or S5a-4.** Lane A's read-buffer and body-size work around `read_http_request` — including `payload_too_large_response` and the route-specific `request_body_size_limit` — is left exactly as found. What Lane B adds is a wall-clock deadline for the whole request read, moving the read off the accept thread, and a write timeout on accepted streams. (4) **S4a-1's garbage collector never reaches into the instance store.** The caller computes the referenced-id set and passes it in, because `delete_surface_instance` establishes the lock order (instance-store lock released *before* the resource-store lock) and inverting it would deadlock. The set deliberately includes references from **persisted** instances, not just live in-memory leases, or a restart would sweep resources that are still in use. (5) **`request_concurrency_class` gained one `Concurrent` arm.** If a later Lane A batch adds routes to that match, note that `GET /v1/surfaces/stream` is now explicitly concurrent and must stay that way — it is a long poll, and classifying it `Serialized` hands one idle client the global route lock for five seconds. (6) **Shutdown behaviour changed slightly, and observably.** A connection already accepted into the listener's backlog when shutdown arrives is now read on the way out instead of being dropped with the listener, and a request whose first bytes had already arrived gets one 500 ms grace window (`SHUTDOWN_READ_GRACE_MILLIS`) rather than being abandoned mid-read. Closing a socket that still holds unread bytes makes Windows answer the peer with an RST, and an RST destroys a response the peer has not read yet — which is what made `daemon_returns_shutting_down_for_request_accepted_before_shutdown` fail with `Os { code: 10054, kind: ConnectionReset }` in roughly two of every three full-suite runs before this batch. Six consecutive runs are clean now. The cost is that shutdown can take up to 500 ms plus one blocking read longer than it used to; a client that has sent nothing is still abandoned immediately, so shutdown never waits on a silent peer. (7) **The two source files are uncommitted, on purpose, and Lane A has to be the one to commit `apps/daemon/src/lib.rs`.** The handoff asked for `git commit -- apps/daemon/src/lib.rs apps/daemon/src/surface_resources.rs`, but `git commit -- <path>` is file-granular, and that file now also holds your F14 line 4630 reading `loom_protocol::SURFACE_STREAM_PROTOCOL_VERSION` — a constant that exists only in the working tree, since `crates/loom_protocol/src/surface.rs` defines it in the tree and not at `HEAD` — plus four `allowLocalhost` fixture hunks from F8o/F8r. `crates/**` and the root `Cargo.toml` / `Cargo.lock` are your reserved paths, so committing `lib.rs` alone would land a daemon referencing a constant absent from the committed tree: a build that fails on a clean checkout, and a bisect trap. Discarding those hunks was not an option either, since `git checkout` / `git restore` / `git stash` on these two files is exactly what this handoff forbids. So both files sit in the working tree, verified at 234 passed / 0 failed, and you can commit `lib.rs` together with `crates/loom_protocol/**` and the lockfile in one coherent commit whenever F14 is ready. `apps/daemon/src/surface_resources.rs` is Lane B's work alone and could stand on its own, but it is held with `lib.rs` deliberately — `lib.rs` is what calls `collect_surface_resource_garbage`, so committing the store without its caller lands dead code and splits one reviewable change in two. Full reasoning is in the F15 record under "Commit scope". **Lane B update, 2026-08-22: the window was taken and every command is green.** No confirmation had arrived, so it was taken the way H11 and F13 took theirs — `cargo` and `rustc` confirmed idle first, per H5, nothing but the scoped commands run, and no PowerShell or `npm` started while they were going. Results are in the F15 record. One thing to report back: for part of that window `cargo test -p loom-daemon` could not build at all, failing in your path with `error[E0425]: cannot find value 'expected' in this scope` at `crates/loom_image_io/src/lib.rs:85` and `:87`. It looked like mid-edit work rather than a regression, Lane B did not touch it, and it resolved on its own — `loom_image_io` compiles in every run recorded in F15. Flagged only in case it is still half-finished in your tree. | closed by Lane B 2026-08-22 — window taken with cargo idle per H5; `fmt --check` and `check --locked --all-targets` clean, six consecutive full-package test runs at 234 passed / 0 failed |
| H15 | 2026-08-22 | B | A | **Lane B has claimed the three persistence-atomicity P2s in `apps/daemon/src/lib.rs` — S5b-2, S5b-3 and S5b-4 — as batch F16.** `apps/daemon/**` is reserved by neither lane, so the ownership table needs no change, but **F8's todo list can strike these three**. Numbered H15 because H14 was taken by Lane B's own F15, and numbered F16 rather than an F11 suffix because Lane A used F11a–F11g. A pre-check ran before any edit: no `### F` record in `phase-78-post-baseline-review.md` covers S5b-2, S5b-3 or S5b-4 — they appear there only as findings (`:902`, `:922`, `:942`) and as cross-references from S5b-5, S5b-6 and S6a — so this is not duplicated work. Four things Lane A needs to know. (1) **One shared helper, four call sites.** Per S5b-4's own fix direction, a new `write_json_atomically` is built on the existing `create_sensitive_temporary` / `replace_sensitive_file` / `restrict_sensitive_path_permissions` / `sync_sensitive_parent` primitives, and `persist_mcp_servers_snapshot`, `LoomSettingsStore::save`, `DeviceRegistryStore::persist` and `write_hook_canvas_root` all route through it. If a later Lane A batch adds a persist site in this file, use that helper rather than `fs::write`. (2) **Two loaders stop silently defaulting.** `DeviceRegistryStore::new` and `LoomSettingsStore::new` both used `unwrap_or_default()`, which turns any parse failure into an empty store; the device registry then immediately re-persisted that emptiness, discarding paired devices and their `session_epoch` revocation counters. Absent-file still means empty, but present-and-unparsable no longer does. This is an observable behaviour change at startup, so it is called out here rather than buried in the record. (3) **`write_hook_canvas_root` gets its atomicity fixed and nothing else.** Whether Loom should be writing Hook's authoritative canvas file at all is a boundary question; Lane B's conclusion is recorded in F16, and any behaviour change on the Hook side is proposed separately, not slipped into this batch. (4) **The same build window and the same commit constraint as H14.** Only the five scoped commands from the handoff are run, never `--workspace`, and `cargo fmt` in `--check` mode only — `lib.rs` still carries roughly +1075 lines of both lanes' uncommitted work, including Lane B's own unpushed F15. The file therefore still cannot be committed by Lane B for the reason in H14(7); F16's record documents whatever commit scope ends up being possible. **Lane B update, 2026-08-22: F16 is recorded done, so F8's todo can now strike S5b-2, S5b-3 and S5b-4 — all three are closed, not merely claimed.** Two co-located P3s went with them and can be struck as well: S5b-6's device-credential half and S5b-5's marker half. The remaining half of each is still open and still Lane A's to schedule if it wants them — S5b-5's abort-on-ACL-failure (a security-posture decision in your startup/bind area) and S5b-6's unauthenticated `/health` disclosure of `pid` and `executable_path` (a public route contract whose cross-project consumers were not enumerable in reasonable time). Reasons for leaving both are in the F16 record. Two further things to report back. **(a) The build window was taken with `cargo` idle per H5, and every command is green** — `fmt -- --check` clean, `check --locked --all-targets` clean with no warnings, the narrow test filters green, and 18 of the last 19 full-package runs at 238 passed / 0 failed. **(b) One pre-existing flake in your area, with a mechanism worth knowing.** Two full-suite runs came back with 38 and 39 simultaneous failures, and every one of them was `ENV_LOCK.lock().expect("env lock")`. Root cause is a single test: `daemon_returns_shutting_down_for_request_accepted_before_shutdown` panics at its `read_to_string(...).expect("read shutdown response")` while holding `ENV_LOCK`, which **poisons the mutex**, and every later env-locked test then fails on the poison rather than on anything of its own. F15 removed the RST half of that flake, but the test still has a bare `thread::sleep(Duration::from_millis(100))` before it signals shutdown, and on a loaded machine that is not enough. Worth two changes on your side: an explicit readiness signal instead of the sleep, and `ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())` so one flake stops cascading into thirty-eight. Lane B touched neither — both are your lines. Separately, one run failed `hook_art_execution_creates_durable_run_evidence`, which passed 6/6 in isolation and did not reproduce in 8 further full runs; recorded as an unrelated flake, not chased. | closed by Lane B 2026-08-22 — see the F16 record |
| H17 | 2026-08-22 | B | A | **Lane B has claimed S5b-1 (P1, the daemon's authentication gateway is skipped by default) as batch F18, and will touch `apps/daemon/src/lib.rs` and `apps/daemon/src/main.rs`.** Neither path is reserved by either lane, so the ownership table needs no change, but **F8's todo can strike S5b-1**. Numbered H17 because H14 and H15 were taken by Lane B's own F15 and F16; H16 was never issued. Pre-check before starting: no `### F` record in `phase-78-post-baseline-review.md` covers S5b-1, and the only other place it is named in this document is `:914`, where it is *excluded* from F13's `remote-surface` staging rather than closed — worth stating because that line reads at a glance like a closure. The finding: every authentication branch in the daemon sits inside `if let Some(token) = auth_token`, and `auth_token` defaults to `None` and is required only for non-loopback binds, so under the default loopback deployment `POST /v1/plugin-credentials/reveal`, `POST /v1/publisher-identity/private-key`, `POST /v1/arts/install`, `POST /v1/frameworks/install`, `POST /v1/mcp/servers/install` and `POST /v1/mcp/call` are reachable with no credential at all; and no route reads `Origin`, `Host` or `Sec-Fetch-*` (grep confirms zero production hits), so a browser page can reach them with a CORS-simple `fetch` and DNS-rebinding defeats the loopback-only assumption. Five things Lane A needs to know. (1) **Shape of the fix.** A token is now always resolved in `LoomDaemon::bind`, between the `repair_legacy_control_plane_permissions` call and the `write_local_capability_manifest` block, in the order `LOOM_DAEMON_TOKEN` env var, then a token file under the already-ACL-restricted control-plane root, then a freshly generated one persisted through F16's `write_bytes_atomically` with `AtomicWritePermissions::Restrict`; if none can be obtained or written, bind **fails closed** rather than falling back to no authentication. The three early gates in `route_request` and the gate in `route` lose their `auth_token.is_some()` precondition. `is_public_device_auth_route` is unchanged: `/health` plus the three device-pairing routes stay public. Added on top: a `Host` check (loopback literal or `localhost` only, port stripped first, since `is_loopback_bind_host` does not strip one), rejection of a present-but-disallowed `Origin`, and a JSON `Content-Type` requirement on state-changing routes so a CORS-simple request cannot get in without a preflight. (2) **The blocker, and the page that breaks is in a Lane A reserved path.** Making the gateway unconditional locks out the daemon's own browser settings UI. `/settings` and `/settings/{app}` render real HTML and are served as `text/html` (the `<!doctype html` sniff at `lib.rs:18218`), so they are reached by browser *navigation*, which cannot attach an `Authorization` header; and the save button's own request — `fetch('/v1/configuration/apps/{app}', {method: 'PUT'})` at `crates/loom_configuration/src/html.rs:93-100` — sends no `Authorization` header either. `device_session_route_allowed` (`lib.rs:3845`) lists only capabilities and surface routes, so a device session cannot substitute. Both work today *only* because `auth_token` defaults to `None`, which is exactly the default this batch removes. Lane B will **not** edit `html.rs`; it is `crates/**` and yours. So F18 has to work against that page unmodified, and the intended shape does: accept a one-shot `?token=` on the `/settings*` navigation, exchange it for an `HttpOnly; SameSite=Strict; Path=/` cookie on the daemon's own origin, and accept that cookie as an administrator credential. The page's existing same-origin `fetch` defaults to `credentials: 'same-origin'` and therefore sends the cookie with no source change, and `SameSite=Strict` together with the new `Origin` check is what stops the cookie from becoming a CSRF hole. Cost: `write_response` (`lib.rs:18216`) grows a header argument. **If Lane A would rather own this as a `loom_configuration` change — rendering the token into the page and having its `fetch` send a bearer — say so here and Lane B will drop the cookie half and gate `/settings*` on the bearer alone.** (3) **The `LOOM_DAEMON_TOKEN` compatibility surface is wider than the finding suggests.** `apps/cli/src/lib.rs:115` reads the variable for the CLI's own calls (help text at `:96`), and four PowerShell smokes touch it — `Invoke-LoomDaemonConcurrencySmoke.ps1`, `Invoke-LoomGatewayBrainPlanSmoke.ps1`, `Invoke-LoomRunPersistenceSmoke.ps1` and `smoke-release.ps1` — of which the middle three deliberately set it to `$null` to obtain a no-authentication daemon. That is no longer obtainable, so they must discover the generated token file instead; `scripts/**` and `apps/cli/**` are both reserved by neither lane and any change there will be announced under H4 before the touch. `crates/loom_process/src/lib.rs:876-896` — yours — asserts the token does *not* leak into spawned children; that assertion stays valid and Lane B will not touch it. Hook needs nothing: `write_local_capability_manifest` already sets `transport.auth` to `bearer` and writes `transport.authToken`, `Neuro/contracts/local-capability/manifest.schema.json:54,71` already require it, and `Hook/src-tauri/tests/loom_connector_contract.rs:69,279` already consume it and already assert an error when it is absent. One caveat that shapes the design: `manifest_dir` defaults to `None`, so the token has to be independently discoverable from the control-plane root and cannot rely on the manifest to propagate it. (4) **Doc debts recorded, not fixed here.** `lib.rs:160`'s help text ("required for non-loopback binds") becomes wrong and is corrected in this batch since the file is already open, but `README.md:163` in the Loom submodule and two super-repo files — `Neuro/README.md:316` and `docs/architecture/neuro-local-app-structure.md:246,:723`, both of which still document the old "loopback needs no token" contract — are outside this batch's scope and are listed as follow-ups in the F18 record. (5) **Same build window and same commit constraint as H14 and H15.** Only `cargo fmt -p loom-daemon -- --check`, `cargo check -p loom-daemon --locked --all-targets` and `cargo test -p loom-daemon --locked`, never `--workspace`, and `fmt` in check mode only — `lib.rs` still carries your F8/F9/F14/F11r work plus Lane B's uncommitted F15 and F16, so the file still cannot be committed by Lane B for the reason in H14(7). Baseline to beat is 238 passed / 0 failed. Also note the test semantics this inverts: roughly 39 `test_daemon_runtime_from_config(&root, DaemonConfig::localhost(0))` sites pass no token, and several of them exist precisely to assert no-token behaviour, so they are being judged one at a time rather than bulk-edited; the four `auth_token: Some(...)` lines at `:22506`, `:26778`, `:26877` and `:26975` are gateway and brain-planner config, not daemon authentication, and are left alone. S5b-5's abort-on-ACL-failure half stays deferred for the reason recorded at `:1305`. | open — please read item (2) before next touching `crates/loom_configuration/src/html.rs` |

## Lane B records


### F4 — done (2026-08-21)

Scope: S9-4. Add `lint`, `typecheck:test` and `test:surface-browser` to
`build-hook-exe.yml` and to the documented local gate `verify:local`, then fix everything
those three scripts report. Files: `Hook/package.json`, `Hook/.github/workflows/**`, plus
whatever the three scripts flag.

What the three scripts reported before the fix: `lint` 19 problems, `typecheck:test` 51
errors, `test:surface-browser` already green. All three are now clean, and the full
`npm test` suite still passes (261 files, 1135 tests).

Gate wiring:

- `Hook/package.json`: `lint` now runs with `--max-warnings 0`, so a new warning fails.
  `verify:local` gained `lint`, `typecheck:test` and `test:surface-browser`.
- `Hook/.github/workflows/build-hook-exe.yml`: `build-windows-exe` gained an `ESLint` step
  and a `TypeScript typecheck (tests)` step before `npm test`. The browser smoke went into
  `parallel-race` instead, with an `npx playwright install --with-deps chromium` step ahead
  of it, so the release-artifact job stays free of the browser download.
- `Hook/CONTRIBUTING.md` section 6 and the `Hook/eslint.config.js` header comment both said
  lint was non-blocking and listed the old gate; corrected.

Production fixes made while clearing the reports (not test-only):

- `src/services/stickerGeometry.ts`: `translateAnnotation`, `cloneStickerAnnotation` and
  `moveLineEndpoint` are now generic over the concrete annotation kind, so callers that
  hold a line or an effect keep that type instead of widening to the whole union.
- `src/components/UnitParamsPanel.tsx` and `src/components/UnitView.tsx`: the inline
  structural `execConfig` prop type is replaced by the real `NodeExecutionConfig`, which is
  what `graphStore.unitExecConfig` and `unit.data.executionConfig` actually carry. The
  inline shape required `__expanded` while the interface makes it optional, so passing a
  real config through the prop did not typecheck.
- `scripts/clean-tauri-dist.d.mts`: new declaration file for the plain-JS build helper the
  Tauri-dist contract test imports (TS7016 without it, and `allowJs` is not on).

Test-side fixes were of four kinds: targeted casts where only the `"2d"` overload of
`getContext`/`createImageData` is stubbed; corrected fixtures where the data was actually
wrong (`file_path` is a delivery type, not a `TransportMode`; missing `serialCounter`,
`direction`, `supported_transports`; duplicate `id` keys shadowed by a later spread);
`as never` for deliberately-invalid normalizer inputs, matching the existing style; and one
array-collected listener capture in `overlaySyntheticEvents` so the `WheelEvent` type
survives the callback boundary.

### F5 — done (2026-08-21)

Scope: the six Stock Monitor Surface findings S8d3-1, S8d3-2, S8d1-1, S8d2-1, S8d2-2 and
S8d3-3. Files: `art-packages/samples/stock-monitor/surface/main.js`,
`art-packages/samples/stock-monitor/runtime/main.ps1`, plus the two test files announced in
H4. The manifest and `art.runtime.json` were read but not modified.

- **S8d3-1 — the two `innerHTML` sinks.** `refs.tip` and `refs.legend` are now built with
  `document.createElement` + `textContent` and installed through `replaceChildren`. The
  tooltip body comes from a new `tipRow` returning an element and a `buildTipContent`
  returning a `DocumentFragment`; the legend comes from a new module-level `legendItem`.
  Provider-supplied text (the point date, the metric values) now goes through `textContent`
  only. The colours are still assigned as `style.color` / `style.background`, which is safe
  because `paletteFor` returns local constants and never provider data.
- **S8d3-2 — pending released by the wrong revision.** `emitAction` now stamps every
  request with a monotonic `requestId` (`"<action>#<n>"`), keeps it in `pendingRequestId` /
  `tickRequestId`, and puts it in the action payload. The runtime echoes it back, and
  `render` only clears a lock when the echoed id matches the id that took the lock. The
  check degrades in three tiers — `lastRequestId`, then `lastActionId`, then
  revision-only — so an older runtime package that echoes neither cannot deadlock `pending`.
- **S8d1-1 — client timeout independent of the host budget.** The runtime publishes its
  effective per-action budgets as `state.actionBudgetsMillis` (new
  `Get-SurfaceActionBudgets`, read from `manifest.json` and clamped by the
  `art.runtime.json` `limits.timeoutMs`). The surface derives its own backstop from that
  with `hostBudgetOf` / `clientDeadlineOf` = budget + `ACTION_DISPATCH_GRACE_MILLIS`,
  clamped to 3 s–180 s. This matters because on a host-side timeout
  `apps/daemon/src/surface_actions.rs` runs `finish_failed` and broadcasts progress but
  never calls `apply_action_response` — no state patch and no revision bump reach the
  surface, so the client timer is the only thing that unlocks the control and it has to fire
  *after* the host gives up. The old constants (`ACTION_TIMEOUT_MILLIS`,
  `PENDING_TIMEOUT_MILLIS`) are kept as the first-frame fallback for the frame before the
  first echo arrives.
- **S8d2-1 — a refused tick left a 1-second full refresh running.** Two changes. The refusal
  no longer latches for the whole lifecycle: it sets `tickDisabledUntil = now +
  TICK_RETRY_COOLDOWN_MILLIS` (5 min), after which the channel is probed again. And while
  the channel is down `refreshPlan` raises the cadence to `FULL_REFRESH_SECONDS`, so the
  fallback is one upstream call per minute instead of four per second.
  **Deviation from the review text:** the review said re-probe "on the next full refresh".
  Clearing the flag after every completed full refresh would make a genuinely unsupported
  tick fire one rejected probe plus an immediate refresh on every cadence tick — that is the
  defect being fixed. A time cooldown gives the same recovery without the retry storm.
- **S8d2-2 — two timers racing.** `fullRefreshTimer` is gone. One interval runs at the tick
  cadence and `refreshPlan` reports `ticksPerFullRefresh`; the interval promotes every
  `ticksPerFullRefresh`-th firing to a full refresh via a `ticksSinceFullRefresh` counter,
  which is reset whenever a full refresh actually settles.
- **S8d3-3 — `resume()` restarted timers without repainting.** Snapshots that arrive while
  suspended do not run `update`, and `suspend` clears the disabled state of the controls, so
  `resume` now calls `render(snapshotValue)` for a whole frame. `setRefreshTimer` is only
  called as a fallback when `activeTimerKey` is still empty, i.e. when `render` returned
  early because there is no snapshot yet — otherwise `render` restarts the timer itself and
  calling both would create two intervals.

Runtime side: a new `Add-ActionEcho` writes `lastActionId`, `lastRequestId` and
`actionBudgetsMillis` into a state patch, and it is wired into all three patch sites
(`Write-SurfaceErrorState`, the interval-commit early return, and the main success path).
All three are required because `statePatch` has merge semantics — a patch that omits the
echo would leave the previous action's `lastRequestId` in place and the client correlation
check would never match again.

Tests (`scripts/tests/**`, unreserved — see H4): `Test-LoomStockMonitorSurface.mjs` covers
the new `refreshPlan` shape and, through two new test hooks
(`disableTickChannel`/`enableTickChannel`), the no-tick cadence and the cooldown re-probe.
Those hooks exist because the VM harness's stub `NeuroSurface.emit()` always returns `true`,
so the refusal path is otherwise unreachable. `Test-LoomStockMonitorArt.ps1` replaces its
`fullRefreshTimer` assertion with source contracts for the single-timer promotion, the
budget mirrors (asserted equal to the real `manifest.json` `timeoutMs` values, so future
drift fails), the correlation echo, and the absence of `refs.tip`/`refs.legend` `innerHTML`;
it also asserts the runtime publishes the budgets and that exactly three patch sites call
`Add-ActionEcho`.

Verification status: `node --check surface/main.js` passes and
`Test-LoomStockMonitorSurface.mjs` passes (`revision-lock=verified
cadence=open/closed/no-tick palette=CN/US`). `Test-LoomStockMonitorArt.ps1` has **not** been
run — `powershell.exe` will not start in this lane's shell, see H5 — so the runtime changes
and the new source contracts are static-reviewed only. What the static review covered: all
three `$statePatch` initializers are `[ordered]@{}`, which satisfies `Add-ActionEcho`'s
`[System.Collections.IDictionary]` parameter; `Get-ActionPayloadValue` tolerates a null
action, so `Get-ActionRequestId` is safe on the no-action paths; the budget lookup is wholly
inside `try`/`catch` and falls back to an empty table, which the surface reads as "no
published budget" and answers with its mirror constants.

### F6 — done (2026-08-21)

Scope: the four Stock Monitor Surface performance findings S8d1-2, S8d2-3, S8d2-6 and
S8d3-4. Files: `art-packages/samples/stock-monitor/surface/main.js` plus the two test files
announced in H4. Nothing in `runtime/` was touched — that is F7.

- **S8d1-2 / S8d2-6 — the derived series was rebuilt from scratch on every redraw.** Two new
  memo layers. `chartSeriesOf(state, revision)` caches `chartRowsOf` under the key
  `code|period|revision|rowCount`, and `chartSampleOf(state, revision, maxPoints, intraday)`
  caches the downsampled points and their moving-average array under that key plus
  `maxPoints` and the intraday flag. Both are single-entry caches: the surface only ever
  draws one series, so a one-slot cache hits on every same-revision repaint and never grows.
  The moving average itself became a rolling accumulator with `MOVING_AVERAGE_WINDOW = 5`,
  replacing the inline `points.slice(index - 4, index + 1).map(...).reduce(...)` that
  allocated three intermediate arrays per point. The intraday branch is a running mean, so
  it needs no window subtraction.
- **S8d2-3 — every frame reallocated the canvas bitmaps and rebuilt roughly 200 DOM nodes.**
  Three separate fixes under one finding.
  1. `canvas.width` / `canvas.height` (and the overlay's mirrors) are now assigned only when
     the computed pixel size differs. Assigning the *same* number still reallocates the
     backing bitmap, and at the `MAX_CANVAS_PIXELS` ceiling the two canvases are about 16 MB
     each. The subsequent `clearRect` + background `fillRect` already clear the whole
     surface, so dropping the reallocation does not leave stale pixels.
  2. `updateMetrics`, `updateHistoryTable`, `updateFavorites`, `renderBookSide` and the tape
     updater no longer start with `replaceChildren()`. They go through a new
     `ensureChildren(host, count, build)` that adds or removes only the difference, and a
     `writeText(node, value)` that skips the assignment when the text is unchanged. Row
     counts are fixed or bounded (8 metrics, 1 head + 8 history rows, `levels.length` book
     rows, 4 tape items), so after the first frame `ensureChildren` does no DOM work at all.
     `appendHistoryRow` is gone. Static labels — the metric captions, the tape captions — are
     written once in the build function and never touched again. `cell.style.color` is still
     assigned unconditionally, with a `|| ""` reset: reading it back yields `rgb(...)` which
     never equals the `#rrggbb` written in, so a comparison would always miss, and the reset
     is required when a data row becomes an `is-empty` row.
  3. Those four updaters, plus the legend, are now behind repaint gates. The updaters read
     only revision-bound state (quote, order book, history, favourites, period), so they run
     only when `revision|activeView|historyLength` changes; the legend depends only on the
     period and the palette, so it has its own key. Everything outside the gates — the
     header, the price block, the session line, the control disabled states — still repaints
     every frame, because those reflect purely local state such as `pending`. **Failure mode
     guarded:** when no usable revision is available (an older host, or a hand-built
     snapshot) the paint key is forced empty, which repaints every frame rather than pinning
     the UI to whatever the first frame showed.
- **S8d3-4 — `ResizeObserver` called `drawChart` once per callback.** Dragging a window edge
  fires the observer every frame, and `drawChart` is a whole-image redraw. A new
  `scheduleChartRedraw` coalesces into one `requestAnimationFrame`, sharing the pattern the
  hover path already used, and `mount` now passes it straight to the observer constructor.
  `cleanup` cancels the pending frame and nulls both caches; `suspend` clears both paint keys
  so the whole-frame repaint that S8d3-3 added to `resume` is not skipped by landing on the
  same revision it suspended at.

Tests (`scripts/tests/**`, unreserved — see H4): `Test-LoomStockMonitorSurface.mjs` gained
behavioural cases for the two changes that can be wrong while still looking plausible. The
rolling MA is compared point-by-point against the naive `slice`/`reduce` definition it
replaced, for both the daily and intraday branches, because an off-by-one in the window
would draw a believable but wrong MA5 line. The sample cache is checked by object identity:
same revision returns the same object, a bumped revision does not, and two different point
budgets get their own entries. The remaining four assertions are source contracts, for the
things a unit test cannot observe without a DOM — the absence of an ungated `canvas.width =`,
the presence of both size comparisons, the observer wired to the coalescer, and the order
book no longer clearing before its loop. `Test-LoomStockMonitorArt.ps1` gained six matching
source contracts.

Verification status: `node --check surface/main.js` passes and
`Test-LoomStockMonitorSurface.mjs` passes (`ma5=rolling sample-cache=verified` added to its
banner). `Test-LoomStockMonitorArt.ps1` has **not** been run — same cause as F5, see H5. In
place of running it, all fourteen of its assertions that touch the F6 surface — the six new
ones plus the eight pre-existing ones that could plausibly break (`MAX_CANVAS_PIXELS` and
`averageValues`, `downsampleRows` and `maxPoints`, the tick promotion counters, the tooltip
and legend contracts, and the two `renderBookSide` call sites) — were re-checked with
equivalent Node regexes against the current file. All fourteen hold.

### F7 — done (2026-08-21)

Scope: the seven Stock Monitor runtime findings S8c1-1, S8c1-2, S8c1-3, S8c2-1, S8c2-2,
S8c2-3 and the runtime half of S8c2-4, plus the Surface half of S8c2-4 and S8d2-9. Files:
`art-packages/samples/stock-monitor/runtime/main.ps1`,
`art-packages/samples/stock-monitor/surface/main.js`, and the two test files announced in
H4. `apps/daemon` and `crates/loom_protocol` were read but deliberately not modified — see
H7.

- **S8c1-1 — the recursive `surfaceAction` search.** `Find-SurfaceAction` walked the whole
  request looking for any object with an `actionId`, which meant an MCP result that happened
  to carry a `surfaceAction` key could fabricate an action invocation. It is replaced by
  `Resolve-SurfaceAction`, which reads three fixed positions only — the request root,
  `params`, and `inputs` — mirroring the host's own `find_surface_action`. Two objects that
  disagree are a hard error (`conflicting surfaceAction invocations were provided`) rather
  than a silent pick.
- **S8c1-2 — a missing observation time was synthesized into a fresh one.** The runtime used
  to fall back to `[DateTimeOffset]::UtcNow` for a provider record that shipped without a
  timestamp, which turned an unknown age into a zero age. The fallback is gone, `ageSeconds`
  stays null, and the staleness verdict fails closed: `($null -eq $ageSeconds) -or
  ($ageSeconds -gt $limit)`. The `-or` is required — in PowerShell `$null -gt 90` is
  `$false`, so the comparison on its own reports an ageless record as fresh.
- **S8c1-3 — offset-less timestamps read as UTC.** Parsing used
  `DateTimeStyles::AssumeUniversal`, so a provider that returns local wall-clock time had its
  age understated by the whole offset. Timestamps must now match
  `(?:[Zz]|[+-]\d{2}:?\d{2})$` to be accepted at all, and parsing uses
  `DateTimeStyles::None`. Combined with S8c1-2 this is fail-closed end to end: an offset-less
  timestamp yields a null age, which is stale, which the cached-record reuse gate refuses,
  so the panel shows no order book rather than a wrong-by-eight-hours one.
- **S8c2-1 — the payload was serialized twice.** The Surface-path formal quote repeated the
  K-line rows, the order book, the tape and the favourites that the same response already
  carried in its state patch. `New-FormalQuote` gained a `-ReferenceState` switch: the
  Surface path emits `history.rowCount`, `history.rowsIn = "authoritativeState.history"`,
  `collectionsIn = "authoritativeState"`, `orderBookLevels`, `liveTapeObservedAt` and
  `favoriteQuoteCount`; the non-Surface path is unchanged, because there is no authoritative
  state for it to point at. The result keeps `statePatch = [ordered]@{}` on purpose — see H7
  for why an omitted field would be destructive.
- **S8c2-2 — a rejected action's text reached stored state and the panel.** An undeclared
  `actionId` was `throw`n with the id interpolated into the message, and the catch wrote that
  message into both the `error` field and the `quote_change` display node. Now
  `Write-SurfaceErrorState -RejectAction` writes a fixed `行情动作未被声明，已拒绝执行`,
  takes the symbol from the authoritative state instead of the rejected payload, and passes
  a null action to `Add-ActionEcho` so the rejected `actionId` / `requestId` never enter the
  stored correlation fields. The client then falls back to its revision-only unlock tier
  (added in F5) instead of waiting for an echo it will never get.
- **S8c2-3 — truncation split surrogate pairs.** A new `Limit-MessageLength` truncates on
  text-element boundaries via `StringInfo.GetTextElementEnumerator`, so an emoji or a
  combining mark is never cut in half. It is applied in both error writers — the
  `Write-RuntimeError` envelope, which previously did not truncate at all and let one long
  upstream message set the response size, and `Write-SurfaceErrorState`.
- **S8c2-4 — `historyError` was computed and dropped.** When the quote succeeds but the
  K-line fetch fails and there is no previous curve to keep, the runtime now publishes
  `historyWarning` in the state patch. The gate stays narrow on purpose: a tick refresh does
  not declare the `history` MCP call at all, so `Try-Get-McpToolContent` returns a
  "missing result" error for it, and widening the gate would emit a spurious warning on every
  tick. Kept-previous-curve and clean-refresh cases both stay silent.
- **S8d2-9 — the staleness verdict was invisible.** The runtime already decided
  `stale` / `ageSeconds` / `maxAgeSeconds`, but the Surface drew only a clock string, so a
  stale record and a current one looked identical. `staleLabel` renders the verdict as text
  (`已陈旧 132/90 秒`, or `已陈旧（观测时间不可用）` when the age is unknown), and
  `is-stale` puts the order-book meta line and the tape strip in the warning colour. The
  footer now renders `historyWarning` through `footerNoticeOf` as
  `K 线数据不可用：<message>` in yellow, so a non-fatal warning is visually distinct from the
  red error it shares a slot with; a real `error` still wins the slot.

One real bug was found by the new tests rather than by review: `staleLabel` first read the
ages through the existing `asNumber`, and `Number(null)` is `0`, which is finite. A record
whose observation time was unknown — precisely the S8c1-2 fail-closed case — rendered as
`已陈旧 0 秒`, i.e. *observed just now*, the exact opposite of the verdict. A local
`asAgeSeconds` now rejects null, undefined and empty string before the numeric conversion.
`asNumber` itself was left alone; changing it would touch every numeric read in the file.

Tests (`scripts/tests/**`, unreserved — see H4): `Test-LoomStockMonitorSurface.mjs` gained
two behavioural groups through two new test hooks, `staleLabel` and `footerNoticeOf` — both
are pure functions of a state record, so they are testable without a DOM, which is what
caught the `asNumber` bug. `deepEqual` on their return values goes through a spread, because
objects built inside the VM realm have that realm's prototype and strict `deepEqual` refuses
them otherwise. `Test-LoomStockMonitorArt.ps1` gained eleven runtime source contracts (the
resolver replacement, the conflict error, `DateTimeStyles::None` with no `AssumeUniversal`,
the explicit-offset pattern, the absence of the synthesized timestamp, both fail-closed
staleness comparisons, `Limit-MessageLength` with the text-element enumerator, the
`-ReferenceState` call site with its pointer string, the empty result patch, and the fixed
rejection message), two Surface source contracts, and four behavioural blocks: the reshaped
formal quote with negative assertions that the collections are *absent*, the two
`result.statePatch` assertions, three `historyWarning` cases (present when the chart is
missing, absent when the previous curve is kept, absent on a clean refresh), and the two
rejection paths.

Verification status: `node --check surface/main.js` passes and
`Test-LoomStockMonitorSurface.mjs` passes (`stale-badge=verified history-warning=verified`
added to its banner). `Test-LoomStockMonitorArt.ps1` has **not** been run in this lane —
same cause as F5 and F6, see H5 — so H6 asks Lane A for one run. In place of running it, the
file was scanned with a Node tokenizer for unterminated quotes, per-line paren imbalance and
accidental `$` interpolation inside double-quoted strings across all 579 lines (the only
imbalances are the eight paired multi-line `param(` / `@(` constructs), and every variable
the new blocks reference was confirmed to be assigned before its first use. The three
contract literals that depend on runtime behaviour rather than text were each checked against
the runtime source: `Write-SurfaceResponse` adds `result` only when it is non-null, so the
"no formal quote" assertion tests an absent key rather than a null one; the rejection path
ends in `exit 0`, so the harness's exit-code assertion still holds; and
`Write-SurfaceErrorState` reads the symbol through `Get-ActionStateValue`, which is why the
rejected-action test expects the authoritative `SZ000034` rather than the payload's
`BJ430047`.

### F11 — S2-1 and S2-2 done (2026-08-21 / 2026-08-22)

Scope: the Hook-side items Lane B took out of Lane A's F8 sweep. All changes are inside
`Hook/`, so nothing in this batch can collide with Lane A. S1-2's Hook half shipped first
and is recorded in H2; this section covers the two Surface-sandbox P2s. S3-3 is still open.

- **S2-1 — a stray `message` event permanently bricked a surface.**
  `public/javascript-surface-bootstrap.js` registered its init listener with
  `{ once: true }` while the handler body was written to *ignore* non-matching messages and
  keep waiting, so the first unrelated `message` event burned the only chance and the
  surface never loaded. The listener is now a named `onHostMessage` that calls
  `removeEventListener` only after an init message is accepted. The defence-in-depth check
  the review kept separate went in at the same time: anything whose `event.source` is not
  `globalThis.parent` is ignored. `event.origin` is deliberately *not* checked — the host
  frame has an opaque origin and must post with `"*"`, so the sender is the only reliable
  axis.
- **S2-2 — the `host-keydown` relay could synthesize arbitrary keystrokes, unthrottled.**
  New policy module `src/services/surfaceHostKeydown.ts`. The relay in
  `src/components/JavaScriptSurface.tsx` now runs shape validation, then
  `isRelayableSurfaceHostKeydown`, then the shared per-second event budget, and only then
  dispatches. Allowlist: plain `Escape`, exactly `Ctrl+E`, and the bare `Control` /
  `Shift` / `Alt` presses `StickerAnnotationLayer`'s modifier tracking needs. Order
  matters — the allowlist runs before the budget so non-relayable spam is dropped without
  spending budget an honest surface may need.

  **Deviation from the review's fix direction, and the one thing worth carrying forward:**
  the review said host handlers should "ignore untrusted events unless they opt in", but
  `isTrusted` is not a usable trust axis in Hook. `src/app.tsx:1250` replays keydowns
  captured by the native overlay keyboard hook (`overlay/global_shortcut`) as untrusted
  `KeyboardEvent`s, so a blanket untrusted-reject would break real global shortcuts. The
  relay instead *tags* what it dispatches (`markSurfaceRelayedKeydown` — a non-writable,
  non-enumerable own property; the sandbox only speaks through the port and cannot reach
  it), and handlers call `acceptsSurfaceRelayedKeydown(event)`, which drops tagged events
  unless the handler passes `{ surfaceRelayed: true }`. Same security property, no
  collateral breakage. If Lane A ever adds a `window` keydown listener in Hook, that call
  is the gate to use.

  Opt-in split across the five existing listeners: `StickerAnnotationLayer` opts in
  (modifier state would go stale mid-drag, because a host drag started inside a surface
  keeps the pointer over the iframe), `StickerContextMenuLayer` and
  `StickerTopStripPropertyBar` opt in (dismissing a menu or dropdown is non-destructive and
  matches user intent). `SurfaceConfirmationDialog` and `AppSettingsDialog` do **not** — a
  surface must not answer its own permission prompt even with the safe answer, nor close
  the user's settings.

Verification. S2-1 is covered by a source contract in
`__tests__/integration/ArtSurfaceInteractionContract.test.ts` (no `once: true`, the removal
exists, and both guards run before it) plus a new real-Chromium scenario `stray-message` in
`scripts/run-javascript-surface-browser-smoke.mjs` that posts a junk message and a port-less
init ahead of the real init and still demands a heartbeat. That scenario was confirmed to
**fail** (`{"kind":"timeout"}`) against the old registration before the fix was restored,
so it is a real regression test and not a tautology. It exists in the browser harness rather
than jsdom because no jsdom harness in this repository evaluates the bootstrap. S2-2 is
covered by `__tests__/unit/surfaceHostKeydown.test.ts` (allowlist accept/reject matrix, tag
detection, opt-in semantics including the untrusted-but-untagged case, and tag
unforgeability), a relay-ordering source contract in
`__tests__/unit/JavaScriptSurface.test.ts`, and a runtime case in
`__tests__/unit/SurfaceConfirmationDialog.test.tsx` proving a relayed `Escape` leaves the
prompt undecided while the next real `Escape` still rejects.

Gate status: `npm test` 262 files / 1142 tests pass, `npm run lint` clean at
`--max-warnings 0`, `npm run typecheck` clean, and `npm run test:surface-browser` reports
`passed: true` across all scenarios including the new one.

### F11 — S3-3 done (2026-08-22)

`Hook/src-tauri` only, so this is invisible to Lane A's build: Hook is its own crate with
its own `target/`, and no cargo command in this batch ran against the Loom workspace.

`network_proxy.rs` gained `shared_client(endpoint, timeout)` and
`shared_client_with(endpoint, timeout, flavor, configure)` over a
`OnceLock<Mutex<ClientCache>>`. Thirteen per-request `Client::builder().build()` sites were
migrated: eight in `loom_hook.rs`, plus `device_session.rs::surface_client`,
`loom_config.rs::read_hook_voice_config` (no timeout), `loom_connector.rs`,
`talk_connector.rs`, and `lib.rs::download_remote_image_bytes_with_reqwest`. Three clients
that were already built once per owning struct — `tea_client.rs`, and both constructors in
`voice/client.rs` — were deliberately left alone.

The one thing worth reading if you ever touch this file: **the cache key is
`(loopback, timeout_millis, flavor)` and deliberately does not include the endpoint**, which
is a deviation from the review's stated `(base_url, proxy generation)` direction. A single
`reqwest::Client` already pools connections per host internally, so keying on the endpoint
would create one pool per host and defeat the point. Proxy generation is an `AtomicU64`
compared against `ClientCache::generation` rather than a key field, so one bump invalidates
everything. `apply_loom_settings` bumps and clears only when the setting actually changed;
`shared_client_with` also clears lazily on mismatch; a client whose build races a change is
returned but not cached; the map is capped at 32 entries because `timeout_millis` comes from
capability manifests.

Lock order, in case Lane A ever adds a caller: the proxy write lock is released *before* the
cache lock is taken in `apply_loom_settings`, because `shared_client_with` takes the cache
lock first and the proxy read lock second (inside `apply_to_url`). Taking them the other way
around in either function deadlocks. `apply_to_url` itself is unchanged and still public.

S3-4 (poisoned proxy `RwLock` silently falling back to `System`) was **not** touched by this
batch — it was a separate P3 finding that nobody had taken. Lane B claimed and fixed it later
the same day; see the F11/S3-4 record below and H8.

Two of Lane A's source-shape contract tests needed a matching update, since they asserted on
text that moved: `tests/loom_connector_contract.rs` and `tests/talk_connector_contract.rs`
now assert `network_proxy::shared_client` at the connector call site *and*
`apply_to_url(Client::builder(), endpoint)` inside the shared path, so the loopback-bypass
guarantee they protect is still asserted end to end rather than weakened.

Gate status: `cargo fmt --check` clean, `cargo clippy --all-targets` introduces no new
warning in any touched file, `cargo test --no-fail-fast` green at 274 tests across all eight
binaries. Three unit tests in `network_proxy.rs` cover reuse, per-timeout/per-flavor
separation, eager invalidation on a real proxy change, no invalidation on a no-op re-apply,
and the cap.

With this, every Hook-side item Lane B took out of F8 (S1-2 Hook half, S2-1, S2-2, S3-3) is
closed.

### F7 — PowerShell side verified in this lane (2026-08-22)

Not a code change. F7 shipped with its only PowerShell test unrun in Lane B, which is why H6
asked Lane A to run it. `powershell.exe` now starts here (see the Lane B follow-up on H5), so
the verification was redone in the lane that wrote the code:

- `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File scripts/tests/Test-LoomStockMonitorArt.ps1`
  — exit 0. `Stock Monitor Surface VM contract passed: revision-lock=verified cadence=open/closed/no-tick palette=CN/US ma5=rolling sample-cache=verified stale-badge=verified history-warning=verified`
  and `Stock Monitor Art contract passed: wrapper=2.9.0 upstream=2.7.3 source=aggregate+pysnowball+xueqiu periods=13 candles=3 tick=1s order-book=2-levels freshness=bounded BJ=verified red-up=CN/HK no-trading=true`.
- `node scripts/tests/Test-LoomStockMonitorSurface.mjs` — exit 0, same Surface VM banner.
- `powershell.exe … -File scripts/tests/Test-LoomSampleArtPackageContract.ps1` — exit 0,
  `Sample Art package contract passed for 7 packages.`, plus the two independent MCP server
  banners. This is the run H3 was reserving a window for; it needs no window, because the
  script calls neither `cargo` nor `node` and so never touches `target/`.

Both banners match Lane A's 2026-08-21 run byte for byte, so nothing was masked by the
earlier gap. F7's status-board row is now plain `done`, and H3 and H6 are closed.

With F7's tail closed, every Lane B batch — F4, F5, F6, F7, F11 — is done and verified. What
remains in this plan is Lane A's (the F8 sweep, F9, and the Loom half of H2) plus the joint
F10, which cannot start until both lanes have committed.

### F11 — S3-4 done (2026-08-22)

Written after the F7 record above, because S3-4 was picked up afterwards. It is the one
finding in the review that carried no owner: listed under S3, not pulled into F8, and not in
either lane's row. Lane B took it because it lives in the same file S3-3 had just reshaped,
`Hook/src-tauri/src/network_proxy.rs`, and nothing else in the repository is involved. No
Loom path was touched and no cargo command ran against the Loom workspace.

The finding: `apply_to_url` read the proxy setting with
`proxy_store().read().map(|proxy| proxy.clone()).unwrap_or_default()`, and `RuntimeProxy`
derived `Default = System`. A poisoned lock therefore returned `System`, so a user who had
switched the proxy off would silently have their traffic put back through it — a privacy
setting failing open because of an unrelated panic somewhere else in the process.

**The fix deliberately departs from the direction the review gave.** The review asked for the
fallback to become `Disabled`. That only moves which user gets hurt: someone on a `Custom`
proxy would lose every outbound call instead. There is no need to guess either way. A lock is
poisoned only because a thread panicked while holding it, which says nothing about the value,
and the value here cannot be half-written — the sole write is one whole-value assignment, so
it is either the old setting or the new one. So `runtime_proxy()` recovers the real setting
through `PoisonError::into_inner()`, clears the poison so one panic does not degrade every
later read, and logs to `stderr` **without the value**, since a custom proxy address can carry
credentials. `unwrap_or_default()` is gone, and so is `impl Default for RuntimeProxy`: the
enum now documents that it has no default on purpose, which forces every future call site to
state what it does when the setting is unreadable. Deleting that impl also removed the
`clippy::derivable_impls` warning the file had been carrying.

Two changes went past the finding's literal scope, both recorded in the review document so
they are not mistaken for drift:

- The write side. `apply_loom_settings` returned `Err("无法锁定 Hook 代理设置")` on a poisoned
  write lock, which means one unrelated panic would have locked the user out of changing their
  own proxy settings for the rest of the process. It now recovers the same way the read side
  does.
- The client-cache mutex from S3-3, which had degraded to building an uncached client on
  poison. That would have switched client reuse off process-wide after any panic and, worse,
  left `apply_loom_settings` unable to drop clients built for the old proxy — so a proxy
  change would have stopped taking effect. `lock_client_cache()` now recovers, but *empties*
  the map: unlike a single enum value, a `HashMap` a panic ran through is not worth trusting,
  and clearing costs one rebuild per key.

Nothing Lane A builds against moved. `apply_to_url` keeps its signature and visibility, and
both source strings the connector contract tests assert — `apply_to_url(Client::builder(), endpoint)`
and the loopback `return Ok(builder.no_proxy())` — are still present verbatim. The S3-3 lock
order is unchanged; the cache-guard scopes in `shared_client_with` are now explicit blocks
with a comment saying why the guard must drop before `apply_to_url` takes the proxy lock.

Worth knowing: `src-tauri/Cargo.toml` sets `panic = "abort"` on the release profile, so no
lock in a shipped `hook.exe` can ever become poisoned. This is defence-in-depth for debug and
test builds and for a future release profile that unwinds — not a live user-facing bug fix.

Gate status: `cargo fmt --check` clean; `cargo clippy --all-targets` clean with **no**
`network_proxy` diagnostic at all, one fewer than before; `cargo test --no-fail-fast` green at
**276** across nine binaries, up from 274. The two new tests poison each lock on purpose from
a spawned thread (the only way a lock becomes poisoned) and assert that a `Disabled` setting
survives, that the poison is cleared, that a later settings change still applies, and that the
cache comes back empty and then keeps caching.

### F12 — S8a1-1 and S8a2b-1 done (2026-08-22)

The two P2 findings the review left inside `mcp-server-packages/**` without an owner: neither
lane's row named them, and F8's sweep stops at Lane A's paths. Both are in a Lane B reserved
path, so claiming them moved no boundary. **No release package was built and Loom `r76` was
not consumed** — these are Loom-side changes that belong in whatever package F10 produces.

#### S8a1-1 — the image-search endpoint was chosen by whoever wrote the manifest

`mcp-server-packages/image-search/runtime/image-search-mcp.ps1` took its Brave endpoint from a
`-Endpoint` switch. `entry.args` in a package manifest is what fills that switch, and every
request the server makes carries the operator's Brave subscription key in
`X-Subscription-Token`. So a manifest could point the server at a host it controlled and be
handed the key on the first search — the credential is not stolen from disk, it is delivered.

**The fix deliberately does not do what the review asked.** The review's direction was to
delete the parameter, or pin the host and force `https`. Deleting it breaks three offline test
stubs that have to reach a local fixture, and pinning the host still leaves the manifest
choosing the path and the scheme. Instead the endpoint became a constant
(`$script:BraveEndpoint`) and the manifest-facing knob was replaced by a seam a manifest cannot
reach:

- `-Endpoint` is still *declared*, but only so that a manifest which still passes it fails
  loudly. `Resolve-SearchEndpoint` throws on any non-empty value and the server exits 1. Silent
  ignoring was the wrong answer here: an old manifest would then keep believing it had chosen
  an endpoint while the server searched somewhere else.
- The offline seam is the environment variable `LOOM_IMAGE_SEARCH_ENDPOINT_OVERRIDE`. A package
  manifest has no way to set it: `McpPackageEntry` (`crates/loom_mcp/src/package.rs:53-62`)
  carries only `command`, `args` and `url`, and the environment the host builds
  (`build_environment`, `framework-packages/runtime-host/src/mcp.rs:377`) comes from
  `FrameworkMcpServer.env`, which is user configuration, plus `credential_env`.
- **The override must resolve to a loopback address, and that requirement is load-bearing
  rather than tidiness.** There is one channel a manifest *does* control that reaches the
  environment: `credentials[].target.name` decides which variable a user-typed credential value
  is injected into. A hostile manifest could therefore name the override variable and get an
  arbitrary string into it. `Test-LoopbackEndpoint` closes that: the value must be `http(s)`,
  carry no `UserInfo` (credentials embedded in the override would be posted to the fixture too),
  and its host must be `localhost` or a literal address for which
  `[System.Net.IPAddress]::IsLoopback` is true. A resolvable name is refused rather than
  resolved — re-resolving here would not be what the HTTP client resolves later anyway. A
  failing check exits 1, because a server that cannot say where the key goes must not send it.

Four call sites passed `-Endpoint` and all four moved to the environment variable. Three are
PowerShell (`scripts/tests/Test-LoomImageSearchMcpServer.ps1`,
`scripts/tests/Test-LoomSampleArtInstallExecution.ps1`) and one is Rust
(`framework-packages/runtime-host/src/mcp.rs`, a `#[tokio::test]` stub — see H10, Lane B cannot
run `cargo` here). The install-execution one needed more than a variable rename: that test
builds a *real* MCP package and installs it, and a manifest cannot set an environment variable,
so `New-ImageSearchFixtureMcpPackage` now writes a wrapper entry script that validates the URL
is loopback, exports the override and then invokes the real server. That is not a new pattern —
`New-StockApiFixtureMcpPackage` in the same file already worked that way.

#### S8a2b-1 — one oversized request desynchronized the framing forever

`mcp-server-packages/stock-api/runtime/stock-api-entry.js` frames JSON-RPC by newline. When the
buffer passed `MAX_REQUEST_BYTES` (1 MiB) *before* a newline arrived, it answered `-32600` and
cleared the buffer — but the rest of that message was still in flight. The next newline
therefore terminated an orphaned tail, which was parsed as a fresh request. From that point on
every response belonged to the wrong request, and nothing recovers on its own: a client that
asked for `tools/list` would get the answer to whatever came before it, forever.

Pure mechanical fix, no judgement call. `runWrapperServer` gained a `discardingUntilNewline`
flag; on a pre-newline overflow the buffer is dropped, the flag is set, and a shared
`rejectOversizedRequest()` emits `{ id: null, error: { code: -32600, ... } }`. While the flag is
set, input is discarded up to and including the next newline and then normal framing resumes.
The overflow of an already-*complete* line needs no discard state, and it now shares the same
`-32600` responder instead of having its own literal. The `-32603` in the parse-failure catch
was left alone deliberately — it is the right code for a genuine internal error, and the two
overflow paths are the ones that had to agree.

#### Tests

`scripts/tests/**` is unreserved; the three files are announced in H4.

- `Test-LoomImageSearchMcpServer.ps1`: the server is launched through the override variable, and
  two fail-closed cases were added — a manifest-supplied `-Endpoint` must exit non-zero with
  `-Endpoint is not accepted`, and a non-loopback override (`https://collector.example/...`,
  i.e. the exfiltration target itself) must exit non-zero mentioning `loopback`. One source
  contract pins the constant. `Start-RedirectedPowerShell` gained a default for `-Arguments`
  so a server can be started with none.
- `Test-LoomStockApiMcpServer.ps1`: after the existing oversized-request `-32600` assertion, two
  legal `tools/list` requests are sent (ids 13 and 14) and the test demands the answer to **14**.
  Asserting only that "a response arrives" would pass against the bug, since the bug produces a
  perfectly well-formed response to the wrong request.
- **The resync test was mutation-probed**, because a framing test that cannot fail is worse than
  none. Setting `discardingUntilNewline = false` and re-running produced exactly the old defect:
  `stock-api JSON-RPC framing did not resynchronize after an oversized request: {"jsonrpc":"2.0","id":13,"result":{"tools"`
  — the orphaned tail answered as id 13. The fix was restored and the probe marker confirmed
  absent before the final run.

#### Verification status

All three focused tests pass, each with `cargo` idle (see H5 for why that matters):

- `Test-LoomImageSearchMcpServer.ps1` — `Independent image-search MCP server contract passed.`
- `Test-LoomStockApiMcpServer.ps1` — `Independent stock-api MCP server contract passed: version=2.9.0 tools=7 quote=24.99 BJ=verified candles=3 bounded-history=verified series=2 five-day=5 retry=verified ttl-cache=verified order-book=2-levels sources=xueqiu+pysnowball+auto`
- `Test-LoomMcpServerPackageContract.ps1` — `Independent MCP server package contract passed: packages=2 stock-api=2.9.0 upstream=2.7.3 pysnowball=0.1.8`, after regenerating the packaged
  artifacts with `scripts/Build-LoomMcpServerPackages.ps1` (`Built 2 independent MCP server packages`). That output directory, `.loom-art-store-data/`, is gitignored (`.gitignore:29`),
  and the script calls neither `cargo` nor `node`.

Two gaps, both in H10 and neither caused by this batch. The Rust stub could not be compiled in
this lane. And `Test-LoomSampleArtInstallExecution.ps1` gets five Arts through and then fails at
`custom-image-search` on Lane A's uncommitted loopback SSRF guard — the MCP search itself
succeeds there, which is the end-to-end evidence that the wrapper-entry seam works inside a
really installed package, but the fixture's image cannot be downloaded from `127.0.0.1` until
Lane A adds a seam of their own.

### F13 — S7c1-1 and S7c2-1 done (2026-08-22)

Claimed: the two unowned P2s in `framework-packages/runtime-host/src/mcp.rs`. That file is a Lane A
reserved path, so the ownership table was amended in **both** copies before any code was touched,
and the loan is written up in H11. Only that one file changed; `framework-packages/runtime-host/Cargo.toml`
and `Cargo.lock` stayed with Lane A, which is the constraint that shaped the S7c1-1 fix.

#### S7c1-1 — the declared MCP server version is now enforced

The trap here is that `metadata.mcp.version` is a semver **requirement** (`^0.1`, `=2.9.0`), not a
concrete version, while `FrameworkMcpServer.version` is concrete (`0.1.0`, `2.9.0`). Comparing the
two strings — the literal reading of "compare the resolved version against the declared one" —
would have rejected every Art in the repository. So the fix does containment, in two places:

- `validate_resolved_server` (extracted out of `execute`, which also makes the id/package/version
  checks unit-testable) rejects a resolved server that reports **no** version at all, and rejects
  one whose version falls outside the range the requirement admits.
- `validate_declared_dependency`, called from `load_config`, re-checks at execution time the tie
  the installer already enforces (`crates/loom_tool_registry/src/install.rs`,
  `validate_mcp_execution_dependency`): exactly one `metadata.dependencies.mcpServers` entry whose
  `id` matches `metadata.mcp.packageId`, carrying the identical version string. The installer only
  sees the package it installs; this sees the manifest that is about to run, so a manifest edited
  in place after installation now fails closed instead of running against a server nobody declared.
  This is deliberately a *re-check*, not a new invariant — recorded here so the next reader does
  not mistake it for one.

Containment is computed by a local `requirement_bounds` + `VersionBounds` pair rather than
`semver::VersionReq`, because **no dependency could be added in this batch**: regenerating
`framework-packages/runtime-host/Cargo.lock` would either sweep in Lane A's uncommitted
`crates/loom_security` (the lock already carries `+11` lines referencing it, and a committed lock
naming an untracked crate breaks a clean checkout) or leave `cargo check --locked` failing. The
local matcher is therefore written to be **sound but incomplete**: it handles the comparator forms
Art manifests actually use (a single `=`, `^`, `~`, or bare requirement over one to three numeric
components) and returns "not checked here" — never "satisfied" — for conjunctions, inequality
comparators, wildcards, and pre-release comparators. A version carrying a pre-release tag is
admitted rather than judged, because pre-release ordering is exactly where a hand-written
comparison would diverge from the real one. The authoritative check remains the host's, with the
real crate, at `crates/loom_tool_registry/src/framework_process.rs:785-813`. The code comment says
to delete both functions in favour of `VersionReq::parse` once `semver` can be added; see H11.

#### S7c2-1 — the argument allowlist now applies on the plain path too

Two options were on the table. **Option A** — apply the Surface bindings as the allowlist on every
path — was rejected: the plain path has no invocation object to bind against, so Stock Monitor's
`code` (which arrives as an Art param there and only as `payload.value` / `authoritativeState.code`
on the Surface path) would have been filtered out and the Art could not run at all outside a
Surface action. **Option B**, implemented: when an Art declares `surfaceActions` it has told the
host which arguments a caller may influence, so `inputs`/`params` are filtered against the union of
every argument name the manifest spells out — `metadata.mcp.arguments` keys, every call's
`arguments` keys, and every Surface binding target. An Art that declares no `surfaceActions` has
expressed no policy and gets `None`, i.e. no filtering, because anything else would break every
existing MCP Art that passes its params straight through.

Blast radius, checked against the two shipped Arts that declare `metadata.mcp`:

- `art-packages/samples/image-search` declares no `surfaceActions`, so nothing about it changes —
  including the existing wholesale-merge expectation in `arguments_merge_defaults_inputs_and_params`.
- `art-packages/samples/stock-monitor` gets the allowlist `{source, period, count, adjust, codes,
  code}`. `code` still reaches the tool call on the plain path; `interval_seconds` — a slider param
  that is Surface state, not a tool argument — stops being forwarded to the MCP server on every
  render.

#### P3s fixed in passing

- **S7c1-2** — a credential name appearing in both the required and the optional map is now
  rejected in `build_environment` *and* `build_headers`, instead of letting the optional mapping
  silently overwrite the required one with a different credential.
- **S7c2-2, security half** — `surfaceAction` is a reserved control key (`SURFACE_ACTION_KEY`) and
  is never merged as a tool argument, so the whole invocation object no longer leaks to the server
  under that name when the Art declares no actions. The other half of the review's suggested fix —
  *rejecting* an invocation the Art cannot handle — is in the backlog below.
- **S7c1-4** — `normalize_config` stores the bytes validation accepted (`server_id`, `package_id`,
  `version`, `toolName`, call ids and tool names, Surface action ids and their selected call ids),
  so a call declared as `" quote"` can no longer pass validation and then fail to match the
  `"quote"` a Surface action selects. Two action ids that collapse to the same key after trimming
  are rejected rather than silently deduplicated.
- **S7c1-5** — `validate_tool_name` is now called from both branches, so the legacy single-call
  `toolName` gets the same 256-byte cap and control-character rejection the multi-call path had.

#### Accepted backlog (not fixed, with reasons)

Fifteen P3s in these two slices stay open. The handoff said nineteen; the review document actually
carries eighteen P3s across S7c1 and S7c2 (S7c1-2…9 and S7c2-2…11), three of which are fixed above.
Every one of the remaining fifteen is listed here, none dropped silently:

- **S7c1-3** (`{artDir}` not expanded in `resolved.command`) — the review names it a prerequisite
  for the package-anchoring fix in **S7b1-1, which is Lane A's**. Expanding the command here first
  would decide, unilaterally, which directory a server binary may resolve inside, before the
  anchoring rules exist. Wrong lane to make that call.
- **S7c1-6** (no denylist for `PATH` / `LD_PRELOAD` / `NODE_OPTIONS` / …) — the review's own fix
  direction is "fix the denylist once, at the `loom_mcp` layer, so both entry points inherit it".
  `crates/**` is outside this loan. Filtering only here would leave S7b1-7 open and give the next
  reader two half-denylists to reconcile.
- **S7c1-7** (header validation duplicated with rules weaker than the transport's) — the fix is to
  expose `loom_mcp`'s managed-header denylist. Same reason: `crates/**`.
- **S7c1-8** (no cap on env/header count or value size) — self-contained, but it is a new rejection
  path with no test that can exercise the failure it prevents (a Windows spawn against a 32 KiB
  environment block) from inside this package. Deferred rather than shipped untested.
- **S7c1-9** (placeholder expansion into the remote URL leaks absolute host paths) — the same one
  line also feeds S7b1's remote-transport work in `crates/**`, and dropping expansion is a
  behaviour removal for any packaged remote server that relies on it. Needs the host-side decision
  first.
- **S7c2-2, remaining half** (reject an invocation the Art cannot handle) — the harm the finding
  describes, Surface state leaving the host as a tool argument, is gone. Turning the ignore into an
  error changes behaviour for any caller that sends invocations to a legacy Art, which is a
  compatibility decision, not a fix.
- **S7c2-3** (`disabled_params` silently deletes bound arguments) — the review asks the two
  contradictory policies to be reconciled into one story. Which story is a product decision about
  what disabling a param means on a Surface, and it is visible in `apps/desktop/**`.
- **S7c2-4** (undeclared arguments dropped silently) — needs a channel for warnings on the
  execution record; `McpExecution` is serialized into the framework protocol response, so adding
  one is a protocol shape change and the handoff said to ask before touching protocol structure.
- **S7c2-5** (normalization is top-level only, `required` never checked) — the review itself calls
  both defensible simplifications. Real schema-directed coercion is a feature, not a fix.
- **S7c2-6** (hardcoded `search_lang` rewrite in the shared bridge) — the fix is an
  argument-alias table in the Art manifest, which means a manifest schema addition plus changes in
  `art-packages/samples/image-search/**`, a path this batch may not touch.
- **S7c2-7** (connect + initialize + `tools/list` per execution) — the review routes it to the S9
  performance queue, and session reuse cannot be built inside a per-execution framework host.
- **S7c2-8** (streamable-HTTP session never terminated) — requires `McpClient::close()` in
  `crates/loom_mcp`. Outside the loan.
- **S7c2-9** (first failure discards succeeded calls; no concurrency) — per-call outcomes are a
  `McpExecution` shape change; same protocol constraint as S7c2-4.
- **S7c2-10** (credentials redacted from errors but not from tool results) — the honest fix is
  redaction over result payloads *including encoded forms*; exact-substring redaction extended to
  results would read as protection while base64 and URL-encoded echoes still pass. Worth doing
  properly, with the redaction set built once, and that belongs with S7a's credential work.
- **S7c2-11** (a legitimately `null` bound value is indistinguishable from a missing one) — needs a
  new field on `McpArgumentBinding` for what the binding accepts, i.e. a manifest schema addition.

#### Verification

All three `ci.yml:87-94` commands, run with `cargo` idle (H5) and no Lane A build in flight:

- `cargo fmt --manifest-path .\framework-packages\runtime-host\Cargo.toml -- --check` — clean.
- `cargo check --locked --all-targets --manifest-path .\framework-packages\runtime-host\Cargo.toml`
  — `Finished dev profile`. The `--locked` run is the evidence that no dependency was added.
- `cargo test --locked --manifest-path .\framework-packages\runtime-host\Cargo.toml` —
  **21 passed; 0 failed**, up from the 11-test baseline. Ten new tests: the version-bound table and
  its undecidable-forms twin, `resolved_version_outside_the_declared_range_is_rejected`,
  `declared_dependency_must_match_the_mcp_package_and_version`,
  `credential_mapping_declared_required_and_optional_is_rejected`,
  `plain_calls_drop_caller_arguments_the_art_never_declared`,
  `surface_invocation_object_is_never_forwarded_as_a_tool_argument`,
  `config_identifiers_are_stored_trimmed_so_selections_match`,
  `surface_action_ids_that_collide_after_trimming_are_rejected`, and
  `legacy_tool_name_is_held_to_the_multi_call_rules`.

`independent_image_search_server_executes_through_mcp_framework` — the one test that drives the
real `execute()` against the real image-search manifest — still passes, which exercises both new
gates end to end (`^0.1` against a resolved `0.1.0`, and the `metadata.dependencies` re-check).
Both shipped Arts and both server packages were audited by hand for the same reason:
image-search declares `^0.1` against a package at `0.1.0`, stock-monitor declares `=2.9.0` against
a package at `2.9.0`, and each Art's `metadata.dependencies.mcpServers` entry is byte-identical to
its `metadata.mcp`. Nothing in the repository is rejected by the new checks.

`scripts/tests/Test-LoomSampleArtInstallExecution.ps1` was **not** run, per the handoff. F10 was
not run. **No Loom release package was built and `r76` was not consumed** — `r76` stays reserved
for F10.

Committed path-scoped (`git commit -- <path>`, never `git add -A`): `framework-packages/runtime-host/src/mcp.rs`
and this file. `framework-packages/runtime-host/Cargo.lock` is modified in the working tree by Lane A
and was left out. Conclusion notes were added beside S7c1-1, S7c1-2, S7c1-4, S7c1-5, S7c2-1 and
S7c2-2 in `docs/progress/phase-78-post-baseline-review.md`, which stays **untracked** in the working
tree — the same choice F12's commit made, since Lane A is still writing that document.

#### S3-1 — the remote Surface half is now explicitly compiled out (Hook)

The same handoff assigned S3-1 to this batch, so it is recorded here rather than as its own number.
Zero boundary change: nothing in Loom was touched, only `Hook/`.

The finding is that the remote / device-session half of Hook's Surface runtime cannot execute in a
shipped binary. `loom_connector::read_default_loom_manifest` validates before returning, and
`validate_loom_manifest` accepts only an origin-only `http` loopback `baseUrl`, so every one of the
eight `authorize_surface_request` call sites receives a loopback manifest,
`loopback_surface_authorization` always answers, and about 500 of `device_session.rs`'s 611 lines are
unreachable. `loom_hook`'s `remote_surface` predicate is permanently false for the same reason.

The owner's 2026-08-21 decision (recorded at `phase-78-post-baseline-review.md:388-390`) is the
second reading: remote is staged for later. So the work was to make "already turned off" **explicit**,
not to turn it on.

What shipped:

- `Hook/src-tauri/Cargo.toml` declares `remote-surface = []`, **off by default**, commented in the
  style of the existing `diag_capture` entry.
- `device_session.rs` gates the remote half per item with `#[cfg(feature = "remote-surface")]`:
  the `Device` credential variant and its `apply` arm, `validate_secure_loom_base_url`, the session
  cache, the whole identity / pairing / session-token chain, the four response structs, the helpers
  (`surface_client`, `decode_signing_key`, `device_session_signature_message`, `random_url_safe`,
  `unix_time_millis`), and the four tests that only mean something with a remote peer. The remote
  body of `authorize_surface_request` moved into a gated `remote_surface_authorization`; with the flag
  off the function refuses a non-loopback endpoint through a new `staged_remote_surface_error`, whose
  message names both the feature and the document. A module doc comment at the top of the file states
  why the code is there and why it does not run.
- `loom_hook.rs` gates the `remote_surface` predicate (it is a `let … = false;` with the flag off),
  `loom_base_url_is_loopback`, `start_remote_surface_poll_listener` and `poll_remote_surface_once`.
- `Hook/docs/REMOTE_SURFACE_STAGED.md` is new: the staged status, why the path is unreachable, the
  exact list of what the flag covers, the intended feature (Loom pushes *rendered* frames to paired
  displays; the loopback snapshot/push pipeline is unaffected and stays in scope), a
  flip-it checklist, and the verification recipe for both combinations.

Design choices worth recording:

- **Per-item `cfg` rather than a nested module.** Moving ~350 lines into `mod remote { … }` would
  re-indent the entire half and make the diff unreadable for a change that alters no logic. A grep
  first confirmed no Loom or Hook test asserts on `device_session` source text, so the nested-module
  option was available — it was rejected on review cost, not on risk.
- **`cfg` rather than `#[allow(dead_code)]`.** An `allow` hides rot; a real `cfg` makes the flag
  load-bearing, which is why both feature combinations are now part of the verification recipe.
- **Two items stay compiled in both combinations**, because live loopback code depends on them:
  `invalidate_surface_sessions` (called from a loopback path in `loom_hook.rs`; with the flag off its
  body is `cfg`'d away and it is a documented no-op) and `DeviceSessionAuthorization::device_id` (read
  on live request paths). `surface_stream_envelope` / `describe_stream_protocol_version` likewise stay
  compiled so their five protocol-version tests keep running by default; the former carries
  `#[cfg_attr(not(feature = "remote-surface"), allow(dead_code))]`.

Scope discipline, per the handoff:

- **The manifest validator was not relaxed.** `validate_loom_manifest`'s strictness is the only gate
  and it is untouched. The document says in as many words that widening it is the same act as
  flipping the flag.
- **S1-1, S1-3 and S3-2 were not fixed** — they are gate conditions. `REMOTE_SURFACE_STAGED.md`
  carries them in a table with the dependency stated the right way round: they must land *before* the
  flag is flipped, because each is latent only while this path is unreachable. S3-2's two prefix
  matchers were annotated in place (`validate_secure_loom_base_url`, `loom_base_url_is_loopback`) with
  a note that they are gated, not fixed, and a pointer at the document. They stay open in the review
  document.
- **S5b-1 is not in this flag's scope** and the document says so explicitly.

Verification — both combinations, as required:

| gate | default (flag off) | `--features remote-surface` |
| --- | --- | --- |
| `cargo fmt --check` | clean | clean |
| `cargo clippy --all-targets` | no warning in the edited ranges | no warning in the edited ranges |
| `cargo test --no-fail-fast` | **273 passed; 0 failed; 1 ignored** | **276 passed; 0 failed; 1 ignored** |

The 276 with the flag on matches the pre-existing baseline exactly. The 273 is arithmetic, not loss:
four remote-only tests compile out and one new flag-off test
(`staged_remote_surface_error_names_the_feature_and_the_document`) compiles in. Clippy's warning set
is identical in both runs and every entry is pre-existing; nothing lands in the ranges this batch
edited. `cargo`/`rustc` were confirmed idle before starting (H5).

F4's three CI gates also pass: `npm run lint` (`eslint src --max-warnings 0`), `npm run
typecheck:test`, and `npm run test:surface-browser` (`"passed": true`).

**No Hook release package was built, so `r92` was not consumed** and stays free for the next batch.
The reason is the handoff's own rule: no reachable behaviour changed. Every path a shipped binary can
take is byte-for-byte the same decision it made before — the endpoint the new refusal fires on is one
`validate_loom_manifest` already rejects upstream. What changed is that ~500 lines of unreachable
code are no longer in the default build and are no longer readable as a live security control.

**No Loom release package was built and `r76` was not consumed** by this half either; F10 was not run.

Commit scope — **the Hook changes were deliberately left uncommitted in the working tree.** Hook's
tree already carries a large complete-but-uncommitted batch that predates this work, and
`src-tauri/src/loom_hook.rs` is one of the files it touches: a `git diff` of that file shows foreign
hunks this batch did not write (the surface-stream protocol-version constant near `:267`, the
`loom_hook_listener_subscription_tests` module near `:1474`, and the rewritten
`authorize_surface_request` call sites at `:2365`, `:2608`, `:2704`, `:2765`, `:2797`, `:2827`,
`:3007`). `git commit -- <path>` is file-granular, so committing that path would silently publish
another batch's work, and `git add -A` is forbidden. Committing only the two files that *are* entirely
this batch's (`src-tauri/Cargo.toml`, `src-tauri/src/device_session.rs`) plus the new document would be
worse: it would land a `[features]` declaration and half the gating without `loom_hook.rs`'s half,
which is a build that does not compile with the flag on. So nothing was committed in the Hook
repository; whoever commits that in-flight batch should pick these four paths up with it —
`src-tauri/Cargo.toml`, `src-tauri/src/device_session.rs`, `src-tauri/src/loom_hook.rs`,
`docs/REMOTE_SURFACE_STAGED.md`. Both feature combinations were verified against the working tree as
it stands, so the four paths are consistent as a set.

In the Loom repository only this file was committed, path-scoped. The S3-1 conclusion note went into
`docs/progress/phase-78-post-baseline-review.md`, which stays **untracked** — same choice as F13's
first half and F12 before it.

### F15 — done (2026-08-22)

Scope: the three P1s in `apps/daemon/src` that neither lane's row covered — **S4a-1**, **S5a-1**,
**S5a-2**. Files: `apps/daemon/src/surface_resources.rs` and `apps/daemon/src/lib.rs`, nothing else.
`apps/daemon/**` is reserved by neither lane, so no ownership change was needed; the claim is in
**H14**, and that row explains why it is not H13.

Both files carry Lane A's uncommitted work. Every edit located its target by symbol name in the
current bytes, never by the review document's baseline line numbers, which are all shifted. No file
was reformatted or rewritten, `cargo fmt` was run in `--check` mode only, and **no `git checkout`,
`git restore` or `git stash` was run against either file.**

#### S4a-1 — the Surface resource store never deleted, and one unreadable object refused startup

Two failures in one store. `SurfaceResourceStore::new` walked the metadata records at load and
returned `SurfaceResourceStoreError::Invalid` when a record's payload was missing or its length did
not match; `LoomDaemon::bind` propagates that with `?`, so a single truncated `.bin` file — a crash
during a write is enough — meant the daemon would not start again. And nothing ever deleted: leases
expired, but the objects they had covered stayed on disk forever.

- **Load is now tolerant.** The per-record work moved into a free function,
  `load_stored_resource(root, path) -> Result<StoredSurfaceResource, String>`, and the loop in `new`
  discards a record it cannot read with a warning naming the file and the reason, instead of failing
  the store. A discarded record's files are left for the orphan sweep rather than deleted inline, so
  a transient read error cannot destroy a recoverable object.
- **`collect_garbage(&mut self, referenced_resource_ids: &BTreeSet<String>)`** deletes an object that
  has neither a live lease nor a reference in the set it is given, and returns a
  `SurfaceResourceGcOutcome { removed_objects, removed_bytes, removed_orphan_files, retained_objects,
  failures }` for the log. Objects younger than `RESOURCE_GC_MIN_AGE_MILLIS` (10 minutes) are kept
  regardless: a resource is uploaded before the instance that references it is written, and that gap
  must not be collectable.
- **The reference set comes from the caller, deliberately.** `collect_surface_resource_garbage`
  (`lib.rs`) reads `SurfaceInstanceStore::list`, releases the instance-store lock, and only then takes
  the resource-store lock — the order `delete_surface_instance` already established. Letting the
  resource store look up its own references would invert those two locks and deadlock. The doc comment
  on that function says so, so the next reader does not "simplify" it.
- **The set counts persisted instances, not just in-memory leases.** This is the part the handoff
  singled out and it is load-bearing: leases do not survive a restart, so a GC pass that trusted only
  the lease table would delete, on the first pass after boot, exactly the resources the persisted
  instances are still displaying. `list` returns temporary instances alongside persisted ones, which
  makes the set a superset — a resource held only by a temporary instance is still in use right now.
- Two triggers: once at startup, and once after `delete_surface_instance` has released that instance's
  leases, which is the moment its resources become collectable. The pass re-reads the instance store,
  so a resource the deleted instance shared with a surviving one is still protected. A failed pass is
  logged and never fatal — the objects stay on disk and the next pass tries again.
- **Delete order is `.json` before `.bin`.** A crash between the two leaves a payload with no
  metadata, which the orphan sweep reclaims; the other order would leave metadata pointing at nothing,
  which is the state that used to refuse startup. `NotFound` counts as deleted, so a half-deleted pair
  converges instead of failing every pass. `sweep_orphan_files` removes a `.json`/`.bin` half whose
  digest no live object names, and skips the lease table.

Four new tests: `a_damaged_object_is_discarded_at_load_instead_of_failing_the_store`,
`gc_keeps_an_object_a_reference_still_names_after_its_lease_is_gone`,
`gc_leaves_a_young_unreferenced_object_alone`,
`gc_sweeps_orphan_halves_but_not_the_lease_table_or_a_temporary`. The age window is reachable in a
test through a `#[cfg(test)]` setter, `set_gc_min_age_ms`, rather than by sleeping.

#### S5a-1 — `GET /v1/surfaces/stream` was classified `Serialized`

The long-poll route fell through `request_concurrency_class` to the `Serialized` default, so one idle
client holding a poll open owned the global `serialized_route_lock` for its full 5-second wait and
every other serialized route — including ordinary Surface writes — queued behind a client doing
nothing. It now has an explicit `Concurrent` arm, next to the dedicated `surface_stream_executor`
that was already carrying these requests.

**A correction to the handoff on this point:** it says the existing test
`request_concurrency_classification_is_conservative` asserted the route was serialized. It did not —
the route appeared in neither of that test's two arrays. The change is an addition, not an inversion,
and the test now names the route in the concurrent list. Anyone auditing this against the handoff text
should read the test, not the summary.

New test: `a_serialized_route_completes_while_a_surface_stream_long_poll_is_parked` — a long poll is
parked against a daemon, a `POST` is issued while it is still parked, and the assertion is that the
POST answers before the poll does. It takes about 3 seconds by design; that is the poll's wait, not a
sleep.

#### S5a-2 — a per-read timeout with no per-request deadline, on the accept thread, with no write timeout

Three defects with one shape: a client that never finishes could hold the daemon. `serve_until` called
`read_connection` inline on the accept thread, and the only bound was a 2-second timeout on each
individual `read`, which a client sending one byte every second resets forever — one such client stalls
every other connection, `/health` included, for as long as it likes. Nothing called
`set_write_timeout` at all, so a peer that stopped reading its response parked the writing worker
permanently.

- **A wall-clock deadline for the whole request read.** `read_http_request_until(stream, deadline,
  abort)` checks the deadline before every read and answers 408 (`request_timeout`) when it passes;
  `read_connection` sets it to `MAX_REQUEST_READ_MILLIS` (30 s) from the moment the socket arrives.
  `read_http_request` survives as a `#[cfg(test)]` wrapper so the three existing test callers did not
  have to move.
- **The read moved off the accept thread** into a small `BoundedRequestExecutor` read stage
  (`CONNECTION_READ_WORKERS = 4`, `CONNECTION_READ_QUEUE_CAPACITY = 64`). The accept thread now only
  applies socket options and hands the socket over; read connections come back through a channel and
  are dispatched by the same loop. An idle pass waits `ACCEPT_IDLE_WAIT_MILLIS` on that channel, which
  is what keeps the loop from spinning now that `accept` is non-blocking in both directions.
- **A write timeout on every accepted stream.** `prepare_connection` sets it
  (`RESPONSE_WRITE_TIMEOUT_MILLIS = 30 s`) on the accept thread rather than in the read stage, because
  the accept thread itself writes some responses — a queue-full 503 goes out on a socket no worker ever
  touches.

**S5a-3 and S5a-4 were not redone.** Lane A's buffer and body-size work around `read_http_request` is
left exactly as found, including `payload_too_large_response` and the moved
`request_body_size_limit`; the 413 logic and `HTTP_READ_CHUNK_BYTES` are untouched.

Two new tests: `a_trickling_request_does_not_block_the_accept_loop` (a byte-at-a-time client is in
flight while a second client's `/health` completes) and
`a_trickling_request_is_rejected_when_it_outlives_the_read_deadline`.

#### Fixed in passing

Neither is a numbered review entry; both are in the code the three items above touch, and both were
found while testing them.

1. **A shutdown signal could be swallowed by the Surface-stream dispatch arm.** That arm ended in
   `continue`, which jumped past the trailing `if shutdown_after_read { break }` — and the signal had
   already been consumed by `try_recv`, so it was gone for good and the daemon kept serving. Dispatch
   is now a function returning `DispatchOutcome::{Continue, Stop}`, so every arm reports the same way
   and no arm can skip the check.
2. **A connection in the listener's backlog was reset at shutdown instead of being answered.** This
   one has a receipt: before the fix, the pre-existing test
   `daemon_returns_shutting_down_for_request_accepted_before_shutdown` failed in roughly two of every
   three full-suite runs with
   `read shutdown response: Os { code: 10054, kind: ConnectionReset }`. Under load `serve_until`
   observes shutdown before it reaches its first `accept()`; dropping the listener then resets every
   connection still queued in it, and a reset destroys a response the peer has not read yet — so a
   client whose request arrived just before shutdown saw a dead socket instead of its 503. The same
   hazard applies one level in, to a request whose bytes had started arriving when the readers began
   draining. Both now get a bounded grace: `drain_accept_backlog` reads the backlog inline on the way
   out, under one shared `SHUTDOWN_READ_GRACE_MILLIS` (500 ms) budget for the whole drain, and
   `read_http_request_until` extends a request that has already read bytes by the same 500 ms instead
   of dropping it — a socket that has sent *nothing* is still abandoned immediately, so shutdown never
   waits on a client that may never speak. Called from both loop exits, not just the top one.

#### Accepted backlog

Not fixed, with the reason each:

- **The read stage can saturate.** A flood of trickling connections fills the 64-slot queue, after
  which new connections — `/health` included — get a bounded 503 rather than service. That is a real
  improvement on the old behaviour (unbounded stalling) but it is not fairness. A proper fix is
  per-peer connection limits, which is a policy decision and a larger change than a P1 fix should
  carry.
- **A pre-read 503 can still reach a sending client as a reset.** The queue-full and drain-at-entry
  paths write 503 without reading, and closing a socket with unread bytes in its receive buffer is
  what produces the reset described above. It is bounded to those two paths and both mean the daemon
  is already refusing work; removing it needs a lingering half-close, which is out of scope here.
- **`RESOURCE_GC_MIN_AGE_MILLIS` is not configurable.** Ten minutes is a guess that has to cover the
  gap between an upload and the instance write that references it. It should probably be a setting, but
  adding one touches the config registry, which is not this batch.
- **The startup GC pass runs against whatever `LOOM_CONTROL_PLANE_ROOT` points at.** For a test that
  does not override it, that is the developer's real control-plane root. Nothing unreferenced and
  younger than 10 minutes can be touched, and the reference scan covers the persisted instance store,
  so this is safe — but it is worth knowing before anyone shortens the age window for convenience.
- **`settings_apply_mcp_limits_and_global_art_update_policy` asserts process-global state.** It read
  `loom_mcp::runtime_limits()`, which is per-process, so it is interference-prone under parallel test
  execution by construction. It failed once during this batch's investigation, before the shutdown
  hardening, and has not failed in the six parallel runs since — but the shape of the test is the
  latent problem and fixing it means giving those limits a per-test scope. Not this batch's file.

#### Verification

Scoped to one package and deliberately without `--workspace --all-targets`, per H14. `cargo`/`rustc`
were confirmed idle before starting, per H5, and no PowerShell or `npm` was run while they were going.

| command | result |
| --- | --- |
| `cargo fmt -p loom-daemon -- --check` | clean, zero diff — so no in-flight Lane A line was reformatted, and the new lines were hand-adjusted until they passed |
| `cargo check -p loom-daemon --locked --all-targets` | clean, no warnings, 11.30s |
| `cargo test -p loom-daemon --locked surface_resource` | **10 passed; 0 failed** — includes all four new S4a-1 tests |
| `cargo test -p loom-daemon --locked request_concurrency` | **1 passed** |
| `cargo test -p loom-daemon --locked trickling` | **2 passed** |
| `cargo test -p loom-daemon --locked draining` | **1 passed** |
| `cargo test -p loom-daemon --locked parked` | **1 passed** (3.07s — the long poll's own wait) |
| `cargo test -p loom-daemon --locked`, six consecutive runs | **234 passed; 0 failed** every time, plus the 8 `daemon_cli_contract` cases |

Those six runs are the evidence for the backlog fix, not decoration: the same command failed two out
of three times before it. An earlier single-threaded run (`--test-threads=1`) was also 234/0, which is
how the failure was identified as a race rather than a defect in the new code.

One environment note: for part of this batch `cargo test -p loom-daemon` could not build at all,
failing in a Lane A reserved path with `error[E0425]: cannot find value 'expected' in this scope` at
`crates/loom_image_io/src/lib.rs:85`. It was mid-edit work, not a regression from this batch, and it
resolved on its own; `loom_image_io` compiles in every run recorded above. Tests run during that
window used the already-built `loom_daemon` test binary.

**No Loom release package was built and `r76` was not consumed.** F10 was not run.

#### Historical commit scope (superseded by `41123d2`)

Only this document was committed: `git commit -- docs/progress/phase-78-lane-sync.md`. No `git add -A`
was run, and nothing was appended to `docs/progress/phase-78-post-baseline-review.md`, which stays
untracked and merges in F10.

The handoff asked for `git commit -- apps/daemon/src/lib.rs apps/daemon/src/surface_resources.rs`.
That command would have produced a commit that does not compile from a clean checkout, so it was not
run. `git commit -- <path>` is file-granular, not hunk-granular, and by the time this batch finished,
`apps/daemon/src/lib.rs` in the working tree contained Lane A's in-flight work as well as this batch's:

- Line 4630, from Lane A's F14, reads `loom_protocol::SURFACE_STREAM_PROTOCOL_VERSION`. That constant
  exists **only in the working tree** — `crates/loom_protocol/src/surface.rs` defines it in the tree
  and not at `HEAD`. `crates/**` and the root `Cargo.toml` / `Cargo.lock` are Lane A reserved paths, so
  the defining crate cannot be committed alongside it. Committing `lib.rs` by itself would therefore
  land a daemon that references a constant absent from the committed tree: a broken build for everyone
  who checks out `main`, and a bisect trap.
- Four `allowLocalhost` permission-policy fixture hunks from Lane A's F8o/F8r are also in that file and
  absent at `HEAD`. Committing them would publish another lane's unfinished work under this batch's
  message.

Discarding those hunks to get a clean commit was not an option either — the handoff forbids
`git checkout`, `git restore` and `git stash` on both files precisely because that destroys Lane A's
uncommitted work, and that prohibition is the whole reason this constraint exists rather than a
workaround for it.

So both files stay in the working tree exactly as verified, at 234 passed / 0 failed. This mirrors the
precedent already recorded in F13's Hook half, where a shared file made a path-scoped commit publish a
neighbouring batch's work and the code was likewise left in the tree with the reason written down.
Lane A already holds every path involved, so it can commit `apps/daemon/src/lib.rs` together with
`crates/loom_protocol/**` and the lockfile in one coherent commit; nothing further is required from
Lane B. `apps/daemon/src/surface_resources.rs` is this batch's work alone and could be committed on its
own, but it is held with `lib.rs` on purpose: `lib.rs` is what calls
`collect_surface_resource_garbage`, so committing the store without its caller would land dead code and
split one reviewable change across two commits.

### F16 — done (2026-08-22)

Scope: the three persistence-atomicity P2s in `apps/daemon/src/lib.rs` that neither lane's row
covers — **S5b-2**, **S5b-3**, **S5b-4** — claimed in **H15**. Every edit was located by symbol
name in the current bytes, never by the review document's baseline line numbers, which are all
shifted by both lanes' in-flight work.

#### S5b-4 first: one shared helper

The fix direction for S5b-4 asked for a helper that does not exist yet, and the other two findings
are call sites of it, so it was built first. Two functions were added next to the existing
`sync_sensitive_parent`:

- `write_json_atomically(path, value)` — serializes pretty JSON with a trailing newline and hands
  the bytes to the byte-level half below.
- `write_bytes_atomically(path, bytes, permissions)` — `create_dir_all(parent)` →
  `create_sensitive_temporary` → `write_all` → `sync_all` → drop the handle → restrict permissions
  → atomic replace → restrict again → `sync_sensitive_parent`. Any error removes the temporary
  before returning, so a failed write leaves neither a partial destination nor a stray file.

That is the same sequence `write_local_capability_manifest` and `persist_mcp_registry_cache`
already used by hand; the point of the helper is that "which persistence site is crash-safe" stops
being per-site knowledge.

`AtomicWritePermissions` has two variants on purpose. `Restrict` is for the files the daemon owns.
`Preserve` exists for exactly one caller — Hook's canvas — because the temporary is created 0o600
on unix, and an atomicity fix must not silently narrow the ACL of a file this process does not own.
The helper also deliberately does **not** re-ACL the parent directory on every write; the parent is
restricted once at startup, and doing it per write would be churn, not safety.

Four call sites now route through it: `persist_mcp_servers_snapshot`, `LoomSettingsStore::save`,
`DeviceRegistryStore::persist` and `write_hook_canvas_root` (the byte-level variant, since it
returns the exact compact bytes it wrote to its caller).

**One discovery that changed the helper.** On Windows a rename-with-replace fails with
`ERROR_ACCESS_DENIED` (`os error 5`) while any other handle holds the destination open — including a
reader that is merely reading it, an antivirus scanner, or the search indexer. The old
delete-then-rename shape was accidentally tolerant of this, so a naive atomic replacement would
have been *less* robust in production than what it replaced. `replace_sensitive_file_with_retry`
was added for this: up to 20 attempts, 5 ms apart. Each attempt is still one atomic replacement —
the retry adds patience, not a second window.

#### S5b-2 — the device registry fails closed

`DeviceRegistryStore::persist` used a bare `fs::write`, and `::new` loaded with
`fs::read().ok().and_then(...).unwrap_or_default()`. A torn write therefore produced an empty
registry at the next start, and the store then re-persisted that emptiness — so every paired device
disappeared *and* every device's `session_epoch` reset. `session_epoch` is the revocation counter:
losing it means a device that was explicitly revoked can pair again and come back. That is the
sharpest edge in this batch, and it is why this one loader is not allowed to degrade.

`DeviceRegistryStore::new` is now fallible (`-> Result<Self>`) and distinguishes three cases:

| on disk | behaviour |
| --- | --- |
| file absent (`ErrorKind::NotFound`) | legitimate empty registry — bootstrap one local host and continue |
| file present, unparsable | **refuse to start**, naming the path and saying to move the file aside |
| file present, unreadable for any other reason | **refuse to start** — an ACL or lock problem must not read as "no devices" |

Failing closed rather than quarantining is the deliberate choice here: quarantining would let the
daemon come up with revocation state that was silently thrown away, which is the exact outcome the
finding is about. The corrupt bytes are left untouched on disk so an operator can inspect them.
Five call sites were updated for the fallible constructor — one production path with
`.context("open device registry")?` and four test paths.

`LoomSettingsStore::new` and `load_persisted_mcp_servers` are different: nothing security-relevant
is lost by starting from defaults, and refusing to boot over a corrupt preference file would be a
worse failure than continuing. Both now **quarantine and degrade** — the unparsable file is renamed
to `<name>.corrupt-<unix_millis>` by a new `quarantine_unreadable_file`, a `[WARN]` line records it,
and startup proceeds with defaults. What neither does any more is discard the file silently.

#### S5b-3 — the publisher identity is never absent

`save_publisher_identity` wrote a fixed-name temporary, then `fs::remove_file(&path)`, then
`fs::rename`. Between the remove and the rename there was no identity file on disk at all, and an
interruption in that window loses the publisher identity permanently. The whole body is now two
lines through `write_json_atomically`. The `remove_file` was not replaced by anything: on Windows
`replace_sensitive_file` uses `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`, and on unix
`fs::rename` replaces an existing destination, so no platform needed it. The fixed temporary name
is gone too — `create_sensitive_temporary` mints `.{file_name}.tmp-{pid}-{nonce}-{attempt}`, so two
concurrent callers can no longer scribble over each other's partial bytes.

#### `write_hook_canvas_root` — the boundary conclusion H15(3) promised

Atomicity is fixed; **no Hook behaviour was changed in this batch**, and no Hook file was touched.
The conclusion, for the record:

**The write path should not be removed, but it should not stay as it is either.** Loom is the
viewer and Hook is the editor that holds the raw project data, so Loom truncating and rewriting
Hook's authoritative canvas file is an inversion of that boundary. But the path is not gratuitous —
it is how a canvas edit made in Loom reaches Hook at all, and deleting it would remove a working
feature rather than fix a defect. What is actually missing is coordination: Loom and Hook share no
lock, no lease and no generation number on that file, so a Hook editor session with the canvas open
can overwrite Loom's write, or be overwritten by it, with neither side detecting the loss. Atomicity
alone narrows the window to zero-length but does not resolve who wins.

The proposal — **separate, not in this batch** — is a generation number in the canvas document plus
compare-and-swap on write: Loom sends the generation it read, Hook (or the write path) refuses the
write if the file has moved on, and Loom surfaces a conflict instead of silently winning. That is a
protocol change across two repositories and needs Hook's editor to participate, so it belongs in its
own change with its own review, not slipped into an atomicity fix. A comment at the call site records
this and points at F16.

#### P3s fixed in passing

Both are local and cheap and sit inside files this batch already had open, per the grading rule.

- **S5b-6, the device-credential half.** `route_with_runtime` evaluated the device credential first
  and returned its error before the administrator bearer was ever considered. Since
  `ParsedHttpRequest::has_bearer` and `authorization_credential` both scan *all* headers — verified
  before changing anything — one request can legitimately carry both `Authorization: Bearer …` and a
  stale `Authorization: Device …`, which is exactly what a just-re-paired desktop client sends. The
  bearer is now decided first, and `Err(_) if admin_authenticated => None` ignores a stale device
  credential when the caller is already entitled to the request. With no admin bearer the device
  error is still reported in full.
- **S5b-5, the marker half.** The Windows ACL-repair marker recorded only a count, so a skipped
  entry could never be found again. It now lists each skipped path (`skipped-path=…`) and is written
  through `write_bytes_atomically` instead of `fs::write`.

#### Accepted as backlog, with reasons — nothing dropped silently

- **S5b-5, the abort-on-failure half.** A single un-repairable entry still aborts startup via `?`,
  and skipped entries are never retried. Not changed here: turning an abort into a warn-and-continue
  is a security-posture decision, not a crash-safety fix — the restriction on the control-plane root
  is what actually protects the tree — and the code sits in Lane A's startup/bind area. It deserves
  its own change with its own review. The marker now at least names what was skipped, which is what
  a retry pass would need.
- **S5b-6, the `/health` half.** The unauthenticated `/health` response still carries `pid` and
  `executable_path`. Not changed here: `/health` is a public route contract and the consumers are
  cross-project (Hook, the desktop app, packaging and CI probes). A repo-wide search for consumers
  did not finish in reasonable time and was stopped rather than left running, so the blast radius of
  removing those two fields is genuinely unverified. Redacting them for unauthenticated callers is
  the right change; it needs the consumer list first.

#### Tests — four added, all in the existing `#[cfg(test)]` module

The three the handoff required, plus one for the S5b-6 fix. Each one was checked against the *old*
code to prove it discriminates, by temporarily restoring the old sequence, running the test, and
restoring the fix.

1. `device_registry_refuses_to_start_when_the_stored_file_is_unparsable` — writes truncated JSON,
   asserts `DeviceRegistryStore::new` errors with a message naming both "device registry" and the
   path, asserts the corrupt bytes are still byte-identical afterwards, then removes the file and
   shows the same call bootstraps one local host. It matches on the error instead of using
   `expect_err` because `expect_err` needs `T: Debug` and `DeviceRegistryStore` deliberately has
   none — it holds live session material. Deriving `Debug` to satisfy a test was rejected.
2. `publisher_identity_replacement_always_leaves_a_readable_file` — a reader thread loops
   `load_publisher_identity` while the main thread performs 12 replacements. The reader fails the
   test on `Ok(None)` ("the identity file was missing during a replacement") and on a parse error
   (half-written bytes). Against the old delete-then-rename body it fails on `Ok(None)` every time
   it was tried — 8 runs, 8 failures, all with that exact message.
3. `atomic_json_writes_keep_the_previous_file_and_leave_no_temporary` — a successful write leaves no
   temporary; a value that cannot serialize (`BTreeMap<(u8,u8),u8>`) fails before the destination is
   touched; a directory standing where the destination should be makes the replace fail, and
   afterwards the directory survives, no temporary remains, and the earlier file's bytes are
   unchanged.
4. `stale_device_credential_does_not_mask_a_valid_administrator_bearer` — a request carrying both a
   valid admin bearer and a stale `Device` credential succeeds; the same stale credential alone is
   still rejected. Against the pre-fix ordering the first assertion fails with
   `401 device_session_invalid`, which is the bug S5b-6 describes.

**What it took to make test 2 stable, since the honest version matters.** Three separate flakes,
all in the test rather than the product:

- Windows refusing the replace while the reader held the file (`os error 5`). Fixed in the product
  by the retry helper above, and in the test by tolerating a refusal — a refused replace leaves the
  previous identity whole, which is the property under test.
- The reader counting a transient *open* failure as a violation. A failed open says nothing about
  the file's contents; only a parse failure means torn bytes. The reader now fails on `Ok(None)` and
  on `用户签名身份无效` (parse) and ignores `无法读取用户签名身份` (open).
- `reads > 0` failing with `contended replacements: 12`, i.e. the reader thread was first scheduled
  *after* all 12 writes had finished, so it never looked at the file — a vacuous pass that the
  assertion correctly caught. The reader now reads once before consulting the stop flag and
  publishes a counter, and the writer waits for the first read before starting, so overlap is
  guaranteed rather than hoped for. The final state assertion is anchored by one write taken after
  the reader has stopped, with a bounded retry.

#### Verification — the five scoped commands, actual results

The window was taken with `cargo` and `rustc` confirmed idle first, per H5, and nothing but scoped
`-p loom-daemon` commands was run — no `--workspace`, no full `--all-targets` sweep of the tree, no
`cargo fmt` in write mode, and no PowerShell or `npm` started while they were in flight.

| command | result |
| --- | --- |
| `cargo fmt -p loom-daemon -- --check` | exit 0, no diff. One of this batch's own new lines needed hand-wrapping to get there; no other lane's line was reformatted. |
| `cargo check -p loom-daemon --locked --all-targets` | `Finished`, no warnings. Two `unused_assignments` warnings appeared mid-batch on this batch's own test code and were fixed rather than allowed. |
| `cargo test -p loom-daemon --locked device_registry` | `ok. 2 passed; 0 failed` |
| `cargo test -p loom-daemon --locked publisher_identity` | `ok. 2 passed; 0 failed` |
| `cargo test -p loom-daemon --locked atomic_json_writes_keep_the_previous_file_and_leave_no_temporary` | `ok. 1 passed; 0 failed` |

Also run, beyond the required five: `cargo test -p loom-daemon --locked stale_device_credential`
(`ok. 1 passed`) for the S5b-6 test, and the full package suite. After the final test hardening,
**18 of the last 19 full `cargo test -p loom-daemon --locked` runs were clean at 238 passed; 0
failed**. The single failure was `hook_art_execution_creates_durable_run_evidence`, which touches
none of the four persistence sites, passed 6/6 in isolation, and did not reproduce in 8 further full
runs.

**One finding for Lane A, free of charge.** During this batch's stress runs two full-suite runs came
back with 38 and 39 simultaneous failures. Every one of them was
`ENV_LOCK.lock().expect("env lock")`: `daemon_returns_shutting_down_for_request_accepted_before_shutdown`
panics at its `read_to_string(...).expect("read shutdown response")` while holding `ENV_LOCK`, which
**poisons the mutex**, and every later env-locked test then fails on the poison rather than on
anything of its own. So a single flake in that test masquerades as a catastrophic suite failure. The
test still has a bare `thread::sleep(Duration::from_millis(100))` before it signals shutdown, and on
a loaded machine (CPU was sitting near 60% from unrelated processes throughout) that sleep is not
enough. Two things worth doing on Lane A's side: replace the sleep with an explicit readiness
signal, and consider `ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())` so one
flake stops cascading. Lane B did not touch either — both are Lane A's lines.

#### Commit scope

**Only this document was committed** (`git commit -- docs/progress/phase-78-lane-sync.md`), path-scoped;
`git add -A` was never run, and nothing was appended to `phase-78-post-baseline-review.md`.
`apps/daemon/src/lib.rs` is intentionally left in the working tree for the same reason as F15, and
that reason is unchanged and re-verified today: the file still carries Lane A's F14 reference to
`loom_protocol::SURFACE_STREAM_PROTOCOL_VERSION`, and `git show HEAD:crates/loom_protocol/src/surface.rs`
still does not contain that constant. A file-granular commit of `lib.rs` alone would therefore land a
daemon that does not build on a clean checkout. `crates/**` is Lane A's reserved path, and
`git checkout` / `git restore` / `git stash` on this file is what H15 forbids, so the code waits in
the tree — verified at 238 passed / 0 failed — for Lane A to commit `lib.rs` together with
`crates/loom_protocol/**` and the lockfile, exactly as H14(7) already asked.

**No Loom release package was built and `r76` was not consumed.**

## Lane A records

Lane A keeps its `### F<n> — done` records in `phase-78-post-baseline-review.md`. Nothing
is needed here beyond the status board and handoff acknowledgements.

## 2026-08-23 independent closeout audit

This section supersedes status and commit-scope claims above where they conflict. The audit read
the repository and release artifacts rather than trusting this board.

### Corrections to stale or false records

- F8's P2 sweep is done. The explicitly accepted P3 list remains backlog; it is not unfinished F8
  implementation. The old `in progress` state was stale.
- F9a/F9b/F9c and F14 are complete and present in commit `41123d2`; their old status rows were
  stale.
- F15 and F16 are also present in `41123d2`. The statements at the end of both historical records
  that `apps/daemon/src/lib.rs` and `surface_resources.rs` were still intentionally uncommitted were
  true only before that commit and are false for the audited repository state.
- F18 was genuinely unfinished when the board was written. It is now implemented: every production
  daemon resolves an administrator token (explicit config, environment, persisted restricted file,
  or newly generated restricted file) and fails closed if it cannot; public routes are limited to
  health and device bootstrap; protected routes require bearer/cookie authentication; loopback
  `Host`, `Origin`, `Sec-Fetch-Site`, and JSON mutation constraints are enforced; the settings-page
  query token is exchanged once for an HttpOnly, SameSite=Strict cookie and redirected to a tokenless
  URL. CLI and desktop native clients discover and send the persisted token.
- F10 was not performed in its originally stated clean/committed shape. The audit produced and tested
  two dirty-worktree candidates because existing work from multiple agents could not safely be
  discarded or silently committed. Therefore the row is `partial`, not `done`.

### Additional unfinished work found and closed

- The Art installer validated MCP immutable directories with a hard-coded 12-character digest while
  the MCP package installer creates 32-character directories. The validator now consumes
  `loom_mcp::package::PACKAGE_DIRECTORY_DIGEST_CHARS`, with a regression test that resolves an MCP
  dependency from the real installer directory shape.
- Windows framework staging renames could fail under short antivirus/indexer locks. The framework
  directory move now retries only Windows `PermissionDenied` failures within a bounded window.
- Checked-in framework/sample package artifacts were stale, so the real image-search loopback seam
  was absent even though F8s said it shipped. All four framework packages and seven sample Art
  packages were rebuilt.
- `Test-LoomSampleArtInstallExecution.ps1` was not actually complete: it lacked daemon bearer auth,
  a loopback permission declaration and a real image fixture, and asserted the old duplicate Stock
  Monitor response shape. Those contracts now match the authenticated daemon, actual HTTP image GET,
  and authoritative Surface state.
- Hook's full gate exposed one stale contract after the shared-client optimization:
  `HookGeneralSettingsContract.test.ts` still required `network_proxy::apply_to_url` at call sites
  that correctly use `shared_client`/`shared_client_with`. The contract now verifies the shared
  clients and also verifies that their central builder still applies the proxy policy.

### Fresh verification and artifacts

- Loom: `cargo fmt --all -- --check` and `cargo check --locked --workspace --all-targets` passed;
  `cargo test --locked --workspace` passed **677 tests in 59 suites**. A separate daemon rerun passed
  **241 tests**.
- Loom release: `scripts/build-release.ps1` built
  `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260822-phase78-closeout-r76`
  plus its main portable ZIP under `packages/`. `verify-release.ps1 -RunSmoke` checked 57 files and passed the standalone,
  Hook canvas, Hook error preview, framework Art store, plugin boundary, Surface prototype, and
  authored-Art smokes.
- Hook: lint, application/test typechecks, full Vitest suite, real browser Surface smoke, production
  web build, Rust fmt/check, and Rust tests passed. Rust reported **273 passed** across the executed
  suites, with the one real-Tea-daemon smoke still ignored by its explicit environment contract.
- Hook release: the actual build script is `scripts/build-local-hook-exe.ps1`; the previously
  documented `scripts/package-hook-release.ps1` does not exist. It produced
  `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Hook\20260823-phase78-closeout-r91\hook.exe`.
  `scripts/package-release-zip.ps1` produced the six-entry portable ZIP in the same directory.
  The release EXE reported self-check `status=ok`, self-check version `0.1.7`, and
  `hook 0.1.7` for `--version`.

Both manifests/worktrees are dirty-source evidence, not a substitute for the still-open strict
clean-source provenance pass. Existing unrelated modifications, `.memsearch`, `.playwright-cli`, and
crash-dump/scratch files were not deleted to manufacture a clean tree.

