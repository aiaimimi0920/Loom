# Phase 74: 图片搜索 MCP 参数边界与服务可见性

日期：2026-08-14

## 状态

源码实现、针对性回归、Hook/Loom 完整源码门禁、独立安全审查、Hook R19、Loom R30、
Loom release verifier、正式 daemon 的隔离 MCP 服务投影验证，以及 R29 `mcp@0.2.0`
到 R30 `mcp@0.2.1` 的升级验证均已完成。

当前仍未执行 R19/R30 原生 Hook/Loom 双端 GUI 验收。用户正在运行 R28 Loom 和既有
Hook 实例；本阶段没有终止、替换或重启这些进程。

## 1. 目的

Phase 72 已把图片搜索 MCP server 收入 Art ZIP，Phase 73 已修复 credential 生命周期，
但现场通过 Hook 手动触发图片搜索时仍出现：

```text
tool registry error: framework `mcp` for tool `custom-image-search` failed
[framework_runtime_host_error: MCP tools/call `brave_image_search` failed:
MCP server returned JSON-RPC error:
{"code":-32000,"message":"unknown image-search argument: __exec_manualTrigger"}]
```

同时，Loom 的“服务 -> MCP 服务”页面只展示用户级 MCP store，无法看到已安装图片搜索
Art 自带并实际使用的 package-local MCP 服务。

本阶段目标是修复 canonical Hook -> Loom -> Framework -> Art 调用边界，并让同一服务
在 Loom 管理面中安全可见；不是重新建立用户级 MCP 副本，也不是恢复旧图片搜索 helper
或兼容执行链。

## 2. 三层根因

### 2.1 Hook 内部控制字段进入 Art 参数

Hook 用参数变更触发节点重新执行。旧实现把手动触发哨兵放入普通参数映射，随后
`execute_art.parameters` 原样转发，因此 `__exec_manualTrigger` 可能越过 Hook 边界。

`result_index` 与这些控制字段不同。它是图片搜索 Art 的正式候选选择参数，必须保留并
交给 Art runtime。

### 2.2 MCP Framework 把完整 Art params 当作 MCP tool arguments

MCP Framework runtime host 原先合并 manifest arguments、inputs 和 params，除显式
disabled 参数外全部发送给 MCP tool。图片搜索 MCP 的 input schema 只允许 `query` 和
`count`，并使用 `additionalProperties: false`，所以它正确拒绝了 Hook 控制字段以及其他
Art-level 参数。

问题不在 MCP server 的严格校验，而在通用 Framework host 没有守住 Art 参数与 MCP
tool 参数的边界。

### 2.3 MCP 页面只读取用户 store

图片搜索 MCP 的配置位于已安装 Art ToolDefinition 的 `metadata.mcp`，由 Framework
runtime host 执行；它不属于 `<control-plane>/mcp/servers.json`。原来的
`GET /v1/mcp/servers` 只返回用户 store，因此运行时能力存在，但管理页面不可见。

## 3. Canonical 调用结构

修复后的执行和管理路径是：

```text
Hook generic Art node
  -> 剥离 Hook 内部控制字段
  -> loom.hook.v1 art.execute
  -> Loom installed Art Tool Registry
  -> neuro.official/mcp Framework package
  -> 按 MCP tool inputSchema 建立参数边界
  -> Art package runtime/image-search-mcp.ps1
  -> brave_image_search(query, count)
  -> Art runtime/main.ps1(result_index)
  -> formal image result + candidates

Loom MCP service page
  -> GET /v1/mcp/servers
  -> 用户 MCP store 条目
     + 已安装 Art Tool Registry 的只读动态投影
```

两条路径共享已安装 Art 与 Tool Registry 这一权威来源，不创建第二份图片搜索 MCP
配置，也不创建第二套生命周期。

## 4. 实现

### 4.1 Hook 控制参数隔离

Hook 现在使用 canonical 内部字段：

```text
__exec_manualTrigger
```

以下字段不会持久化到 session、进入 workflow sync snapshot，或进入正式 Art execute
parameters：

```text
__exec_*
__ui_resize
force_update
```

`force_update` 不再是生产触发机制；它只保留在统一 sanitation predicate 中，用于删除
历史状态中可能存在的旧字段。仓内没有生产调用点，也没有为它保留兼容执行语义。

`result_index` 继续作为 Art-owned 参数进入执行，用来选择图片候选。

