# 瓷砖屏幕墙阶段 2：静态图片呈现

日期：2026-09-10。状态：本阶段已构建并完成下述验证；完整可交互屏幕墙仍在实施中。

本阶段打通 Loom 图片导入、受授权的资源读取和 Hook 物理输出。Live 动态画面、交互 Art、
鼠标/键盘回源、双机拼接和显示时序仍按 [实施计划](TILE_WALL_IMPLEMENTATION_PLAN.md) 推进。

## 候选版本与使用

推荐使用以下两个候选。它们来自各自未提交的工作树，provenance 如实记录 dirty 状态，
没有公开发布、签名或创建 Git tag；此前的版本目录均保留。

- Loom：`C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom\20260910-tile-wall-r2\Loom.exe`
- Hook：`C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Hook\v0.2.30.2-image\hook.exe`

Hook 内部版本是 `v0.2.30.2`，本迭代只分配一次。`-image` 是重建候选目录后缀。
最初的 `release\Hook\v0.2.30.2` 在原生 WebView 验证中触发 CSP 错误，保留供核查，
不要使用该目录验收图片。修复后的候选在 `v0.2.30.2-image`。

运行 Loom 后，使用 `hook.exe --tile` 完成设备配对并选择屏幕。进入 Loom 的
“设备管理 → 屏幕墙”，创建墙面、添加已注册输出，然后导入图片、点击“放置到墙面”并保存。
已有 Art 正式图片输出也可作为来源；关闭并重新打开编辑器后，可继续选择已保存墙面中的图片引用。
未放置或未保存的导入保留临时资源租约，之后按正常 GC 清理；当前没有独立持久图片库。

Loom 可以只运行包内 `runtime\loom-daemon.exe`。关闭 Hook 管理窗口保留独立输出进程；
输出窗口按 Escape 退出。远端连接沿用现有 HTTPS 与配对要求。

| 文件 | SHA-256 |
| --- | --- |
| Loom `Loom.exe` | `b5354b2c0d5b2722756ef536a4a7e49c25046d9b37744fc277b643628b3f4736` |
| Loom `runtime\loom-daemon.exe` | `4d33e6d9421b1d7b715bc8f7e5505b22bd449d7fe29a9535c67e2820368001d9` |
| Hook `v0.2.30.2-image\hook.exe` | `32550103fd0ed7f026a1f764b7f8a3f10a57ca80e4b767d80714017b2cbd8371` |

Loom 完整包通过官方 `verify-release.ps1 -RunSmoke`：50 个文件校验通过；standalone、
Hook canvas、error preview、framework Art store、plugin boundary、Surface prototype、
authored Art 共 7 组 smoke 通过。Hook 摘要与 `build-provenance.json` 一致，
`--self-check` 返回 `status=ok`，`--tile-outputs`、`--help` 均成功。
Hook 文件版本和公开包版本仍为 `0.2.30`，内部构建身份记录在 provenance 中。

## 实现与边界

Loom 使用既有 `/v1/surfaces/resources` 存储，不创建重复的图片仓库。
新增 `/v1/walls/images/read` 同时校验配对设备、端点归属、当前呈现租约、布局版本和
该墙面中的精确图片引用。管理员身份单独调用此 presenter 路由会被拒绝。
协议见 [WALL_CONTROL_API.md](../protocol/WALL_CONTROL_API.md)。

图片内容按 SHA-256 标识。服务端每次读取都验证长度和摘要，文件读取本身也有上限。
墙面引用加入既有资源回收保护；启动时先加载持久化墙面，再运行资源 GC。
删除 Art 或释放临时资源租约不会删除墙面仍引用的图片；删除最后一个引用后恢复正常 GC。

Hook 原生端验证摘要、格式与尺寸，再编码为有界 PNG 交给 WebView。
支持 PNG、JPEG、WebP、BMP、GIF 首帧和既有 RGBA8 图片资源。输入资源至多 16 MiB，
单图至多 16,777,216 像素、任一轴至多 16,384；decoder allocation budget 为 64 MiB。
每进程只允许一个下载/解码任务，图片网络超时为 8 秒，取消等待不会提前释放 decoder 的并发许可。

终端最多保留 16 张可见图片、合计 16,777,216 像素，重复来源共享位图。
这是保留位图预算，进程总内存还包括 Canvas、下载、解码、PNG/Base64 和 WebView 开销。
图片在后台逐张准备，控制循环继续读取状态和心跳；准备完成后由下一次有效呈现循环统一绘制。
未完成或失败时保持 `appliedRevision=null`。迟到结果关闭位图，不能异步绘回旧布局。

首次发布版测试发现 `fetch(data:)` 被现有 `connect-src` 拦截。
修复使用本地 Base64 解码和 Blob，不修改 CSP；新增回归测试禁止图片 API 调用 fetch。
重新构建后通过实际 WebView 图片验证。

## 实际运行证据

本机只有一块已核验物理屏幕。实际输出 Canvas 为 3840 × 2160，CSS viewport 为
2560 × 1440，DPR 为 1.5。使用发布版 daemon 与 Hook 完成新设备签名配对，
输出进程路径均核对到上述候选目录。输出页没有管理按钮。

使用一张 16 × 16 四象限 PNG，通过真实资源 API 上传。
其资源 ID 为 `sha256:4b4c14fda0aa80d26656ddc18ee732bffeb25dd520c0e254b628b970650f82a2`。
在画布左上、右上、左下、右下内部位置采样，结果与期望数组逐项比较通过：

