# 瓷砖墙阶段 6：冻结、黑场与恢复显示

2026-09-12。显示控制的本机发布版验收已通过，包括实际 Loom 管理界面、
Hook 物理输出、真实 Art 来源、图片和重启恢复。使用一块 3840 × 2160 输出，
完整瓷砖墙计划仍在实施中，见 [实施计划](TILE_WALL_IMPLEMENTATION_PLAN.md)。

## 当前候选

- Loom：`Neuro/release/Loom/20260912-tile-wall-r7-display/Loom.exe`。
- 无头服务：同目录 `runtime/loom-daemon.exe`。
- Hook：`Neuro/release/Hook/v0.2.30.7-display-isolated/hook.exe`。
- Hook 内部版本为 `v0.2.30.7`，公开 SemVer 保持 `0.2.30`，`uiAccess: false`。
  内部版本只分配一次，修复后的重建复用该身份。
- 两仓候选来自 dirty 工作区，没有创建提交或公开发布。早期 Hook
  `v0.2.30.7-display`、阶段 5 候选及失败证据全部保留。

最终文件摘要经重新读取核对；daemon/Hook 与原生 D 记录一致，Hook 也与
provenance 一致：

| 文件 | 字节数 | SHA-256 |
| --- | ---: | --- |
| `Loom.exe` | 10323968 | `48d6bf0c5ad80ae269877e95e480c43ca73cff8865b225dc4f63637e1eb9a75f` |
| `runtime/loom-daemon.exe` | 25778176 | `85dde97435f4aa0ae95b4af9fe49ffe7b7f5123f8e8419f663e641848d87aa4b` |
| `hook.exe` | 8456704 | `7d3b596d48c769550832a6c7e8de277dfea1fa1900a8ecbdf232cb554aeeda66` |

## 已实现的合同

管理员通过 `PUT /v1/walls/presentation` 切换 `running`、`frozen`、`black`。
操作使用目录 CAS 与原子持久化；设备凭证不能管理模式。冻结和黑场保留几何版本，
在权威 store 立即拒绝新的 Live/Art 输入及确认，并在响应前释放 Live 控制权和
按键/按钮，不等待终端回执。已接受的 Art 执行继续，所属请求仍可取消。

控制记录为 `presentations: [{wallId, revision, mode}]`；运行状态不存储控制项。
终端回执为 `presentation: {revision, outcome}`，其中 `outcome` 是 `applied`
或 `frame_unavailable`。完整冻结帧必须对应当前 `appliedRevision`；黑场或缺帧
必须报告 null。回执与持久化模式分离，租约、权限、布局或连接失效后不能沿用。
完整协议及严格解析的升级边界见 [显示控制 API](../protocol/WALL_PRESENTATION_API.md)。

Hook 只保留同一端点、租约、布局版本和尺寸的完整画面。冻结停止 Live 解码与
Art 轮询，保留已准入快照和图片资源，拒绝迟到结果；黑场清除内容且不加载来源。
输出重启、断线、失去授权、改变几何或黑场后再冻结时，没有旧帧就明确报告缺帧。
恢复显示推进布局版本，载入当前来源；旧连接或队列中的输入不能重放。

Loom 管理页区分请求模式和真实输出回执。控制操作保留未保存的几何草稿与原始
CAS 版本；只有仍与已保存状态一致的干净草稿随响应刷新。管理窗口关闭后控制、
呈现和来源由 daemon 与终端继续维护。

## 原生失败与修复证据

早期 A、B、C 运行分别保存在 `Hook/artifacts/tile-display-r7-a-native`、
`tile-display-r7-b-native` 和 `tile-display-r7-c-timing`。三次均在冻结之前，
dashboard 刷新动作返回 `wall_surface_event_expired`。

C 的请求时序记录显示所有原生 IPC 响应不超过 354 ms；表单事件约在 5618 ms
和 9025 ms 提交，dashboard 点击约在 9025 ms 到达。表单已有两个成功回执，
dashboard 没有回执。原因是所有 Art 共用一个等待执行完成的输入队列；表单的
input/change 校验占用队列，使无关点击超过原有 1.5 秒期限。