主要实现位置：

```text
Hook/src/constants.ts
Hook/src/hooks/useNodeParameters.ts
Hook/src/components/UnitParamsPanel.tsx
Hook/src/services/artParamSecurity.ts
Hook/src/services/graphImageResolution.ts
Hook/src/services/sessionStickerMapping.ts
Hook/src/services/syncService.ts
```

### 4.2 MCP tool schema 参数边界

`framework-packages/runtime-host/src/mcp.rs` 现在读取 MCP `tools/list` 返回的
`inputSchema`。当 schema 是可安全判定的简单顶层对象，且显式声明：

```json
"additionalProperties": false
```

host 只把 `properties` 声明的字段传给 tool。图片搜索 MCP 因此只收到：

```text
query
count
```

以下 Art 或 Hook 层字段不会再发送给 `brave_image_search`：

```text
result_index
__exec_manualTrigger
force_update
```

对 `patternProperties`、`allOf`、`anyOf`、`oneOf`、`$ref` 等复杂 schema，host 不做
可能错误的不完整删参，而是保留参数并由 MCP server 自己执行 schema 校验。这避免为了
修复图片搜索而破坏更一般的 MCP schema 语义。

另外，`McpExecution` 不再把实际 tool arguments 回显到 `frameworkData.mcp`。这同时
消除了参数中潜在 secret 被 Art runtime、日志或 UI 再次暴露的风险。

### 4.3 图片搜索 Art 0.3.2

`neuro.official/custom-image-search` 已提升到 `0.3.2`。Art runtime 从 Art-level：

```text
params.result_index
```

选择正式候选，不再固定返回第一项。MCP tool 仍只负责 `query` 和 `count`，候选选择仍
属于 Art runtime，不被错误下推给 MCP server。

正式 Art ZIP 继续包含：

```text
manifest.json
art.runtime.json
runtime/common.ps1
runtime/image-search-mcp.ps1
runtime/main.ps1
```

### 4.4 Art-managed MCP 动态投影

Loom daemon 的 `GET /v1/mcp/servers` 现在合并两类条目：

1. 用户在 MCP 服务页面配置、由用户 store 持久化的普通 MCP server；
2. 从已安装 Art Tool Registry 动态生成的只读 Art-managed MCP 条目。

动态条目包含：

```text
managed=true
source="art"
ownerArtId
serverId
toolName
readOnly=true
editable=false
deletable=false
credentialRequired
credentialBound
```

安全边界：

- 不投影 `env`、`headers`、`credentialEnv` 或 `credentialHeaders`；
- `command` 固定投影为空字符串，不能把条目直接交给通用 `/v1/mcp/test`；
- credential 只投影 `required/bound` 布尔摘要，不返回 alias 或明文；
- managed ID 使用保留前缀 `art-mcp:`，owner Art ID 和 server ID 分段做十六进制编码；
- 用户 store 中伪造或遗留的保留前缀条目不会进入列表；
- 对 managed ID 的 PUT/DELETE 均返回 HTTP 409 和
  `mcp_server_managed_by_art`；
- Art uninstall 后投影随 Tool Registry 状态自动消失。

Desktop 类型和 `McpHub` 识别同时满足 `managed=true` 与 `source="art"` 的条目，不把
任意普通 `readOnly` server 错判成 Art-managed。页面对 managed 条目不显示编辑、
测试、启停或删除操作，只显示：

```text
只读 · 请在 Art 管理中配置
```

### 4.5 为什么不写入 `servers.json`

把 Art 内置 MCP 再写入用户 `servers.json` 会制造两个相互漂移的真相源：

- Art 安装、升级、回滚、卸载生命周期；
- 用户 MCP server 的创建、编辑、启停、删除生命周期。

它还会把 Art-scoped credential alias、package-local executable 和用户 MCP 配置混在
一起。当前动态投影只描述已存在的运行时能力，执行仍由 Art -> Framework canonical
路径负责，管理仍由 Art settings 负责。

因此本阶段明确没有：

- 向 `<control-plane>/mcp/servers.json` 复制图片搜索 MCP；
- 创建全局 `Brave Search` 依赖或 alias；
- 展开 credential alias 为普通环境变量或 header；
- 为 `custom-image-search` 增加 host/UI Art-ID 特判。

