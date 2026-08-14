# Loom / Hook Art 框架重构独立复核交接文档

日期：2026-08-13

状态：已完成实现方整理，等待独立 AI 按源码、测试与运行证据复核。

本文面向没有参与实现过程的审查者。它不是“完成证明”，而是审查入口。
审查者应以本文定位到的源码、提交、release manifest 和 acceptance JSON 为准，
不要仅依据本文或其他进度文档中的结论判定完成。

## 0. 复核范围与版本定位

需要同时检查三个 Git 层级：

1. Hook 子仓库：`../Hook`
   - 本次实现提交：`0d952ec391012e5ba5292932b06a2341f5da5eab`
   - 提交标题：`feat(art): adopt canonical Loom Hook protocol`
2. Loom 子仓库：本文所在提交。审查时在 Loom 根目录执行
   `git rev-parse HEAD`，该提交包含本轮 Loom 实现与本文。
3. Neuro 父仓库：审查父仓库当前提交中的 `Hook`、`Loom` gitlink，确认它们
   分别指向上述两个子仓库提交。父仓库的 Gateway、Platform、Talk、Tea 和其他
   工作区修改不属于本次 Art 框架提交。

正式运行候选位于父仓库 release 根，而不是子仓库：

- Hook R14：
  `../release/Hook/20260813-loom-hook-v1-surface-wire-r14/hook.exe`
- Loom R23：
  `../release/Loom/20260813-loom-hook-v1-surface-wire-r23`

release 与 runtime evidence 按仓库策略没有强制加入源码 Git 提交；复核时需要
在共享工作区独立核对其存在性、manifest、哈希和 JSON 证据。

---

## 1. 框架重构目的

### 1.1 消除 Loom 与 Hook 之间的双重实现和隐式耦合

旧实现同时存在 ArtLoom、ArtHook、AHRP、未命名 bridge 路由、Hook 本地 Art
执行器以及若干 Art-ID 专用分支。其问题不是简单的命名陈旧，而是：

- Hook 可能绕过 Loom 直接解释或执行 Art；
- 新增 Art 需要修改宿主源代码；
- 相同语义在 HTTP、native bridge、browser client 和测试 fixture 中有多套字段；
- preview、formal result、cancellation 和 Surface 生命周期难以建立单一真相源；
- 旧字段、旧目录和旧 session 的自动推断会掩盖错误数据，而不是 fail closed。

重构目的，是让 Loom 成为唯一的 Art 包、Framework、执行与状态所有者，Hook
只作为画布和交互客户端消费通用能力与结果。

### 1.2 建立可扩展、可验证的 Framework / Art 插件边界

目标不是把旧硬编码执行器换一个目录，而是使第三方可以在不修改 Loom 或 Hook
源码的情况下提供：

- 独立 Framework 包；
- 独立 Art 包；
- publisher-qualified 身份；
- manifest、Schema、签名、信任、权限与资源声明；
- 安装、升级、回滚、禁用、卸载、恢复和运行证据；
- 标准化 process + JSON ABI 与 Loom/Hook wire contract。

### 1.3 统一 Art node 的数据和生命周期语义

重构要把下面这些语义变成协议，而不是 UI 或某个 Art 的特殊行为：

- capability 驱动的输入、输出和参数；
- publisher-qualified packaged Art ID；
- typed formal values；
- preview 与 formal output 严格分离；
- preview revision 与 result revision 独立；
- generation replacement、显式取消和 late-result rejection；
- Surface snapshot、patch、action、confirmation、resource、recovery 和 dispose；
- browser/native Hook 客户端行为一致。

### 1.4 在尚未大规模部署的阶段清除 Legacy 兼容成本

本项目没有需要无损迁移的大规模生产用户数据，因此本轮原则是：

- 不扫描旧 app-data identifier；
- 不迁移旧 package layout、session shape 或 config；
- 不接受旧 public field alias；
- 不保留旧路由、旧事件名或旧执行器；
- canonical 字段缺失或类型错误时 fail closed；
- 不提供 legacy migration utility 或 compatibility shim。

这项原则不等于删除可靠性机制。运行失败 fallback、恢复、回滚、checkpoint、
journal、tombstone、共享内存失败后的 typed inline fallback 等属于当前产品能力，
不是历史兼容层。

