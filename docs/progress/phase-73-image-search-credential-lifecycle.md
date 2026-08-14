# Phase 73: 图片搜索 credential 生命周期与 Hook canonical 输入修复

日期：2026-08-14

## 状态

实现、回归测试、完整源码门禁、Hook R18、Loom R28、Loom release verifier，以及
R28 隔离真实 Brave provider 执行均已完成。R18/R28 的 600 秒原生双端验收仍因用户
正在运行旧 Hook 实例而安全阻塞；没有终止或替换用户进程。

## 1. 目的

Phase 72 已把图片搜索 MCP server 收入 Art ZIP，但现场执行仍会出现：

```text
MCP Art requires credential `brave_api_key`
```

本阶段的目标不是增加另一个 fallback、旧 MCP server alias 或 Hook 专用执行分支，
而是修复 canonical Framework / Art 调用链中的四个真实边界：

1. Art settings 与 ToolDefinition 之间的 credential binding 生命周期；
2. Hook 对 capability 明确空输入 `inputs: []` 的端口权威语义；
3. canonical `type: "secret"` 在 Hook UI、session、sync 和执行 payload 中的安全语义；
4. Brave raw image result 的来源页面字段映射。

## 2. 根因

### 2.1 Loom registry 丢失持久化 credential binding 投影

用户设置保存在：

```text
<control-plane>/art-user-settings.json
```

受保护 credential 保存在 `CredentialStore`。Windows 持久化值由当前用户 DPAPI
保护，管理 API 只返回摘要和 alias，不返回明文。

Phase 72 的安装、升级或重启路径会重新生成 ToolDefinition，但 Framework 执行前的
credential grant 只读取：

```text
metadata.artUserSettings.credentialBindings
```

因此，settings 文件和 registry projection 发生漂移后，credential 本身仍存在，
Art management 也能看到 binding，但执行 ToolDefinition 已没有 binding。真正故障
位于 Loom credential-binding lifecycle，不是 Art ZIP 缺失、MCP server 缺失、
Brave endpoint 不可用或 key 失效。

### 2.2 Hook 把旧 `input_image` 连线发给 generator Art

`custom-image-search` 的 canonical manifest 明确声明：

```json
"inputs": []
```

旧 session 仍可能保留指向 `input_image` 的 link。Hook 原先在 capability 的 image
inputs 为空时回退到旧 `unit.inputs`，使 stale link 进入
`loom.hook.art.execute.inputs`。package-local MCP 随后会正确拒绝未知参数。

### 2.3 Hook 未完整执行 secret 语义

manifest 使用：

```json
{ "id": "brave_api_key", "type": "secret", "required": true }
```

Hook 原 normalization 主要读取布尔 `secret`，导致 canonical secret 可能显示成普通
参数并写入节点/session。进一步审查还发现：即使执行参数已过滤 secret，工作流同步
快照和本地 session save 仍会原样使用旧 `unit.params`。

### 2.4 Brave source page 映射错误

Brave raw image schema 中 `result.url` 是图片 URL，来源页面是 `result.source`。Phase 72
把 `result.url` 同时当作 `sourcePageUrl`，可能给后续图片下载提供错误 Referer。

## 3. 实现

### 3.1 `art-user-settings.json` 是唯一持久化权威

`ArtSettingsStore::get_optional` 区分“没有该 Art 设置”和“存在默认值为空的有效设置”。
canonical `<control-plane>/tools` registry 在以下时机重新投影 settings：

- `ToolRegistry::save_tool_inner`；
- `ToolRegistry::read_tools`，包括 daemon restart 后首次读取；
- Art install、upgrade、rollback 返回 active ToolDefinition 时。

投影前会先移除 ToolDefinition 中已有的 `artUserSettings`。如果 settings store 没有
对应 qualified Art entry，陈旧 binding 不会作为第二真相源继续存活；如果 entry
存在，则只从该 entry 重新生成：

```text
autoUpdate
defaults
valueBindings
credentialBindings
```

这不是 registry 旧数据 fallback，而是删除 stale projection 后从当前持久化权威
重建运行时视图。读取失败返回结构化 `art_settings_error`，不会静默使用旧 binding。