| 布局 revision | 操作 | 采样颜色 / 状态 |
| --- | --- | --- |
| 2 | 完整图片 | 红、绿、蓝、黄；确认 2 |
| 3 | 仅源右半裁剪 | 绿、绿、黄、黄；确认 3 |
| 4 | 瓷砖 90 度旋转 | 绿、黄、红、蓝；确认 4 |
| 5 | 同一图片两处叠放，上层裁剪为红色区域 | 红、绿、红、黄；确认 5 |
| 6 | 改成不存在的资源 ID | `wall_request_rejected`，画布像素清零；确认 null |
| 7 | 恢复原图片 | 红、绿、红、黄；确认 7 |

之后结束 Hook 管理窗口，独立输出仍在线并维持 revision 7。停止隔离 daemon 后，
输出报告 `wall_transport_failed` 并清屏。使用同一持久化目录重启 daemon，原输出重新认证、
连接并确认 revision 7。再退出并独立重开输出进程，像素仍为红、绿、红、黄。

GC 验证中，上传后的 Surface resource lease 已通过 API 释放。仅对测试夹具的图片 metadata
把 `createdAtMs` 置为 0，使它在重启时满足回收年龄；没有修改用户资源。
重启后仍能重新下载和显示该图片，临时资源租约数为 0、墙面引用数为 2。
这验证了真实 startup GC 保留墙面引用，未用内存中残留截图代替恢复。

最后按 Escape 退出输出，端点离线、应用确认清空、墙面 revision 7 保留。
本次创建的输出、管理窗口、daemon 和浏览器预览进程均已清理。

Loom 管理页另用真实桌面前端 bundle 加明确标记的 HTTP fixture 验证：
上传期间修改草稿宽度不会丢失；上传不隐式保存布局；显式放置/保存后关闭并重开编辑器，
可复用同一图片新增第二个位置，上传次数仍为 1。
这项 UI 证据与上述真实 native/API 证据分别记录，不宣称它是双机或完整 native Loom UI 联合验收。

## 检查、审查与证据位置

- Loom：图片 HTTP / GC 测试 2 项、Surface resource 测试 15 项、桌面 Wall / 图片导入测试 8 项通过。
- Hook：原生 Wall client / 图片测试 7 项、前端 presenter / cache / 图片 API 共 11 项通过。
- 两仓直接相关 Rust 编译、前端 typecheck、Rust formatter 和 `git diff --check` 通过；Hook lint、test typecheck 通过。
- Loom strict 行数检查通过：991 个文件，12 个既有 soft exception；Hook ratchet 检查通过：1131 个文件。
- 本阶段关键文件有效行数：Loom `surface_lifecycle.rs` 445、`surface_resource_leases.rs` 261、
  `surface_instance_routes.rs` 283、图片路由 55、图片授权模块 58、图片 HTTP/GC 测试 195；
  Hook native Wall client 240、图片 decoder 136、图片 API 36、renderer 56、cache 43。抽检均为 UTF-8 无 BOM。
- 两仓依赖安全契约与真实 OSV 扫描通过，沿用已有例外；存在 unused-ignore 提示，没有新增或伪造安全豁免。
- 独立只读审查复核图片授权、GC 锁序、decoder 并发、bitmap 清理和导入草稿所有权。
  相同布局 revision 对应不可变配置的约束由 Loom CAS 保证；没有为协议违约加入重复摘要状态。
  旧上传未放置时只受临时租约保护，已在使用说明中明确。

构建/检查日志：`Loom/target/tile-wall-r2-build.log`、`tile-wall-r2-verify.log`、
`wall-image-tests.log`、`wall-image-resource-tests.log`、`tile-wall-r2-ui-smoke.log`；
Hook 日志在 `artifacts/tile-wall-r2-image-build.log`、`wall-image-native-tests.log`。

真实联调证据：`Hook/artifacts/tile-image-release-20260910-fixed/` 中的布局状态、
`*-pixels.log`、`recovery-proof.json`、`final-state.json` 和截图。
重开输出的最终像素证据使用 `reopened-pixels.log`；测试启动器的长生命周期父进程使链式
后续截图命令未立即执行，已改用单独命令完成采样，不把启动器退出状态当作画面证据。
该夹具目录含私有测试身份和 manifest，保持忽略，不复制到发布包。
UI 截图在 `Loom/output/playwright/wall-image-ui-import.png`。

最终 Git 快照分别为：Loom 19 个已跟踪修改、1609 个未跟踪文件；Hook 7 个已跟踪修改、
86 个未跟踪文件；两仓暂存区均为 0。计数包含进入任务前已有内容，不能视为本阶段新增量。
两仓 HEAD 仍为计划中的起始提交，没有提交、重置或覆盖既有工作。

## 尚未完成

本阶段完成 TW-05 的静态图片子项。Live 会话画面、Art 交互实例、点击/拖动/滚轮/键盘回源、
跨瓷砖手势、多用户操作权、场景调度、显示同步与延迟测量仍未实现或未验收。
没有宣称支持无 CPU/GPU 专用硬件，也没有两台实体电脑的局域网拼接证据。
当前控制轮询仍承担输出检查，网络/配对等待期间的热插拔响应沿用阶段 1 的限制。
后续应继续接通已有 Live 会话的授权画面与输入通路，并保留来源会话和焦点/操作租约边界。