---

## 2. 框架重构目标

### 2.1 所有权边界

| 领域 | 目标所有者 | 目标约束 |
| --- | --- | --- |
| 包发现、安装、Framework 选择 | Loom | Hook 不读取或加载包代码 |
| Art 执行、取消、资源、运行证据 | Loom | 所有执行都经过 Loom 监督边界 |
| preview/formal revisions | Loom | 失败或取消不能覆盖最后成功 formal result |
| 画布、节点控件、连线、用户交互 | Hook | 只依据 capability 与通用协议渲染 |
| graph input resolution | Hook | 不按具体 Art ID 分派 |
| Surface durable state | Loom | Hook 只持有/恢复实例引用和展示状态 |

验收标准：添加新的 Art 或第三方 Framework 只需要 package/manifest，不需要在
Loom 或 Hook 添加该 Art ID 的执行分支。

### 2.2 标准化 Loom / Hook 调用协议

唯一跨应用 Art bridge 协议应为 `loom.hook.v1`。公开 Rust 类型、语言无关
Schema 和 Hook mirror 必须一致：

- Loom Rust：`crates/loom_protocol/src/hook.rs`
- JSON Schema：`protocol/schemas/hook-message.v1.schema.json`
- Hook TypeScript：`../Hook/src/services/protocol.ts`
- Hook native bridge：`../Hook/src-tauri/src/loom_hook.rs`

标准方法：

1. `loom.hook.handshake`
2. `loom.hook.capabilities.list`
3. `loom.hook.subscribe`
4. `loom.hook.workflow.sync`
5. `loom.hook.workflow.node.update`
6. `loom.hook.workflow.instantiate`
7. `loom.hook.art.execute`
8. `loom.hook.art.cancel`
9. `loom.hook.art.resources.release`
10. `loom.hook.settings.get`
11. `loom.hook.enhancements.get`
12. `loom.hook.ocr.execute`
13. `loom.hook.translation.execute`

标准 Art 事件：

- `loom.hook.workflow.instantiated`
- `loom.hook.workflow.updated`
- `loom.hook.capabilities.updated`
- `loom.hook.art.ack`
- `loom.hook.art.progress`
- `loom.hook.art.preview`
- `loom.hook.art.result`
- `loom.hook.art.failure`
- `loom.hook.settings.updated`
- `loom.hook.cache.control`

Surface payload protocol 为 `loom.surface.v1`；公开 wire 事件必须使用精确的
`loom.surface.*` 名称，不能接受 `surface`、`surface/snapshot` 等旧名称。

### 2.3 Canonical package identity 与磁盘布局

目标身份为 `publisher/id`。安装后的唯一布局：

```text
<control-plane>/
  frameworks/<publisher>/<id>/
    versions/<version>-<digest>/
    active.json
    locks/
    lifecycle.json
  arts/<publisher>/<id>/
    versions/<version>-<digest>/
    active.json
    locks/
    state/
    cache/
    outputs/
```

目标约束：

- Framework manifest 的 publisher 必填；
- Art manifest 的 `metadata.packageSecurity.publisher` 必填；
- packaged Art 的 `metadata.art.qualifiedId` 必填并与 publisher/id 一致；
- flat `frameworks/<id>`、`arts/<id>` 不生成、不解析、不迁移；
- Framework/Art immutable version 由 version + digest 定位；
- Framework host 侧用 qualified identity 选包和加锁，进入已选包进程 ABI 后使用
  manifest 的 package-local framework ID；
- Art Store 只使用 `arts/<id>/<version>.zip` 和 digest sidecar，不保留 flat latest
  copy 或 `/latest` 下载路由。

### 2.4 Framework 标准化

正式 Framework 包为：

1. `neuro.official/process`
2. `neuro.official/cloud_api`
3. `neuro.official/mcp`
4. `neuro.official/workflow`

其中 command、PowerShell/script 和 Python Art 统一走 `process`，而不是各保留一套
host executor。Framework ABI 为 `loom.framework.v1`；manifest 必须声明 publisher
和 `entry.processModel`，MCP 配置必须声明 transport，成功状态只能是 `success`。

### 2.5 九个历史/原型 Art 迁移