### 3.2 Hook 以 capability 端口为权威

对于已解析的 Art capability：

- `inputs` 字段存在时，只有 capability 声明的端口有效；
- `inputs: []` 表示该 Art 没有输入端口；
- 只有 capability 没有提供 `inputs` 字段时，才使用当前 unit port fallback；
- 没有增加 `custom-image-search` Art-ID 特判或 `input_image` alias。

因此旧 session link 可以继续存在于历史图数据中，但不会进入本次执行 inputs，也不会
被误报成 generator Art 的缺失必需输入。

### 3.3 Secret 不进入 Hook 普通状态或 payload

Hook 现在执行以下规则：

- `type: "secret"` 或 `data_type: "secret"` 必然归一化为 `secret: true`；
- 冲突的 `secret: false` 不能降级 canonical secret 类型；
- secret 的 manifest/defaults 值被丢弃；
- 新 Art node 不把 secret 写入 `unit.params`；
- 普通参数面板不渲染 secret；
- Art execute 参数过滤 capability-declared secret；
- session restore 会删除旧 session 中已知的 secret 参数；
- workflow sync snapshot 和本地 session save 使用剥离 secret 后的参数映射。

credential 明文只在 Loom 受保护 credential 边界内解析，并由 Framework execution
context 映射为 package-local 进程环境变量 `BRAVE_API_KEY`。

### 3.4 图片搜索 Art 0.3.1

`neuro.official/custom-image-search` 版本提升到 `0.3.1`。package-local MCP 保持 exact
tool `brave_image_search`，并把：

```text
result.source -> sourcePageUrl
```

Art 仍然只依赖 `neuro.official/mcp` Framework，不依赖用户级历史 MCP server 配置，
也没有 npm/npx fallback。

## 4. 默认 key 的安全定义

当前“默认可用 key”不是仓库中的 hard-coded key。它由以下两个用户控制面对象组成：

1. `art-user-settings.json` 中
   `credentialBindings.brave_api_key -> <credential alias>`；
2. `CredentialStore` 中 Art-scoped、DPAPI-protected credential value。

本轮没有把 key 写入 Git、manifest、普通 defaults、Hook session、workflow payload、
日志、文档、hash、长度、前缀或片段。把真实 key 编译进 release 会破坏 publisher
隔离、credential rotation 和 secret redaction，因此明确不实现。

当前用户控制面中已有的 protected binding 已通过正式 settings API 重新保存，并由
实时 R27 成功执行真实 Brave 请求。R28 又在隔离控制面中完成更强验证：测试先从复制
的 `tools.json` 删除 `artUserSettings`，再启动 R28，由 R28 从 settings store 重建
binding，安装 release 内的 `custom-image-search@0.3.1`，并完成真实 provider 搜索和
图片下载。验证过程没有读取或输出明文；临时 daemon、临时 credential 副本和脚本已
全部清理。

## 5. 回归测试与源码门禁

2026-08-14 fresh 结果：

| Gate | 结果 |
| --- | --- |
| Hook targeted image-search/security | 6 files / 56 tests passed |
| Hook frontend full suite | 253 files / 1065 tests passed |
| Hook TypeScript / production build | passed |
| Hook Rust formatting | passed |
| Hook Rust library | 228 passed |
| Loom Rust formatting | passed |
| `loom_tool_registry --lib` | 122 passed |
| `loom-daemon --lib --test-threads=1` | 205 passed |
| Loom desktop | 137 passed；typecheck/build passed |
| Framework package contract | 4 Frameworks passed |
| Sample Art package contract | 6 Arts passed |
| Package-local MCP direct contract | passed |
| Installed Framework + Art execution | 6 formal executions passed |

关键新增回归覆盖：

- restart 后从独立 settings store 重建 credential binding；
- settings entry 不存在时删除 registry stale binding；
- upgrade 和 rollback 保留 settings 与返回 ToolDefinition 的 binding；
- Framework process request 获得 Art-scoped credential grant；
- explicit `inputs: []` 拒绝 stale image link；
- canonical secret 不能被冲突布尔字段降级；
- secret 不进入 node defaults、UI、execute params、session restore 或 sync/save 参数；
- Brave `source` 映射为 `sourcePageUrl`。