### 4.6 MCP Framework 0.2.1 与已安装控制面升级

R29 首次发布后追加升级审计发现：R28 与 R29 都把 MCP Framework 标记为 `0.2.0`。
FrameworkRegistry 的底层安装以 package digest 建立不可变版本目录，因此显式安装同版本
但不同 digest 的 ZIP 仍能切换 runtime；但是 Desktop packaged bootstrap 只比较已安装
版本和打包 catalog 的版本字符串。用户沿用 R28 控制面启动 R29 时，`0.2.0 == 0.2.0`
会跳过自动升级。

这意味着 R29 在全新控制面的服务投影和执行验证虽然真实通过，但不能证明既有用户控制面
自动获得了新 MCP runtime。R29 因此被 R30 取代，不作为最终交付版本。

R30 将 MCP Framework 正式提升为：

```text
neuro.official/mcp@0.2.1
```

Desktop bootstrap 看到已安装 `0.2.0` 与打包 `0.2.1` 不同后，会调用：

```text
POST /v1/frameworks/mcp/upgrade
```

并在 Framework 变化后重新安装依赖它的 bundled Art，使 Art dependency lock 指向新的
active Framework。这里没有增加 R29/R28 专用分支或旧 runtime fallback，只执行正常的
Framework 版本升级语义。

## 5. 默认 key 的安全定义

“默认 key”仍然只表示用户已通过 Art settings 建立的 credential binding：

1. `art-user-settings.json` 保存 `brave_api_key -> credential alias`；
2. Windows `CredentialStore` 保存当前用户 DPAPI-protected credential value；
3. Framework 执行时按 Art scope 将其映射到 package-local MCP 进程的
   `BRAVE_API_KEY`。

本阶段没有把真实 key 写入 Git、manifest、普通 defaults、Hook session、workflow
payload、MCP 列表响应、日志、文档、hash、长度、前缀或片段。隔离验证使用随机
生成且随临时控制面一同删除的测试 credential，只验证 `credentialBound` 摘要变化，
没有调用真实 provider。

## 6. 验证

### 6.1 源码与 package 门禁

2026-08-14 fresh 结果：

| Gate | 结果 |
| --- | --- |
| Hook targeted control/security/image-search | 5 files / 50 tests passed |
| Hook frontend full suite | 253 files / 1067 tests passed |
| Hook production typecheck | passed |
| Hook production build | passed |
| MCP Framework runtime host | 8 passed |
| `loom_tool_registry --lib` | 122 passed |
| `loom-daemon --lib --test-threads=1` | 206 passed |
| Loom Rust formatting | passed |
| Loom desktop | 138 passed；typecheck/build passed |
| Desktop packaged bootstrap `0.2.0 -> 0.2.1` | 1 passed |
| Framework package contract | 4 Frameworks passed |
| Sample Art package contract | 6 Arts passed |
| Package-local image-search MCP direct contract | passed |
| Curated Art direct runtime | 6 cases passed |
| Installed Framework + Art formal execution | 6 executions passed |

`npm run typecheck:test` 仍会报告仓内既有测试 fixture/canvas mock 类型问题；这不是本轮
新增回归。本轮完整 Vitest、生产 typecheck 和 production build 均通过。

新增 native Desktop bootstrap 测试单独运行通过。随后执行完整
`cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib` 时，当前既有 baseline
为 26 passed / 8 failed：首个无关失败是 Loom cache settings fixture 使用 snake_case
字段后得到 `None`，其 panic 使共享 `ENV_LOCK` poisoned，并连带 7 个环境变量测试失败。
本阶段没有修改该 cache/settings 逻辑，也没有把这些失败误报为通过。

### 6.2 独立审查修复

三组独立审查覆盖 Hook 参数边界、MCP Framework host 和 managed MCP 页面。审查发现
并在 R19/R29 前修复：

1. MCP arguments 回显造成的潜在 secret 风险；
2. managed ID 与用户 server ID 的冲突；
3. managed ID 的后端 PUT/DELETE 绕过；
4. UI 对 `readOnly` 的过宽 Art-managed 判定；
5. managed projection 暴露 executable command 的隔离风险；
6. composed/pattern JSON Schema 被错误删参的风险。

### 6.3 R29 正式 daemon 隔离运行时验证

验证直接使用：

```text
release/Loom/20260814-image-search-mcp-visibility-r29/runtime/loom-daemon.exe
```