目标是九个源码包都符合 canonical manifest、ports、framework dependency 和
package-local execution：

| 源目录 | Qualified Art ID | Framework |
| --- | --- | --- |
| `art-packages/samples/image-compress` | `neuro.official/custom-1770146354922` | process |
| `art-packages/samples/remove-bg` | `neuro.official/custom-remove-bg-cloud` | cloud_api |
| `art-packages/samples/image-search` | `neuro.official/custom-image-search` | mcp |
| `art-packages/samples/color-transfer` | `neuro.official/custom-1770131241684` | process |
| `art-packages/samples/image-blend` | `neuro.official/custom-image-blend-script` | process |
| `art-packages/samples/image-blend-compress` | `neuro.official/custom-image-blend-compress-workflow` | workflow |
| `art-packages/surface-prototypes/dashboard` | `neuro.official/surface-device-dashboard` | process |
| `art-packages/surface-prototypes/form` | `neuro.official/surface-project-form` | process |
| `art-packages/surface-prototypes/stock-card` | `neuro.official/surface-stock-card` | process |

Workflow Art 的 child dependency 与 `uses` 也必须 qualified；不能依赖短 ID 或
被删除的 duplicate source。

### 2.6 Canonical canvas 与输出语义

当前两个 producer 各自有一种明确 shape：

1. Hook persisted session：`stickers` / `links`，link 字段为 `fromUnitId`、
   `toUnitId`、`fromPortId`、`toPortId`；内部 node type 是 `sticker` / `art`。
2. `loom.hook.workflow.sync`：`nodes` / `edges`，edge 字段为 `source`、`target`、
   `sourceHandle`、`targetHandle`；公开 node type 是 `sticker` / `artNode`。

不接受历史第三套 `units`、`sourceNodeId`、`targetNodeId`、旧 port alias。Art 节点
必须有 canonical Art ID；缺 type、未知 type、只有 artId、或 `sticker + artId`
不能被猜测成 Art。

formal value 的类型只能是 `value`、`inline_resource`、`shared_memory`、`resource`。
preview 仅展示；下游执行、保存、持久化和导出只消费 formal output。

每个 `art.execute` 必须显式声明 `outputTransports`。共享内存释放命令使用独立的新
`requestId`，并通过 `executionRequestId` 指向原执行；Loom 将每个输出 handle 绑定到
精确的 device / execution / node / generation。跨请求、跨设备、跨节点、跨 generation
或混入非所属 handle 的释放会整体失败，不会释放任何映射。取消、generation 替换、
bridge reset 与 terminal cache eviction 会自动回收仍未显式释放的 handle。

### 2.7 生命周期、安全与发布目标

目标包括：

- secure ZIP extraction；
- signature、trust、revocation；
- immutable activation、dependency lock、upgrade、rollback；
- lifecycle journal、tombstone 和 startup recovery；
- process tree、timeout、stdout/stderr 和资源限制；
- scoped credential、host-mediated network；
- cancellation、checkpoint、Surface remount/restart recovery；
- source immutability：包生命周期不得修改 Loom/Hook 源码；
- Loom 与 Hook 各自生成正式 release；
- 使用精确路径和 SHA-256 做原生双端 600 秒验收。

---

## 3. 当前实现状况

### 3.1 总体结论

在“Loom/Hook 调用框架、标准化协议、九个 Art 源码迁移、旧公开调用面删除”这一
范围内，当前源码实现已闭环，并通过全量源码测试、构建、第三方 plugin boundary
以及真实 Framework + Art Store + Hook smoke。它不是仅改文档或仅跑 unit tests。

但是，2026-08-14 独立审查新增的 `outputTransports`、严格 formal-value 校验、
多输出端口保留、shared-memory execution ownership/release、package version 收紧和
canvas shape fail-closed 尚未进入新的 Hook/Loom release。因此 R14/R23 与既有 600 秒
native acceptance 只能作为历史基线，不能作为当前审查后源码的发布证据。

但是，“所有内部对象都必须 qualified”“默认生产级信任/OS 沙箱”“九个 Art 全部
进入正式 release catalog”“release 能由最终干净提交直接追溯”这些更强命题目前
不能宣称完成，详见第 4 节。

### 3.2 Loom / Hook 新调用框架已经落地