`tileSurfaceController.ts` 改为最多 4 个按 Art 实例等待的工作循环，同实例
顺序及编辑依赖保留，总队列仍限 32 项。原生事件提交仍串行，为轮询、图片、
确认和取消保留并发容量。实际发送前重查代际、授权和原始期限；失败只丢弃同实例
依赖队列，冻结、布局变化和失焦仍撤销全部未发送操作。独立动作期限没有放宽。

`TileSurfaceIsolation.test.ts` 的前两项先在旧实现失败，再验证独立点击不会被
其他实例的执行或失败阻断。第三项验证原生提交串行，以及冻结丢弃尚未发送的工作。
红灯记录为 `Hook/.tmp/tile-wall-r7-isolation-red.log`；6 文件 30 项的通过记录为
`tile-wall-r7-isolation-tests.log`，新增第三项后的独立重跑为
`tile-wall-r7-isolation-green.log`，3 项通过。

## 最终原生 D 验收

从 Hook 仓库运行：

```powershell
rtk powershell -NoProfile -ExecutionPolicy Bypass `
  -File scripts/tests/Invoke-TileWallArtProbe.ps1 `
  -Scenario presentation `
  -HookExe ..\release\Hook\v0.2.30.7-display-isolated\hook.exe `
  -LoomPackageDir ..\release\Loom\20260912-tile-wall-r7-display `
  -PrototypeDir ..\Loom\target\tile-wall-art-prototypes `
  -EvidenceRoot artifacts\tile-display-r7-d-isolated
```

命令退出码为 0；证据目录中的 `summary.json` 为 `passed: true`。使用真实配对、
独立持久化目录、安装的 PowerShell Art 运行时和内容寻址图片，Playwright 驱动
实际 WebView2。Loom 管理页通过原生 Tauri 传输操作实际 daemon，没有替换成 HTTP fixture。

- `management-result.json`：冻结、黑场、恢复按钮生效；输出回执及计数真实；
  脏草稿和原始 CAS 保留，干净草稿刷新，黑场转冻结显示缺帧。
- `presentation-result.json`：来源继续更新时冻结截图 SHA-256 不变，Art 快照
  版本、值和资源 URL 不变。黑场三个采样点均为 `[0, 0, 0, 255]`；黑场后冻结
  不重用旧画面。恢复取得最新来源并接受输入。
- 同一阶段追加 3 次冻结/恢复循环，临时视图不重复；几何变化使保留帧失效；
  关闭管理窗口后操作继续；输出强制退出不删除普通来源。
- `recovery-result.json`：重启输出后不会重放冻结帧，恢复后输入可用，普通
  来源回执历史保留。
- `disconnect-result.json`：daemon 断开后冻结内容清空。
- `daemon-recovery-result.json`：同目录重启 daemon 保留请求的冻结模式并报告
  无可用帧；恢复后重新接受输入。最终输出退出仍保留普通来源及其历史。

管理页截图位于 `Hook/output/playwright/tile-display-r7-d-isolated/loom-management-frozen.png`。
输出截图使用 `Hook/output/playwright/tile-display-tile-display-r7-d-isolated-`
前缀，分别为 `frozen.png`、`black.png`、`recovery.png`、`daemon-recovery.png`。
管理页、冻结、黑场和 daemon 恢复截图已视觉核对；恢复截图包含测试有意设置的旋转。

初始/重启 daemon、Hook 管理窗口、Loom 管理窗口、输出枚举及初始/重启输出，
共 7 个产品进程的实际可执行路径均与候选一致。12 个启动记录中，一个短命
`presentation` Node 驱动进程的 `actualExe` 为 null；其余 11 个记录均匹配，
没有把不可读取的路径记为核验成功。

`cleanup.json` 为 `passed: true`、`remaining: []`，覆盖 12 个登记的启动进程
及采样跟踪的子进程。清理后复查没有本阶段候选进程；用户已有两份 Hook 和旧版
Loom/daemon 仍存活，未停止或替换。