并使用空闲 loopback 端口、全新临时 `LOOM_CONTROL_PLANE_ROOT`、
`LOOM_CONFIGURATION_ROOT` 和 run store。只安装 R29 自带：

```text
packages/frameworks/mcp.zip
packages/arts/custom-image-search.zip
```

实际 `GET /v1/mcp/servers` 返回且只返回一个对应图片搜索条目：

```text
serverId=neuro-image-search
source=art
ownerArtId=neuro.official/custom-image-search
toolName=brave_image_search
managed=true
readOnly=true
editable=false
deletable=false
command=""
credentialRequired=true
credentialBound=false -> true
```

同时验证：

- `env`、`headers`、`credentialEnv`、`credentialHeaders` 均不存在；
- managed ID PUT 返回 HTTP 409 / `mcp_server_managed_by_art`；
- managed ID DELETE 返回 HTTP 409 / `mcp_server_managed_by_art`；
- 安装、绑定、列表、拒绝修改和卸载全过程均未生成 `servers.json`；
- Art uninstall 后图片搜索 managed MCP 条目自动消失；
- 本轮 R29 daemon 已退出，临时控制面已删除；
- 用户原有 R28 Loom/daemon 和 Hook 进程未被停止或修改。

这组验证证明 managed MCP 投影本身进入了正式 release bytes，但后续升级审计发现 R29 的
Framework 版本号没有变化，因此 R29 不再作为最终用户升级包。

### 6.4 R29 到 R30 的隔离升级验证

验证使用同一个临时控制面依次启动 R29 和 R30 正式 daemon：

1. 用 R29 安装 `mcp@0.2.0` 和 `custom-image-search@0.3.2`；
2. 停止本轮 R29 daemon；
3. 用 R30 daemon 打开同一个控制面，确认升级前仍是 `mcp@0.2.0` 且 ready；
4. 使用 R30 打包的 `mcp.zip` 调用正式 upgrade API；
5. 确认 active Framework 变为 `mcp@0.2.1` 且 ready；
6. 重新安装 R30 bundled image-search Art；
7. 确认 Art-managed 图片搜索 MCP 投影仍存在且不暴露 credential/executable 字段。

实际结果：

```text
before: mcp@0.2.0 ready=true
after:  mcp@0.2.1 ready=true
managed serverId=neuro-image-search
managed ownerArtId=neuro.official/custom-image-search
servers.json absent=true
```

独立 Desktop bootstrap 回归同时证明版本不同时会请求
`POST /v1/frameworks/mcp/upgrade`，并随后请求 bundled Art reinstall。验证结束后，本轮
R29/R30 daemon 和临时控制面均已清理，用户原有进程未被修改。

## 7. 正式 release

### 7.1 Hook R19

```text
release/Hook/20260814-image-search-mcp-visibility-r19/hook.exe
```

| Item | Value |
| --- | --- |
| source commit | `e38a96588b221961822f85be252805547820ab9f` |
| version | `0.1.7` |
| bytes | `7036416` |
| SHA-256 | `a851b75221c4d08263cfea9865faecc728bc330f7f6ee1dfcf9387f7057179d7` |
| Authenticode | `NotSigned` |

### 7.2 Loom R30

```text
release/Loom/20260814-image-search-mcp-upgrade-r30
```

| Item | Value |
| --- | --- |
| source commit | `74de815d9a42f89f07a4e35d1d71b6f9fc94bfc3` |
| `gitDirty` | `false` |
| MCP Framework version | `0.2.1` |
| `Loom.exe` | `c84ebca6075edad82b4f5926d7efc93a2a1cee0b1f44006055b17cfb8c4a8c4c` |
| `runtime/loom-daemon.exe` | `9f04abc044c747188ce10a603aa7f8ab6d5ceb49db6ac1f3161a4f8070bbb5d1` |
| `packages/frameworks/mcp.zip` | `de9e2f8b351f873473943bca0082ca3af899b2887fc600f2f78cc94244741418` |
| `packages/arts/custom-image-search.zip` | `bce52be4f183763edfb377f81582319a6e7733f03c97556d7c252bddc87dfd57` |
| image-search ZIP bytes | `10182` |
| desktop ZIP | `85cc46c49b52edfca76d99ddfe172e6429309b07d267240671fe77c3cb212cdd` |