- Loom daemon 是 Art 执行、取消、workflow sync/instantiate、capability、settings、
  OCR/translation 和 Surface 推送的服务端。
- Hook 的 browser/native clients 都发送 `loom.hook.v1` 请求并按 protocol、
  requestId、nodeId、device/generation 匹配响应。
- Hook 不加载 Art package code；旧 `mock_artloom.rs` 与前端 ArtLoom startup 已删除/
  重命名为 Loom Hook client。
- Art-specific local delivery/execution 分支已删除；候选列表使用通用
  `kind=image.candidates` metadata。
- exact method/event 列表在 daemon runtime 和 acceptance summary 中可直接核对。

主要实现入口：

- `apps/daemon/src/lib.rs`
- `apps/daemon/src/hook_canvas.rs`
- `apps/daemon/src/surface_actions.rs`
- `crates/loom_hook_bridge/src/lib.rs`
- `crates/loom_protocol/src/hook.rs`
- `../Hook/src-tauri/src/loom_hook.rs`
- `../Hook/src/services/api.ts`
- `../Hook/src/services/protocol.ts`

### 3.3 Publisher-qualified package boundary 已落地

- `FrameworkPackageManifest.publisher` 已是 required field，Framework schema 同步。
- Plugin CLI scaffold 直接要求 publisher，不再生成 publisher-less package。
- Art install 缺 `metadata.packageSecurity.publisher` 会失败。
- Framework/Art package install、runtime、recovery 和 tombstone 路径只走
  `<publisher>/<id>` layout。
- 旧 flat layout lookup/migration 和 package execution directory fallback 已删除。
- 同 local ID、不同 publisher 的包进入独立根目录。
- workflow child refs 和 sample workflow `uses` 已 publisher-qualified。

主要实现入口：

- `crates/loom_protocol/src/lib.rs`
- `apps/plugin-cli/src/lib.rs`
- `crates/loom_tool_registry/src/install.rs`
- `crates/loom_tool_registry/src/framework.rs`
- `crates/loom_tool_registry/src/framework_process.rs`
- `crates/loom_workflow_store/src/lib.rs`
- `apps/art-store/src/lib.rs`

### 3.4 九个 Art 源码包已迁移

- 六个 sample package 均通过 manifest/package contract；
- 三个 Surface prototype 均通过 validate、pack、digest sidecar 和 runtime smoke；
- 每个 package 都包含 publisher、qualifiedId、globalId、framework dependency、
  canonical ports 与 package-local runtime；
- workflow Art 的两个 child refs 已 qualified；
- `resources/workflow-arts/image-blend-compress` duplicate source 已删除；
- Surface action 由 package-local runtime 处理，host 没有按 prototype Art ID switch。

### 3.5 Legacy 生产调用面已删除

已经删除或拒绝：

- `/v1/artloom-compat/*`、`/v1/python-arts/*`；
- `art_loom/*`、`art_hook/*`、`art/process`、AHRP 和 unnamespaced bridge dispatch；
- 旧 Surface wire names；
- Hook-local pixelate/blur/checkerboard Art executor；
- old settings/MCP/manifest/response/scalar/node geometry aliases；
- reversed workflow image binding normalization；
- missing type 或 stale `sticker + artId` 的 Art 推断；
- old app-data identifier discovery；
- flat package roots/latest copies；
- per-Art installer wrapper 和重复 workflow Art source；
- 无校验 Workflow Store read/write 和 bare workflow child refs。

没有实现旧数据迁移工具；遇到旧数据应明确失败或被视为非 canonical，而不是静默
修复。

### 3.6 当前可靠性能力

以下能力保留且有测试，它们不是 Legacy shim：

- Windows capture WGC-first，运行失败后 GDI fallback；显式
  `HOOK_CAPTURE_BACKEND=gdi` 仅用于诊断；
- shared memory 创建失败时使用 typed `inline_resource`；
- browser/native Hook clients；
- Surface `fallbackScene` 协商；
- Surface checkpoint、migration、rollback、remount、restart recovery；
- stream resume、resource lease、journal、tombstone、corruption detection；
- device-bound confirmation/cancellation；
- generation replacement 与 late-result rejection；
- shared-memory output 的 execution ownership、显式 release、重复 release 幂等，以及
  cancellation/replacement/reset/terminal-eviction 自动回收；
