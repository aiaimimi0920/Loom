# 瓷砖屏幕墙第一阶段候选与验收记录

日期：2026-09-10。状态：本阶段已构建并验证；完整可交互屏幕墙仍在实施中。

当前可以配置墙面、绑定 Windows 物理输出、在独立 Hook 终端显示空墙测试图，
并在关闭 Hook 管理窗口、重启 Loom daemon 后保持或恢复这条控制链路。
图片、Hook Live、Art 的真实跨屏内容传输及点击 / 键盘回源尚未接通。
这些功能继续按 [实施清单](TILE_WALL_IMPLEMENTATION_PLAN.md) 推进，不能以本次候选替代最终交付。

## 候选版本

| 项目 | 目录 | 验证结果 |
| --- | --- | --- |
| Loom | `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260910-tile-wall-r1-package` | 官方完整包校验及 7 组运行 smoke 通过 |
| Hook | `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Hook\v0.2.30.1` | provenance 摘要匹配、自检、物理枚举、独立终端联合运行通过 |

两个版本均来自当前未提交工作区，`gitDirty = true`，用于内部验证。
Hook 公开产品版本仍为 `0.2.30`，内部版本为 `v0.2.30.1`，没有重复增加迭代号。
没有创建公开发布、标签或提交，也没有覆盖既有版本。

Loom 初次构建的 `20260910-tile-wall-r1` 是未生成 ZIP 的准备目录。
随后通过官方 `build-release.ps1 -PreparedPayloadRoot` 生成上表完整包，
保留相同 Loom / daemon 可执行文件并生成 desktop、CLI、Plugin SDK 归档。
运行校验以 `20260910-tile-wall-r1-package` 为准。

可执行文件 SHA-256：

| 文件 | SHA-256 |
| --- | --- |
| `Loom.exe` | `a564f28de7ccf3276ab9df1d6546fdfbc3330f97b2601a991cd08a47cd91d6a7` |
| `runtime\loom-daemon.exe` | `0b9623d1c5e40585f97630d84371d18e62b5b6eb8450f81f42674480b9530786` |
| `hook.exe` | `fcadabd30dc5b6240f3f50f773b3a796f36f6e2ce43fab60cfdf2f760595d844` |

## 使用入口

启动候选 Loom，在既有“设备管理”中选择“屏幕墙”。该页可以创建墙面，
添加终端、排列与旋转瓷砖、数值编辑几何、预览和保存布局。
界面保存使用打开草稿时的 catalog revision；发现其他修改时会保留草稿并阻止过期保存。

Hook 提供三个入口：

```powershell
.\hook.exe --tile
.\hook.exe --tile-outputs
.\hook.exe --tile-output monitor-<64 位十六进制输出标识>
```

`--tile` 管理窗口显示可绑定输出并发起 Loom 配对，首次连接需在 Loom 设备页批准。
连接配置沿用 `LOOM_MANIFEST_PATH` 等现有能力发现机制。
建议先完成管理窗口配对，再启动输出；当前尚未完成多输出首次同时创建身份的验证。
输出进程注册后，在 Loom 墙面编辑器中分配该输出，保存没有内容位置的墙面即可显示测试图。

输出窗口按 Escape 退出；关闭 Hook 管理窗口保留已启动输出。
终端运行不要求 Loom 桌面显示在瓷砖上，本次联合验收直接启动了无头 daemon。
跨设备连接仍遵守现有 HTTPS 和设备授权要求。详细边界见
[Hook 终端说明](../../Hook/docs/TILE_TERMINAL.md)。

## 本阶段实现

Loom 墙面管理沿用设备管理入口和现有 UI token。布局草稿、轮询状态、保存错误分别管理，
不会用在线状态轮询覆盖编辑中的几何。墙面协议在 TypeScript 接收边界拒绝未知字段和非法枚举。
内容选择已读取现有 Live 会话与 Art Surface 目录，图片引用仅取正式输出；
既有图片库的完整选择、真实媒体显示和输入还需要后续接入。

Hook 增加独立 Tauri runtime，避开普通截图入口的全局输入钩子、托盘和来源监听器。
物理输出标识来自 Windows 显示设备接口的 SHA-256，显示枚举在 scoped per-monitor DPI
上下文中执行。每块输出单独持有进程互斥锁，前端使用一个串行 presenter owner 管理注册、
呈现租约、布局应用、心跳和退出。Canvas 总像素预算为 16777216。

空墙测试图使用公开的墙面几何映射。对于包含真实内容位置的布局，当前终端清除测试图、
报告 `tile_content_transport_pending`，并发送 null 应用确认。
输入能力列表为空，没有声明已经支持 Live、Art、H.264、鼠标、键盘或触摸。

## 实际发布二进制的联合验收

测试使用本机一块 3840 × 2160 显示输出、新建的隔离目录和新配对设备。
没有读取或更改用户既有设备身份；管理员 token 仅在测试进程内用于管理请求，没有输出到日志。
HTTP 只用于本机 loopback，设备请求仍由 Hook 原生层完成签名认证。