R30 `checksums.sha256` 包含 49 项。正式 Framework/Art ZIP 已独立确认：

- MCP Framework package version 为 `0.2.1`；
- package version 为 `0.3.2`；
- `runtime/image-search-mcp.ps1` 存在；
- `runtime/main.ps1` 包含 `result_index` 候选选择；
- MCP Framework ZIP 包含新构建的 `runtime/loom-framework-mcp.exe`；
- release manifest 的 `gitHead` 为上述 R30 source commit，`gitDirty=false`。

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

## 8. 当前未完成内容

### 8.1 R19/R30 原生双端 GUI 验收

本阶段完成时，用户仍在运行：

```text
release/Loom/20260814-image-search-runtime-fix-r28/Loom.exe
release/Loom/20260814-image-search-runtime-fix-r28/runtime/loom-daemon.exe
```

因此尚未运行 R19 Hook -> R30 Loom 的原生 pairing、MCP 服务页面可见性、候选点击、
formal image output、restart recovery 和 600 秒 soak。必须等用户正常退出旧实例后再执行；
不能为了验收强杀、重启或替换现场进程。

### 8.2 R30 未重复消费真实 Brave credential

Phase 73 已对 R28 完成隔离真实 provider 搜索与图片下载。R30 本阶段专注控制参数、
Framework schema 边界和 MCP 服务投影；正式 package/installed execution 测试继续通过，
但 R30 隔离升级验证没有读取或消费用户真实 key，也没有再次发起真实 Brave 请求。

这不是已知代码缺口，但属于最终 R19/R30 原生验收需要覆盖的外部 provider 边界。

### 8.3 既有框架全局边界保持不变

完整 OS sandbox、Unix keyring/强资源限制、Workflow child 自动引用计数与 orphan GC、
hosted marketplace，以及 Surface prototypes 进入正式 release catalog，仍是独立路线，
不应被本阶段图片搜索/MCP 修复误报为完成。

### 8.4 发布签名

Hook R19 的当前 Authenticode 状态为 `NotSigned`。发布物内容与 SHA-256 已固定，但若正式
分发策略要求平台代码签名，仍需由独立签名发布流程完成；不能通过兼容代码替代签名。

### 8.5 Native Desktop 既有测试与格式 baseline

standalone `apps/desktop/src-tauri` 的完整 Rust library suite 当前不是全绿：

```text
26 passed
8 failed
```

首个失败位于既有 Loom cache settings fixture；其后的环境相关失败由共享 mutex poison
连带产生。新增 `packaged_art_bootstrap_upgrades_changed_framework_version` 单测独立通过，
R30 release verifier 也全部通过，因此该 baseline 不改变本阶段 MCP 升级结论，但仍应由
独立 Desktop settings/cache 工作项修复。

对该 standalone manifest 运行 `cargo fmt --check` 还会报告同一文件中本阶段修改范围外的
既有格式差异；本阶段新增测试块已按 rustfmt 输出调整，没有批量改写无关代码。主 Loom
workspace 的 Rust formatting gate 仍通过。

## 9. 明确不应补回的实现

不要为历史 session、旧 MCP 配置或单个 Art 新增：

- `force_update` 执行兼容语义；
- Hook-local image-search executor；
- desktop-specific `mcpImageSearch` helper；
- Art-ID-specific host/UI switch；
- npm/npx MCP fallback；
- 用户 `servers.json` 中的图片搜索 MCP 副本；
- plaintext/default manifest key；
- old ArtLoom/AHRP route 或 event alias。

发生 canonical 数据缺失或参数冲突时，应修复当前权威来源或 fail closed，而不是恢复
Legacy 兼容层。

## 10. 结论

图片搜索 Art 当前已采用新的 Hook -> Loom -> Framework -> package-local MCP canonical
调用框架。原始 `__exec_manualTrigger` 错误的两个泄漏边界都已修复；图片候选选择由
Art-owned `result_index` 正确执行；MCP 页面通过只读动态投影显示同一个 package-local
服务，不创建第二份用户 MCP 配置或 credential 真相源。

从源码、package、release verifier、Desktop bootstrap 回归和正式 R29 -> R30 daemon
升级证据看，本阶段开发目标已经完成。剩余工作是等待现场旧进程自然退出后执行
R19/R30 原生 GUI/真实 provider 验收，
以及按独立发布策略处理代码签名。