- preview/formal revision 独立 stale check；
- 当前 `hook-live -> latest.yaml` workflow storage contract。

### 3.7 验证结果

2026-08-14 审查后源码的 fresh gates：

| Gate | 结果 |
| --- | --- |
| Rust formatting / combined compile | passed |
| `loom_native_image` | passed |
| `loom_process` | passed |
| `loom_protocol` | 25 passed |
| `loom_tool_registry --lib` | 120 passed |
| `loom_workflow_runtime` | 16 passed |
| `loom_workflow_store` | passed |
| `loom-daemon --lib --test-threads=1` | 205 passed |
| Loom desktop | 137 passed；typecheck/build passed |
| Hook Rust | 228 passed；formatting passed |
| Hook frontend | 252 files / 1058 tests passed；typecheck/build passed |
| Framework + Art Store + Hook smoke | 4 Frameworks、6 Arts、6 formal executions passed |
| Third-party plugin boundary smoke | full lifecycle passed |
| PowerShell plugin/release contract gates | passed |

本轮 Framework/Art/Hook 可机读证据：
`target/framework-art-store-hook-smoke/20260814-092704-framework-store-30172-a112e5982f944661b95c4796401233b8/summary.json`。
其中 `result=passed`、`formalHookExecutions.count=6`、
`formalHookExecutions.protocolVersion=loom.hook.v1`，且 bridge stop 后
`running=false`。smoke 结束后再次核对 Hook/Loom/daemon/Art Store 进程和相关 listener，
均为空。

### 3.8 历史 release 与 600 秒原生证据

以下 R14/R23 证据早于 2026-08-14 独立审查修复，不包含本次最终源码。它们只证明
此前候选的 native 运行状态；当前源码若要成为正式产品候选，必须生成新的 Hook 与
Loom release、更新 verifier/default hashes，并以新制品精确 SHA-256 重跑 600 秒双端
acceptance。

Hook R14：

- path：`../release/Hook/20260813-loom-hook-v1-surface-wire-r14/hook.exe`
- bytes：`7019520`
- SHA-256：`341fb0c88a268bd0cece05eacb623e5a3fc02c6238c80c7fe7f66b1854e746d2`

Loom R23：

- path：`../release/Loom/20260813-loom-hook-v1-surface-wire-r23`
- `Loom.exe` SHA-256：
  `02b3cbe635a578c8d100ed330cb341680e843384781a0ba3f301a6c8ff463c9f`
- `runtime/loom-daemon.exe` SHA-256：
  `376f336dcfe97ad83d18d1d9e74397fc36b81f67ac5f6844594012adfd4b75b6`
- Plugin SDK SHA-256：
  `dcba7d0a0e075bb9703982012fe82223b861b54dd45b0061728d2327f70ba9eb`
- manifest：
  `../release/Loom/20260813-loom-hook-v1-surface-wire-r23/manifest.json`

最终 acceptance：

- outer summary：
  `../Hook/artifacts/runtime-performance/hook-loom-surface-candidate/20260813-205423-hook-loom-surface-b89e7c2bd751/summary.json`
- outer run：`20260813-205423-hook-loom-surface-b89e7c2bd751`，passed
- native run：`20260813-205427-a4407e3b25e0`，passed
- 600 秒，402 个 process-tree samples；private bytes
  `154849280 -> 158683136`，增长 `3833856` bytes / `2.476%`，peak
  `160337920`，zero violations；
- 同一 Surface instance：
  `instance:69847ef9-43b3-449b-bbad-7abefc9a049a`；
- revision：`1 -> 4 -> 8`；
- native WebView2/Tauri startup、single-instance refusal、device approval、
  qualified dashboard、action/resource/formal result、normal exit、restart recovery
  和 final teardown 均通过；
- forced Hook cleanup：none；候选进程和选择的 listeners 最终为空。

---

## 4. 当前未实现内容

本节故意采用比“Phase 71 complete”更严格的口径。第一类是已知平台能力边界；
第二类是相对“绝对 canonical / Legacy zero”仍需决策或继续清理的残余；第三类是
明确不应该实现的旧兼容。

### 4.1 已知平台能力边界（未实现）

#### A. 完整 OS sandbox 未实现