| 步骤 | 观察结果 |
| --- | --- |
| 新设备配对 | Hook 管理窗口通过真实 pending 请求和管理员批准连接隔离 daemon |
| 输出注册 | 输出进程独立注册物理输出，端点 online，未分配墙面时 appliedRevision 为 null |
| 空墙 revision 2 | 3840 × 2160 Canvas 显示渐变 / 网格 / 对角线，终端确认 2 |
| DPI | CSS viewport 2560 × 1440、DPR 1.5，对应 3840 × 2160 物理像素 |
| 管理窗口结束 | 输出进程仍存活，daemon 仍收到应用确认 |
| 90 度旋转 revision 3 | 画面像素采样随旋转改变，终端确认 3，无管理按钮混入输出 |
| 未支持内容 revision 4 | 旧测试图像素清空，出现结构化待实现状态，应用确认保持 null |
| 恢复空墙 revision 5 | 终端恢复测试图并确认 5 |
| daemon 停止 / 重启 | 停止后出现 `wall_transport_failed`；复用持久化目录重启后重新认证、连接并确认 5 |
| 重复输出进程 | 第二进程 exit 1，原输出进程保持存活 |
| Escape 退出 | 输出进程退出，端点立即离线、确认清空，墙面和端点配置保留 |

本次成功运行的实际进程：Hook 管理 PID 47128、输出 PID 45808；
Loom daemon PID 32216，重启后 PID 47300。可执行文件均位于上表候选目录。
测试结束后已按 PID 与可执行路径核对并清理本次拥有的进程。

本机证据位于 `Hook/artifacts/tile-release-smoke-20260910-r3`：
`configured-state.json`、`recovered-state.json`、`final-state.json` 和不含凭证的 `runtime.json`。
目录中的 manifest / 身份文件是私有运行数据，位于 Git 忽略范围，不应打包或分享。
最初的 smoke 脚本只遍历 `devices` 而漏读独立的 `pending` 集合，导致配对等待误报超时；
修正测试脚本后以全新隔离目录执行上述成功运行，没有为通过测试修改产品认证逻辑。

可查看以下截图：

- [Hook 管理窗口](../../Hook/output/playwright/tile-release-control.png)
- [实际输出测试图](../../Hook/output/playwright/tile-release-output.png)
- [90 度旋转](../../Hook/output/playwright/tile-release-rotation.png)
- [未支持内容的清屏状态](../../Hook/output/playwright/tile-release-pending.png)

## 检查结果与源码边界

Loom：协议 70 项和 daemon 库 333 项测试通过，1 项既有跨进程入口按设计 ignored；
桌面新墙面服务 5 项测试、类型检查、前端构建通过。strict 行数检查扫描 985 个文件，
12 项既有例外有效，零违规；checker 自测 15 项通过。相关 Rust formatter / 编译通过。
发布包官方 `verify-release.ps1 -RunSmoke` 校验 47 个文件，7 组 smoke 全部 passed，
日志为 `Loom/target/tile-wall-release-verify.log`。

Hook：墙面几何 / API 17 项、presenter 5 项测试通过；原生墙面客户端 4 项、设备认证 13 项，
以及终端参数、输出身份和 DPI 上下文恢复测试通过。`lint`、前端与测试类型检查、
Rust formatter / 编译、前端构建和版本一致性检查通过。
ratchet 行数检查扫描 1124 个文件，无超过 500 有效行的受检文件，零违规。

两仓依赖安全契约及真实 OSV 扫描通过，沿用已有受审查例外，没有新增豁免。
两仓 `git diff --check` 分别通过。本次功能源码均低于 500 有效行，没有新增行数例外。
最终 Git 状态：Loom 有 13 个已跟踪文件修改、1600 个未跟踪文件；
Hook 有 7 个已跟踪文件修改、72 个未跟踪文件。未跟踪统计包含任务开始前的既有内容。
两个仓库的暂存区均为空，未进行批量暂存或清理。
相关修改文本及 4 份墙面 / 终端文档的 UTF-8 无 BOM 检查通过。
最终复核候选可执行路径和本次隔离 WebView 目录，残留进程数为 0。

## 继续实施的边界

仍需完成：完整来源选择、图片有界资源读取、Live 会话复用与媒体呈现、Art Surface 呈现、
输入授权 / 坐标转换 / 焦点与按键释放、多终端恢复和定时显示。
网络或首次配对请求等待期间的显示热插拔响应，以及多输出首次身份创建的并发行为仍需硬化。

当前只核验了一块物理输出，没有两台独立电脑的真实局域网拼接证据。
跨屏接缝、图片 / Live / Art 联合显示、真实源程序交互、多人竞争、媒体同步和延迟测量均未验收。
这部分工作保留在原清单中，完整任务未标记完成。