`process-samples.json` 有 102 次采样；presentation 阶段 74 次，首末跨度
154370 ms。采样包含管理窗口退出及进程重启，前后进程集合不同，不据此计算稳态
资源增长率或声称长期内存稳定。更长压力及端到端显示延迟仍未验收。

## 门禁与逐文件边界

- Loom daemon wall 行为测试 35 项通过、332 项过滤；覆盖权限、CAS、持久化、
  实际 HTTP 输入释放、报告组合及恢复。日志为 `Loom/.tmp/tile-wall-r6-display-backend.log`。
- Loom 显示控制服务测试最新重跑 3 项通过；桌面类型检查、Rust formatter 通过。
- Hook 显示控制、资源和输入队列聚焦测试通过；上述 6 文件 30 项及最终隔离测试
  3 项为独立命令。原生 wall client 9 项通过；生产/测试类型检查和 lint 通过。
- 原生 probe 最终 TypeScript 检查、PowerShell 解析及进程生命周期回归通过。
  记录包括 `tile-wall-r7-final-probe-types.log` 和
  `tile-wall-r7-management-process-tests.log`，均在 `Hook/.tmp`。
- Loom strict 行数门禁扫描 1018 文件，12 项既有软例外有效，0 违规。
  Hook 最终 ratchet 扫描 1181 文件，没有超过 500 有效行的文件，0 违规和警告。
- Loom 官方 `verify-release.ps1 -RunSmoke` 校验 50 文件，7 组 smoke 全部通过。
  首轮在 MCP fixture PUT 超时；保留失败日志和诊断目录，未修改产品或放宽超时。
  同一候选串行重跑通过，见 `Loom/.tmp/tile-wall-r7-release-verify-retry.log`。
  首轮与 Hook 构建并行，现有证据未证明并行构建是超时原因。
- Hook 候选构建、SHA-256/provenance、`--help`、`--self-check`、`--tile-outputs`
  均通过，自检为 `ok`。本阶段未改变依赖清单或锁文件；沿用阶段 5 的依赖安全
  契约及 OSV 证据，本阶段没有重复扫描。

主要文件的最终有效行数如下，没有新增行数例外：

| 仓库与文件 | 有效行 |
| --- | ---: |
| Hook `tileSurfaceController.ts` / `TileSurfaceIsolation.test.ts` | 235 / 108 |
| Hook `Invoke-TileWallArtProbe.ps1` / `probePresentation.ts` | 198 / 178 |
| Hook `presentationManagement.ts` / `presentationTrace.ts` / `probeSession.ts` | 78 / 48 / 58 |
| Loom `wall_store/presentation.rs` / `tests_presentation.rs` | 138 / 306 |
| Loom `tests/wall_presentation_http.rs` / `WallPresentationControls.tsx` | 71 / 35 |

审查覆盖管理员/设备权限、版本和输入边界、临时资源与来源所有权、有界队列、
冻结期间授权监视、迟到结果和进程清理。隔离修复前后的控制器为 212 / 235 行，
增长仍属于同一输入编排职责。最终两仓 `git diff --check` 通过；10 份文档的
76 个本地链接均存在，UTF-8 无 BOM 与尾随空白检查通过。两仓保留已有 dirty
内容，没有暂存或提交改动。

## 未完成范围

本阶段实物显示控制场景覆盖 Art 与图片，未加入物理动态 Live 来源。Live 暂停和
解码生命周期有单元测试，立即释放输入有真实 HTTP 测试；已有
[阶段 4](TILE_WALL_STAGE_4_ACCEPTANCE.md) 保留原生 Live 来源和输入验收范围。
这些分开取得的证据不等同于动态 Live 与显示控制的实物联合验收。

物理屏幕识别与完整管理、完整来源恢复、普通 Hook GUI 源、调度/媒体时钟、
长期资源、跨瓷砖手势和多人/多终端任务仍按实施计划推进。没有宣称物理同步、
Frame Lock/Genlock 或两台电脑验收。普通 Hook GUI 源验收仍需要已有实例正常退出的
窗口；双机验收仍需要第二台实体电脑的局域网地址及已授权远程执行方式。