当前 Windows 使用 Job Objects，Unix 使用 process groups。它们提供进程树终止、
timeout、输出上限，以及 Windows memory/active-process 限制，但不是完整的：

- Windows AppContainer / restricted token；
- Linux namespace / seccomp；
- filesystem broker；
- VM isolation。

因此任意外部 executable 的直接 network/filesystem/GPU/clipboard 访问，在默认
`audit` 模式不能宣称被 OS 完全拒绝。`strict` 会在执行前拒绝声明了宿主无法强制
隔离的能力，但这不等于实现了该能力的细粒度沙箱。

#### B. Unix 强资源限制与 OS keyring 未实现

- Unix process-group backend 对 memory 和 active-process-count 声明仍是 advisory；
- Unix persistent credential 使用 owner-only local file fallback，不是 OS keyring；
- 需要硬件/OS secret storage 的部署仍需 external credential broker。

#### C. Workflow child 自动引用计数与 orphan GC 未实现

Workflow child Arts 是独立 package。卸载 parent 不维护依赖引用计数，也不会自动
垃圾回收 orphan child Art/workflow；需要 operator 显式卸载。

#### D. Hosted marketplace 业务能力未实现

Hosted marketplace operations、payment/licensing 和 remote publisher governance
不在当前实现中。当前完成的是本地 package/store/control-plane 与安全边界。

### 4.2 相对最强“Legacy zero / 全量正式发布”定义仍需审查的内容

下面各项不应被实现方悄悄归类为“已经完成”；独立审查者需要决定是接受为产品
设计，还是继续删除/收紧。

#### A. 默认 trust policy 仍为 `AllowUnsigned`

`crates/loom_plugin_security/src/lib.rs` 的 `TrustPolicy::default()` 仍是
`AllowUnsigned`，且 `LOOM_PLUGIN_TRUST_POLICY` 同时接受 hyphen 和 underscore
spelling，例如 `allow-unsigned` / `allow_unsigned`。部分安全文档仍把它称为
“compatibility default”。

这不属于旧 Art wire/layout，但它确实是一个为了开发期 unsigned/local package
保留的宽松默认和字段拼写 alias。若用户的“Legacy 兼容层清零”包含配置 spelling
和安全默认，则该项尚未完成。生产部署当前必须显式使用 `require-trusted`；是否将
默认值直接改为 `RequireTrusted`，以及是否删除 underscore aliases，需要单独评估
现有 bundled/local authoring 流程。

#### B. 通用 `ToolDefinition` 仍允许没有 publisher

packaged Framework/Art 的安装边界已经要求 publisher；但
`crates/loom_tool_registry/src/lib.rs` 中通用 `ToolDefinition` 的
`publisher_identity()` 仍返回 `Option`，`qualified_id()` 在缺 publisher 时仍返回
裸 `id`。这支撑 native/internal/authored/non-package tool 模型，不会创建 flat
package layout。

如果目标只是“所有已安装 package 必须 canonical”，当前实现满足；如果目标是
“任何内部 ToolDefinition 都必须 publisher-qualified”，该目标尚未完成。

#### C. 裸 local ID 的唯一匹配查询仍保留

Registry/API 在恰好只有一个 publisher match 时仍允许用裸 local ID 查询；它不会
生成或解析 flat disk layout，也不能作为 Hook catalog identity。Phase 71 将其定义
为当前查询便利。若绝对禁止任何短 ID，这仍是未删除的例外。

#### D. Built-in native Art ID 仍使用 `core.image.*`

Hook/Loom wire 对 packaged Art 强制 `publisher/id`，但 built-in native image nodes
保留 `core.image.*`。它们不是 package，也不走 publisher package layout。若审查目标
要求连 native built-in 都改为 publisher namespace，这部分未实现。

#### E. 三个 Surface prototype 未进入 R23 正式 Art catalog

九个 Art 的**源码和独立 package 构建**均已 canonical；但 R23 manifest 的正式
`sampleArtPackages` 只有六个 sample ZIP。三个 Surface prototype 由
`build-surface-prototypes.ps1` 独立打包并在 release verifier/runtime acceptance 中
作为 smoke/fixture 使用，没有作为 R23 的正式 catalog artifacts 发布。

