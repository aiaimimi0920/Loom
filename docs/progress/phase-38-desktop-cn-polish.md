# Phase 38: Desktop Chinese Copy Polish

## Goal

Continue the Phase 35 Chinese UI pass by removing remaining user-visible English
or internal runtime jargon from the Loom desktop surface.

The Phase 37 release proved Hook live workflow persistence, but a follow-up
desktop audit still found UI-adjacent strings that could confuse users:

- desktop API and Tauri errors still referred to the internal `daemon`;
- MCP Marketplace cards still used English status tags such as
  `Manual config`, `Key missing`, `Test failed`, and `Tools N`;
- MCP Marketplace categories were rendered directly from English enum values
  such as `Search`, `Web`, `Local`, and `Developer`;
- Workflow Studio warnings and default names still used English text such as
  `Untitled workflow`, `Input image`, and `Workflow YAML has no nodes`;
- direct HTTP errors used `returned HTTP`.

## Changes

- Localized desktop local-service error copy.
  - `Unable to reach the local Loom daemon` became `无法连接 Loom 本地服务`.
  - Tauri bridge errors now say `Loom 本地服务`, `Loom 本地服务地址`, and
    `Loom 本地服务 API 路径` instead of exposing `daemon URL/path`.
  - Workflow load errors now say `Loom 本地服务没有返回工作流 ...`.
- Localized direct HTTP errors:
  - `returned HTTP` became `Loom 本地服务请求 ... 返回 HTTP ...`.
- Localized MCP Marketplace text.
  - Built-in server descriptions and source labels are now Chinese.
  - Health tags now use `需要配置`, `密钥已填`, `缺少密钥`, `已发现工具 N`,
    and `测试失败`.
  - Registry notes now use `注册表更新时间 ...` and
    `启用前需要手动补充包参数。`.
  - Added `MCP_MARKET_CATEGORY_LABELS` and `mcpMarketCategoryLabel()` so the UI
    renders category labels as `搜索`, `网页`, `本地`, `记忆`, `浏览器`,
    `开发`, `工具`, `推理`, and `兼容`.
- Localized Workflow Studio fallback copy.
  - Default name is now `未命名工作流`.
  - Image input label is now `输入图像`.
  - Interface inference warnings are now Chinese.
- Extended `test-loom-artloom-parity-contract.ps1` with negative assertions so
  these user-visible English strings and daemon jargon do not regress.

## Validation

RED checks observed:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
```

Failed first on the existing desktop API copy:

```text
Loom desktop user-facing API errors must say Loom 本地服务, not daemon jargon. Forbidden=[local Loom daemon]
```

After the first fix, the Tauri backend test also correctly failed because the
expected error string was still the old English daemon wording:

```text
tests::rejects_non_loopback_daemon_url ... FAILED
left: Some("Loom 桌面端只连接回环地址上的本地服务。")
right: Some("Loom desktop only connects to loopback daemon URLs")
```

GREEN/regression validation:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\tests\test-loom-artloom-parity-contract.ps1
npm run typecheck --prefix Loom\apps\desktop
npm run build --prefix Loom\apps\desktop
cargo test --manifest-path Loom\apps\desktop\src-tauri\Cargo.toml
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-release-exes.ps1 -Apps Loom -VersionId loom-desktop-cn-polish-phase38 -Force
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\verify-release.ps1 -VersionId loom-desktop-cn-polish-phase38 -Apps Loom
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke-release-local-apps.ps1 -VersionId loom-desktop-cn-polish-phase38 -Apps Loom
```

Results:

- Parity contract passed.
- Desktop TypeScript typecheck passed.
- Desktop Rsbuild build passed.
- Tauri backend validation passed: 5 tests passed.
- Formal release verification passed:
  - `gitHead = bc569bd70be0e6ce097040319e4aea8a7a3cd736`
  - `gitDirty = false`
  - `checksumEntries = 31`
- Full packaged Loom release smoke passed, including the Phase 37 Hook live
  workflow evidence:
  - `hookLiveWorkflow.workflowId = "hook-live"`
  - `hookLiveWorkflow.listName = "Hook 实时工作流"`
  - `hookLiveWorkflow.nodePersisted = true`
  - `hookLiveWorkflow.targetNodePersisted = true`
  - `hookLiveWorkflow.edgePersisted = true`

Smoke summary:

```text
output\smoke\runs\20260617-034816-Loom-25704-5feed27a4b7e4e8eb74eaefdf06edbff\release-local-apps-loom-desktop-cn-polish-phase38-Loom-summary.json
output\smoke\latest\release-local-apps-loom-desktop-cn-polish-phase38-Loom-summary.json
```

## Release

Generated release:

```text
release\Loom\loom-desktop-cn-polish-phase38
```

Executables:

```text
loom.exe
loom-daemon.exe
loom-desktop.exe
```

Package:

```text
release\Loom\loom-desktop-cn-polish-phase38\packages\Loom-loom-desktop-cn-polish-phase38-windows-x64.zip
```

Package sha256:

```text
b6c5d77189ef9d6e7932ab4546502f8ee499282ef5aa5fc6d029e436eb73daec
```

## User-facing impact

- Users should see `Loom 本地服务` instead of needing to understand the internal
  `daemon` process name.
- MCP Marketplace category filters and cards are understandable in Chinese.
- Workflow Studio default names and interface inference warnings are localized.
- `loom-desktop.exe` remains the normal startup path.