## 6. 正式 release

### Hook R18

```text
release/Hook/20260814-image-search-runtime-fix-r18/hook.exe
```

| Item | Value |
| --- | --- |
| source commit | `091f5d5c379027a4fe633da11434b48b0949de35` |
| bytes | `7036416` |
| SHA-256 | `9f514b1a61f21bd337e40dd890f471c8cba400cde29c3a2e85a717baa148b72b` |

### Loom R28

```text
release/Loom/20260814-image-search-runtime-fix-r28
```

| Item | Value |
| --- | --- |
| source commit | `e17a6ae71add5d3554b051524178d6e628af0ee4` |
| `gitDirty` | `false` |
| `Loom.exe` | `9eb26fad75109e29df0c41828d7f733eec1360213743920b36eb6434d64c0554` |
| `runtime/loom-daemon.exe` | `0258a54a65b2e0530a39d010af1df8e4361773df43d33b342a68a6be1f1b7a96` |
| `packages/frameworks/mcp.zip` | `cc418d16ca55c34b4c493eee9646cb8f4ded34638053027dd134e3ec4e9b57b9` |
| `packages/arts/custom-image-search.zip` | `f4094b09394c0ceb6f43742244b79e93c0d3dd065add73aa3139bd410b8aa347` |
| desktop ZIP | `f0d41d111043a0749b33d87d062809f475ada79d1a898ff1deab9d8a96bafb6b` |

R28 `checksums.sha256` 包含 49 项。正式 Art ZIP 已独立打开确认：

- version `0.3.1`；
- `runtime/image-search-mcp.ps1` 存在；
- command 为 package-local runtime；
- `BRAVE_API_KEY -> brave_api_key` credential alias contract 存在；
- `result.source -> sourcePageUrl` 已进入 release bytes。

release verifier 结果：

```text
filesChecked = 49
smoke = passed
hookCanvasSmoke = passed
hookErrorPreviewSmoke = passed
frameworkArtStoreHookSmoke = passed
pluginBoundarySmoke = passed
surfacePrototypeSmoke = passed
authoredArtCreationSmoke = passed
```

## 7. 当前未完成内容

### 7.1 R18/R28 原生 600 秒双端验收

最新 exact-hash preflight：

```text
Hook/artifacts/runtime-performance/hook-loom-surface-candidate/
  20260814-144933-hook-loom-surface-cb75a03e3b58/summary.json
```

结果为 `blocked_existing_hook`。preflight 已验证 R18/R28 路径和 SHA-256，但没有启动
候选，也没有终止现存用户 Hook/Loom。必须等用户正常退出旧实例后，再运行 packaged
Hook GUI -> packaged Loom 的 pairing、图片搜索候选渲染、formal image output、restart
recovery 和 600 秒 soak。

### 7.2 用户级历史 MCP 配置不属于新 Art 依赖

当前用户控制面仍有一个历史 `Brave Search` MCP server 配置；没有 ToolDefinition 引用
其 server ID，R28 `custom-image-search@0.3.1` 也不依赖它。它是用户级配置而不是仓库
compatibility code。本轮未删除该用户配置，以避免破坏可能存在的手工 MCP 用途；
独立审查应把它与新 Art 的 package-local MCP 明确区分。

### 7.3 框架全局未实现边界保持不变

完整 OS sandbox、Unix keyring/强资源限制、Workflow child 自动引用计数与 orphan GC、
hosted marketplace，以及三个 Surface prototype 进入正式 release catalog，仍按独立
复核交接文档第 4 节处理。本阶段没有把这些更强目标伪报为已完成。

## 8. 不应补回的实现

不要为了旧 session 或历史 MCP 配置新增：

- `input_image` 兼容 alias；
- Hook-local image-search executor；
- Art-ID-specific host/UI switch；
- npm/npx MCP fallback；
- plaintext/default manifest key；
- registry stale `artUserSettings` fallback；
- old ArtLoom/AHRP route 或 event alias。

发生 canonical 数据缺失或冲突时，应修复当前权威来源或 fail closed，而不是恢复旧
兼容层。