因此可以宣称“九个源码包已重构并验证”，不能宣称“R23 正式发行包分发九个 Art”。
若开发目标要求九个都随 Loom release 分发，还需要扩展 release manifest、catalog、
checksums、SBOM/provenance 和 verifier。

#### F. 当前审查后源码尚无对应正式 release

R23 `manifest.json` 记录：

- `gitHead = 98115667cb256cb242b10e32c29a11c853ebe929`
- `gitDirty = true`

R14 也在源码提交整理前构建，并且 R14/R23 都不包含 2026-08-14 审查新增的资源归属
释放、transport negotiation、strict formal output、multi-output 与 package/canvas
收紧。已有哈希和 native acceptance 只能证明“这些旧 bytes 在该工作区运行通过”，
不能证明当前源码已经发布。需要在 Hook/Loom 提交后构建新的 release ID、重新
verify，并重跑精确 hash 的 native acceptance。

#### G. Native acceptance 有显式测试态输入隔离

前两次 R13/R23 诊断在共享桌面接收到真实 global Delete input，导致被测 Surface
在 restart 前按正常产品语义 dispose。R14 在显式 `HOOK_NATIVE_ACCEPTANCE=1` 时只
忽略 native global Delete event，正常产品模式和 protocol lifecycle 不变。

最终 600 秒证据是在此测试态隔离下通过。独立审查应核对：

- gate 是否只能由显式 acceptance environment 激活；
- 是否只影响 native global Delete，而不屏蔽 protocol dispose；
- release 默认启动是否不带该变量；
- 团队是否接受“共享桌面输入隔离后的稳定性验收”作为正式 evidence。

#### H. 两套当前 canvas shape 必须被证明是双 producer，而非旧 alias

`HookCanvasDocument::from_serialized_root` 同时读取 persisted `stickers/links` 和
workflow wire `nodes/edges`。实现方将它们定义为两个当前 producer，并已删除第三套
legacy shape。独立审查应追踪两个 writer，确认没有任何旧 reader 借此静默迁移旧
session；如果实际只有一个 producer，另一支就应继续删除。

#### I. Bundled/local unsigned package 的信任模型仍需产品化决策

严格用户安装策略会拒绝 external unsigned package，但 checksum-verified bundled
catalog 和 local authored draft 有显式路径可以保持 unsigned。当前 R14/R23 native
evidence 中安装的 official process Framework 与 dashboard fixture 也显示
`trustStatus=unsigned`。这不影响 wire canonicality，却意味着“正式官方 package 全部
签名并 trusted”没有实现。

### 4.3 刻意不实现，且不应被复核者补回的内容

以下内容是明确的 breaking cleanup，不是遗漏：

- 旧 package layout/session/config/app-data 的自动迁移；
- ArtLoom/AHRP/ArtHook 旧 route 和旧 event aliases；
- snake_case/camelCase 双字段接收；
- publisher-less packaged Framework/Art；
- Hook-local Art fallback executor；
- flat latest Art Store copy；
- missing type、artId-only 或 stale node 的 Art 推断；
- 为已删除 per-Art installer 保留 wrapper；
- 为旧 workflow binding 顺序做运行时 normalization。

如果独立审查发现测试或脚本仍要求这些行为，正确方向应是修复测试/fixture 或删除
残余，而不是添加兼容 shim。

---

## 5. 建议独立 AI 的复核方法

### 5.1 先核对 Git 边界

```powershell
git -C .\Hook show --stat --oneline 0d952ec391012e5ba5292932b06a2341f5da5eab
git -C .\Loom rev-parse HEAD
git ls-files -s Hook Loom
git status --short
```

确认父提交只更新 Hook/Loom gitlink，没有夹带其他子项目或根目录文件。

### 5.2 审查 canonical protocol，而不是只 grep 名称

重点对照：

- `Loom/crates/loom_protocol/src/hook.rs`
- `Loom/protocol/schemas/hook-message.v1.schema.json`
- `Hook/src/services/protocol.ts`
- `Hook/src-tauri/src/loom_hook.rs`
- `Loom/apps/daemon/src/lib.rs`

检查 method/event、request/response identity、camelCase wire fields、typed formal value、
generation/cancellation/revision 和 Surface event namespace 是否逐项一致。

