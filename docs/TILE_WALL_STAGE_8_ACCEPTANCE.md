# 瓷砖墙阶段 8：管理放置、原生输入与资源观测

2026-09-13。本机管理界面的内容选择、放置、裁剪、层级、来源复用和关闭管理窗口后的
持续显示与输入已验收。600 秒长测尚未通过，普通 Hook GUI 源联合验收仍待执行。
完整范围继续按 [实施计划](TILE_WALL_IMPLEMENTATION_PLAN.md) 推进。

## 候选与验收范围

本阶段没有修改产品源码或重建二进制，沿用阶段 7 最终候选：

- Hook：`Neuro/release/Hook/v0.2.30.8-identification-resync/hook.exe`。
- Loom：`Neuro/release/Loom/20260912-tile-wall-r8-admission`。

已重新计算 Hook、Loom GUI、daemon 的 SHA-256，均与
[阶段 7 产物摘要](TILE_WALL_STAGE_7_ACCEPTANCE.md#当前候选与门禁) 一致。
此前官方 Loom 包校验覆盖 50 个文件和 7 组 smoke；本阶段没有改变依赖或发布脚本，
没有重新分配内部版本，也没有暂存、提交或公开发布。

机器为 Windows 11 build 26200、Intel i5-12400F、6 核 / 12 逻辑处理器、约 31.8 GiB
可见内存。实际输出为 3840 × 2160，WebView DPR 为 1.5。Live 来源使用真实 WinForms
客户区 WGC 和生产回源代码，由 Rust 原生测试进程承载。该测试进程摘要为
`a25fcf1ad7837d6cb85948c1b1b1c6f29212fad0fbb8d242ca2a7c536dfa6fc1`。
这些记录覆盖一块物理输出，不构成双机、接缝同步或任意低能力终端验收。

## 管理界面与来源生命周期

最终完整运行目录为 `Hook/artifacts/tile-management-r8-d-wheel-evidence`。
`summary.json` 为 `passed: true`，6 个产品进程路径匹配候选；`cleanup.json`
为 `passed: true`、`remaining: []`。管理阶段所有修改均通过发布版 Loom GUI，
HTTP 只观察结果。后续原生输入阶段另行使用 API 创建其测试布局。

管理页从空目录创建 300 × 100 的逻辑墙面，选择已有 Live、Art Surface，并导入
红蓝 PNG。真实输出的图片像素与 Art DOM 均经过断言。继续复用同一图片，裁剪到
右半区，分别设置层级 10 和 -1，验证实际遮挡结果；同一 Live 增加第二个位置时，
会话 ID 集合保持不变，原来源持续送帧。

删除重复图片、Live 位置及 Art 显示后，普通来源附件、正式结果和 Live 会话仍保留。
重新加入 Art 继续引用原实例。布局版本从 2 推进到 7；最终管理截图明确显示
`1 / 1 个输出已确认`。关闭本次拥有的 Loom 管理进程后，混合画面继续更新，源按钮
点击产生一次真实效果，图片像素与布局版本保持正确。

已人工查看同目录对应的三张截图，位置在
`Hook/output/playwright/tile-management-r8-d-wheel-evidence`：
`management-three-sources.png`、`management-layout-editor.png`、`management-headless.png`。

早期记录保留如下：

- A：隐式 label 的 exact 匹配失败；仅修正探针定位器，清理为空。
- B：管理断言通过，后续滚轮等待超时；当时没有完整 DOM/滚轮轨迹，原因未确认。
- C：增加有界输入观测后完整通过，滚轮从 DOM、成功 IPC 到源滑块 40 → 37。
- D：完整通过，额外执行 16 次交替方向滚轮；每次均有 DOM、成功 IPC、恰好一次原生
  `MouseWheel` 和方向一致的数值变化。最终滑块回到 40，按键/按钮均释放。

B 的滚轮失败没有因 C/D 通过而被标记为已修复，也不据后续长测的现象倒推其原因。

D 同时验证点击、拖动、键盘、裁剪旋转、布局和失焦释放、物理识别、冻结、黑场、
恢复、输出重启及断源清屏。显示控制期间源帧从 123 增至 170，冻结 canvas 摘要为
`0007ba072fd60a91e23c2d3ba83a17485f60e5d38a3d0fb99365828dc8d32df6`。
27 次点击到可观测源效果的中位数为 65.4219 ms、P95 为 119.5549 ms、最大值为
138.4487 ms；包含 50 ms 轮询，不代表物理显示反馈延迟。

## 长测失败与观测边界

新增 `-SoakSeconds` 支持 120 至 600 秒，自动启用管理步骤。混合布局保存并关闭
Loom 管理窗口后，每约 2 秒进行一次源点击，检查恰好一次效果、持续送帧、动态像素、
应用版本和输入释放。输入失败仍使整次验收失败，不重放失败点击或放宽原有超时。

- `Hook/artifacts/tile-live-soak-r8-a-600s`：完成 7 次点击后，第 8 次等待超时；
  源仍送帧。缺少该次输入轨迹，原因未确认。整体失败，清理为空。
- `Hook/artifacts/tile-live-soak-r8-b-trace`：完成 97 次点击后，第 98 次等待超时。
  已保留 `soak-input-trace.json`、`soak-failure-state.json`、失败截图及资源采样；
  整体失败，清理为空。

B 的轨迹显示，探针之外的键盘事件和持续指针移动进入同一共享输出。丢失点击发生时，
探针在约 194412 ms 向 canvas 的 CSS `(122.98, 398.51)` 发送按下/释放；194432 ms
另一个移动进入 Art 的 INPUT 区域 `(1072, 314)`。随后获取控制权成功，但立即释放，
没有发送该点击的回源 input。生产 `tileInputEvents.ts` 的 `pointerleave` 取消与
`tileInputController.ts` 的 generation 检查解释了这条链路；未修改此安全行为。
保留的末段 256 条 IPC 记录没有 failed 项，端点仍在线且应用版本为 7。

因此下一次完整物理输出长测需要约 12 分钟不操作共享桌面的键鼠。没有屏蔽用户输入、
终止用户程序，或用直接调用回源 API 替代失败的输出交互。

资源采样修复了管理根进程退出后遗漏仍存活子进程的问题：保留已观察到的 PID 与
精确创建时间，仅当当前 CIM 身份一致时继续采样和递归归属，拒绝复用 PID。先运行
保留孤儿与 PID 复用反例得到红灯，再修改 owner helper，原有退出竞态、时间精度、
CPU 缺值和句柄释放测试一并通过。

资源门禁要求 daemon、源进程、输出进程分别具有匹配路径且跨基线/尾段稳定的身份，
多个 WebView 不能代替缺失根进程。4 项 CLI 回归覆盖正常观测、缺少任一根进程、
错误路径/PID 复用、持续内存/句柄增长。原实现的两个反例失败，修复后全部通过。
观测最多 720 份，输入/DOM 轨迹各最多 256 项，待收 IPC 最多 64 项。

B 失败后单独分析其 214.692 秒、123 份资源观测。16 个稳定产品进程通过预先定义的
局部门禁：基线为第 60 至 90 秒，尾段为最后 30 秒；每进程私有内存增长上限为
64 MiB 与基线 25% 的较大者，句柄上限为 64；总量上限分别为 128 MiB / 25% 和
128 个句柄。总私有内存均值为 588.77 → 599.59 MiB，增长 10.82 MiB；总句柄均值为
7199.94 → 7102.39。CPU 按 12 个逻辑处理器归一化，排除测试运行器和用户进程。

| 进程 | 私有内存基线 / 尾段 MiB | 平均整机 CPU 占比 |
| --- | ---: | ---: |
| daemon | 10.93 / 11.25 | 0.25% |
| 输出 Hook 主进程 | 9.55 / 9.38 | 0.82% |
| 原生源测试进程 | 57.57 / 59.27 | 4.14% |

每个 WebView 的数据也保存在 `resource-result.json`。这个文件的 `passed: true`
只表示上述短窗口资源门禁通过，不能覆盖整体失败或补足 600 秒。完整长测、显示反馈
延迟、NIC 流量和多终端资源隔离保持未完成；raw BGRA 负载估算也不等同于网络实测。

## 普通 Hook GUI 来源

独立基线 `Hook/artifacts/tile-gui-capture-r8-a-baseline` 已通过：真实 Ctrl+2 与原生
指针选区、HWND/DPI、角点拖动、源按钮、源窗口移动后区域/输入保留、原生滑块拖动、
Tab 参数、Ctrl+E 编辑器及 Shift+1 菜单路线。6401.34 ms 内取得 101 个编码帧，
Hook CPU 增量为 1.1875 秒。该基线关闭 Loom 集成，`artCreated: false`。

`Invoke-TileWallNativeInputProbe.ps1 -GuiSource` 已实现并通过类型与语法检查，使用
真实 Ctrl+2、原生选区、普通 Hook GUI 捕获及生产 `publish_live_capture_to_loom`
命令。Surface 授权绑定由隔离管理 API 准备；当前 `LiveRelayPanel.tsx` 没有挂载入口，
故此探针不声称覆盖用户可见的发布按钮。停止时等待生产 relay/capture 停止回执和
输入释放，再请求测试门控的正常退出。

联合 GUI 运行尚未启动成功：预检发现用户从 Explorer 启动的普通 Hook，按单例边界
退出。已请用户正常关闭 Hook 后再运行。用户的 Hook 和旧 Loom/daemon 均保留。

## 源码门禁与未完成项

本阶段只改变 Hook 探针、夹具与 Loom 验收文档。Hook `lint`、`typecheck:test`、
PowerShell parser、Node TypeScript syntax、进程归属契约与 4 项资源门禁回归已通过。
Hook 行数 checker 契约 17 项通过，ratchet 扫描 1197 个文件，无超过 500 有效行的文件。
Loom checker 契约 15 项通过，strict 扫描 1027 个文件，保留 12 项有效既有软例外，
没有新增违规。两仓 `git diff --check` 均通过。
日志保存在 Hook `.tmp/tile-wall-stage8-*`、`tile-wall-owned-orphans-*` 和
`tile-wall-soak-resource-*`。修改均保持 UTF-8 无 BOM，没有新增行数例外。

本阶段有效行数：管理探针 162、原生输入 206、滚轮观测 37、输入轨迹 66、GUI 捕获
63、GUI 源监控 76、源准备 55、会话 helper 62、进程归属 helper 48、归属契约 54、
资源门禁 65、资源门禁测试 63。C# 原生夹具以 checker 的 C-like lexer 人工核对，
由 246 增至 257 行；仓库扩展名策略尚未自动纳入 C#，此处没有将其作为豁免。
原生编排脚本当前为 247 行、长测脚本为 98 行，均低于 500 行。

两仓分别复核 Git 状态：Hook 有 16 个 tracked 修改和 555 个 untracked 文件，Loom
有 38 个 tracked 修改和 1658 个 untracked 文件；两仓均无暂存项。计数包含此前源码、
截图和本地证据，未将这些内容批量暂存或清理。长测 A/B 的 `remaining` 均为空；用户原有
Hook 主进程/子进程及旧 Loom/daemon 的路径和创建时间保持原身份。

待完成项仍包括完整 600 秒运行、普通 GUI 源联合路径、用户可见发布入口、完整源恢复、
场景调度/媒体时钟、低能力媒体通路、跨瓷砖手势、多人/多终端与双机物理验收。
第二台 Windows 电脑的局域网地址及已授权远程入口尚未提供，不能以单机模拟补足。