### 5.3 审查“无 Art-ID 分支”

在 Loom/Hook source 中搜索九个 Art ID。允许出现的位置应主要是：

- package manifest/runtime；
- package contracts/smoke fixtures；
- 文档；
- acceptance 指定 dashboard fixture。

若在通用 host execution、Hook graph resolution 或 UI dispatch 中发现按 Art ID 的
业务 switch，应视为重构未完成。

### 5.4 审查 package/layout fail-closed

至少验证：

- manifest 缺 publisher 被拒绝；
- flat framework/art directory 不会被 runtime/recovery 解析；
- two publishers/same local ID 隔离；
- workflow child bare ID 被拒绝；
- Art Store 不暴露 flat latest；
- Hook old app-data identifiers 不被自动探测；
- old node/session field aliases 不被解释。

### 5.5 复核 source tests

推荐最小命令集合：

```powershell
# Loom
cargo fmt --all -- --check
cargo test -p loom_protocol
cargo test -p loom_tool_registry --lib
cargo test -p loom_workflow_runtime
cargo test -p loom_workflow_store
cargo test -p loom-daemon --lib -- --test-threads=1

# Loom desktop
Set-Location .\apps\desktop
npm test
npm run typecheck
npm run build

# Hook
Set-Location ..\..\..\Hook
cargo fmt --manifest-path .\src-tauri\Cargo.toml --all -- --check
cargo test --manifest-path .\src-tauri\Cargo.toml --lib
npm test
npm run typecheck
npm run build
```

Hook frontend 全量 suite 在本工作区约需 7 到 10 分钟，不应使用 120 秒 harness
timeout 将其误报为测试失败。

### 5.6 复核 release 与 runtime evidence

```powershell
Get-FileHash -Algorithm SHA256 `
  .\release\Hook\20260813-loom-hook-v1-surface-wire-r14\hook.exe

Get-FileHash -Algorithm SHA256 `
  .\release\Loom\20260813-loom-hook-v1-surface-wire-r23\Loom.exe

Get-FileHash -Algorithm SHA256 `
  .\release\Loom\20260813-loom-hook-v1-surface-wire-r23\runtime\loom-daemon.exe

powershell.exe -NoProfile -ExecutionPolicy Bypass `
  -File .\Loom\scripts\verify-release.ps1 `
  -PackageDir .\release\Loom\20260813-loom-hook-v1-surface-wire-r23 `
  -RunSmoke
```

然后直接读取 acceptance outer/inner `summary.json`，核对：

- exact executable paths 和 hashes；
- native startup 与 single-instance；
- publisher-qualified dashboard；
- pairing/approval；
- Surface action/resource/formal result；
- 600 秒 samples 和内存阈值；
- same-instance restart recovery；
- normal exit、forced cleanup、process/listener teardown。

重新运行 native acceptance 会控制桌面应用与本机进程，执行前必须先审计现有 Hook/
Loom 进程和 listeners，不能在未知用户运行态下直接启动或终止进程。

---

## 6. 审查结果应回答的问题

独立审查最终不应只回答“测试通过/不通过”，而应分别回答：

1. Hook 是否已经不再执行或加载 Art package code？
2. Loom 是否是所有 packaged Art 执行、取消、资源和 durable state 的唯一所有者？
3. `loom.hook.v1` / `loom.surface.v1` 是否是唯一跨应用 production wire？
4. 九个 Art 源码包是否都已迁移，且 host 无 Art-ID-specific dispatch？
5. packaged Framework/Art 是否在 install、runtime、rollback、recovery 全程使用
   publisher-qualified identity 和 canonical layout？
6. preview/formal、generation/cancellation、revision/stale checks 是否真正闭环？
7. 已删除的 Legacy 行为是否仍通过隐藏 alias、默认值或 test fixture 回流？
8. 第 4.2 节的例外哪些是合理产品边界，哪些应继续清零？
9. 三个 Surface prototype 是否应进入正式 release catalog？
10. 是否需要在最终 commit 上重建 clean-provenance release 并重跑 600 秒验收？

只有这些问题分别给出源码和运行证据后，才能把“调用框架已成功”“开发清单已完成”
与“所有潜在兼容/安全/发行目标均已完成”这三个不同结论准确地区分开。
