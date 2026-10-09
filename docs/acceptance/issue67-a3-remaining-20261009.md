# Issue #67：限定续期与停止验收之后的剩余 A3 工作

本工作包从 PR #88 的已验证候选证据继续，不重开已经通过的默认 900 秒自动续期、
正常停止清帧、此前停止竞态及专用设备禁用场景，也不重开已结单的资源增长项。
已完成证据见 [新组合验收](issue67-newpair-renewal-20261009.md)。

本文件承接未完成项；下述限定 A3-P 已恢复执行，不代表获得额外设备或破坏性测试授权。
已完成证据由 [#88](https://github.com/aiaimimi0920/Loom/pull/88) 合入 `main`，
提交为 `c959288524d2d7368cef46b99e4cefd4bc787359`；合并树与通过检查的 PR head 一致。
本工作包由独立 [Draft #89](https://github.com/aiaimimi0920/Loom/pull/89) 承接，
基线已同步上述 `main`，不重复提交 #88 的已完成证据。

## 待验矩阵

| 项目 | 尚缺的真实证据 | 启动条件与禁止替代 |
| --- | --- | --- |
| A3-V | 2/4 个独立原生观看端及慢端隔离 | 先确认真实独立设备/交互会话和隔离条件；不能用 daemon socket 数代替，也不绕过单实例锁 |
| A3-L | 真实 device loss 后的边界与恢复 | 设备丢失的触发方式、受影响范围及恢复方案须单独批准；不能用 resize、撤销或模拟异常代替 |
| A3-N | 物理断连/切换与剩余重启时序 | 先约定目标网络、管理链路保留和回退；不因已有隔离会话许可而中断物理网络或共享服务 |
| A3-P | 完整性能对照、画质配准、GPU、跨机帧龄、重复顺序及全部负载 | 先明确工作负载、基线、测量误差及可用资源；不能用单观看端已通过的静态/滚动子矩阵代替 |
| A3-D | 合成器确认或物理屏幕呈现 | 先明确测量设备、时间基准和误差；不能把 Element Timing、软件帧推进或截图称为物理呈现 |

## 执行与结单规则

- [ ] 每次只选择环境与明确授权同时满足的最小场景；没有条件的项目保持未验。
- [ ] 保留精确候选 SHA、身份与会话边界、原始失败记录、时序和清理回执。
- [ ] 测试进程及监听端口必须按所有权核验清理，不停止日常 Hook 或无关进程。
- [ ] 有实际产品缺陷时另建最小修复与聚焦回归；验收空项不是新增功能的依据。
- [ ] 已完成且可独立成立的证据先单独交付，未完成项继续以独立 PR 承接。
- [ ] 所有工作包真实满足验收条件后，才考虑关闭整个 A3 / Issue #67。

## 已完成交付的 CI 历史

#88 head `86e7a134769e29c61dde01aa7d7503abeed372ed` 的 Windows CI 首次失败于
`Test-FrameworkFixtureReadiness.ps1` 的 owned fixture 清理，记录为
`Owned fixture cleanup failed. Expected=[0] Actual=[2]`。
同项本地 8 项通过；完整 smoke 模块本地又出现不同的 cleanup budget 超时。
仅失败 Windows job 重跑一次后通过，该 PR head 的 17 项检查均完成且成功，才执行合并。
首次失败原因尚未完全确定，记录保留，不称首次全绿，不降低清理要求或绕过 CI。
本地模块的 cleanup budget 超时也不追改为通过；这些记录不回写既有双机验收结论。

合并后 `main` 的 Windows full validation job `113788370414` 已联网确认
`completed/success`；这不替代原生性能或正式 release 验收。

## 2026-10-09：限定 A3-P 恢复，raw 完成、JPEG 启动前停止

用户明确允许 PC1/PC3、隔离身份下的非破坏性对照，禁止退出日常 Hook、物理断网
或驱动重置；隔离或内存条件不满足即停止。计划为单观看端 scrolling，raw → JPEG，
每组预热 30 秒、daemon 采样 60 秒，另采 CPU/资源及 GPU process-engine 前后端点。

证据根：`linshi/issue67-a3p-resume-20261009`。原始身份、日志和私钥仅本地保留；
脱敏索引见 [本轮回执](issue67-a3p-resume-20261009.json)。候选未换包：

- Hook `.32` SHA256：`6911eb0a70b02baf19aa1320c36928c4e5d35cf3aae3852f66f227f62433f840`。
- Loom SHA256：`8132707e238e9a1e439a0d6a9543e05c58a8927ff9d7a2c3887b8616ec3df806`。
- 使用内部隔离候选，不重标为合并后 clean-source 正式 release。

### 已执行及异常

raw-1 完成真实发布/加入、三个观察器及其 helper 关闭、正常停止清帧和双机清理。
11:41:24Z 的清理回执通过；PC3 专用任务 Disabled，日常 Hook 退出次数为 0。
采样稳定媒体尺寸为 `658×407`，启动回执 `439×271` 未被改写；离线派生记录保留两者。

| 指标 | 本轮 raw 实测 | 边界 |
| --- | --- | --- |
| daemon | 119 样本，59.676 秒；发布 1211 帧，成功写入 549 次 | 约 20.293 发布/秒、9.200 写入/秒，不是显示 FPS |
| daemon 计数差 | viewer skipped 663，source sequence gaps 0，failed writes 0 | 存在明显跳帧，不能称流畅度或性能达标 |
| viewer | 240 样本，65.780 秒；182 个不同绘制帧，53 次缺绘制证明 | 离散采样，不把缺失证明补成成功绘制 |
| viewer reconnect | 采样窗口增加 2 次 | 原因尚未定位，不能从成功停止或 runner 通过推断性能健康 |
| CPU/资源 | 两机约 87.4–87.6 秒独立端点，进程身份配对完整 | 与媒体采样不同窗，不冒称 60 秒平均 |
| GPU | source 每端点 22 行、viewer 每端点 27 行真实 engine 读数 | source/viewer 每端点另有 8/6 个进程无 counter，记 unavailable，不填 0 |

GPU 读数是 Windows formatted counter 的离散前后端点，整数粒度；不跨 engine 求和，
不称 60 秒平均 GPU 利用率，也不据多项 0 认定 GPU 无负载。未做跨机时钟校准、
画质配准或物理呈现。本轮 raw 的跳帧、重连需先离线定位，不能与旧版本单次结果
直接比较并归因为版本收益或回归。

JPEG-1 在 11:41:50Z 被 `Required isolated ports unavailable` 拦截，未 claim run、
未启动 Hook/daemon/媒体、未创建 PC3 任务。端口快照中 47873/49875/49879/47881
分别有 856/1/2/1 个 `TimeWait`、PID 0 端点；门禁检查所有 TCP 状态而不只是监听。
没有放宽门禁、停止无关进程、网络重置或自动重跑。已传输的隔离文件保留。

11:43:57Z 再次只读核验：两机 Hook 均为 0，相关监听为空，raw 专用任务 Disabled，
JPEG 专用任务不存在。清理通过与端口暂不可复用是不同结论，不把前者冒充后者。

### 工具检查及后续边界

启动前修复旧模板遗漏的候选路径归一化，并补齐远端依赖 `Check-InteractiveOwner.ps1`，
未放宽断言。PowerShell/Node 语法、UTF-8 无 BOM、每文件小于 500 物理行上界和
远端依赖闭包检查通过；GPU 4 项及 Node 32 项聚焦回归通过，两组 ValidateOnly 通过。

本轮仅新增未配对 raw 证据，**不生成 JPEG/raw 收益结论，不关闭 A3-P 或 #67**。
下一步先离线核查 raw 重连/跳帧与测量工具影响；恢复原生前重新核验端口、隔离、
内存和证书有效期，不要求用户腾资源，不以物理断网或驱动重置排障。

## 后续离线定位：区分写入慢与窗口末尾重连

本次仅取回已结束试验的 PC3 runtime log 并分析原始回执，没有重启媒体。
可复核的脱敏分析见 [raw 时间线](issue67-a3p-raw-diagnosis-20261009.json)，
原始分析脚本位于上述证据根 `offline-diagnosis/Analyze-Raw.mjs`。

- daemon 窗口为 PC1 时间 `11:39:26.644Z–11:40:26.650Z`；Caddy 在同一主机记录的
  两次 `/resume` 请求开始约为 `11:40:37.566Z`、`11:40:38.598Z`，均返回 200。
  无需跨机时钟相减即可确认：这两次恢复发生在 daemon 采样结束之后。
- viewer 在自身采样开始后约 63.977 秒、65.140 秒观察到 reconnectCount 递增。
  前后离散样本均为 connected/hasError=false，并不证明中间没有瞬态异常；
  两次恢复也不是各隔 30 秒。不得将其与 daemon 的 60 秒窗口混为同一时间段。
- daemon 的 119 个不同 lastForward 槽中，socketWriteMs 中位数 94 ms、
  范围 0–179 ms；queueAgeMs 中位数 25 ms、范围 0–74 ms；adaptationMs 均为 0。
  这是采到的写入槽分布，不是全部写入的平均或全帧 p95，也不能将整数 0 当成零成本。
- 源码中 `viewerSkippedFrames` 是同 epoch、非零 cursor 之后，成功写出最新帧时
  跨过的 frame ID 数；daemon 选择 ring 最新帧而非逐帧排队。因此窗口内的写入慢与
  skip 相容，但不是网络丢包证明，且不能由窗口之后的这两次重连解释。
- `LiveRelayRuntimeState::mark_connected()` 在再次成功连接时递增 reconnectCount，
  同时清除错误。现有 PC3 runtime log 没有断开原因；250 ms 的普通读超时继续循环，
  10 秒 pending Ping 到期清除 RTT，并不直接触发重连。没有证据支持“30 秒定时重连”。
- viewer 观察器记录的是 240 次只读 DOM diagnostic evaluate，加上目标连接与末尾截图；
  未发现显式 reconnect 操作。这不排除采样、截图或主机调度的间接负载影响。

定位边界已缩小，但根因仍未建立：缺少断开瞬间的 socket error/Close reason 和
代理两端吞吐证据，现有数据不能区分物理链路、代理、接收端及调度背压。
不猜测修复超时、缓冲或编码策略，也不因原生 runner 通过而关闭性能异常。
后续原生测试应先明确怎样保留断开原因，再在隔离和资源条件满足时执行限定窗口；
这次离线分析不构成新的原生验收或配对通过。

## 断开原因取证候选 `.33`（构建阶段，后续原生结果见末节）

[Hook Draft #62](https://github.com/aiaimimi0920/Hook/pull/62) 在
`6acc92f17247c9e4dbf8e341b8a5be4ca7017312` 增加 acceptance-only viewer 断开记录。
只记录固定原因类别、IO kind/可选 OS code、数字 Close code、连接时长及数字计数器；
不保存原始错误文本、peer Close reason、媒体、URL 或凭据。每 worker 最多 32 条，
保留既有日志等级与 best-effort 有界队列，不保证每条必定落盘。state 采用 try_lock，
忙时计数器缺失，不为诊断等待锁。普通读超时、Ping、重连、授权和缓冲策略未改变。

软件验证：live_relay 62 项通过、4 项按原定义 ignored（其中新增 6 项）；格式、
严格行数及 diff 检查通过。独立审查发现的诊断锁等待已修复并增加锁竞争回归。
单观看端按专用进程绑定日志；该事件没有 relay identity，不能作多端逐会话归因。

隔离前端构建及 Tauri release 编译成功；候选来自上述干净源码，位置：
`Neuro/release/Hook/v0.2.32.33-viewer-disconnect-20261009/hook.exe`。
SHA256 `d1c8ed646dcf9dd88baf390a97285be11ce711b78d2e7bc9e858566ac3767d92`，
大小 9043968 字节，provenance 与 EXE 同目录。headless `--self-check` exit 0、status ok；
首次 PowerShell 包装未取得 ExitCode，保留原输出后用直接调用确认，未把空退出码视为通过。

本轮没有启动媒体或替换日常 Hook，尚未证明候选在真实异常中取得断开原因；
headless 自检也不替代 WebView2 原生启动/加载与业务验收。保留 `.32` 原始性能证据，
不把 `.33` 当性能修复或正式 release，不关闭 A3-P。

## `.33` 首次限定原生尝试：媒体启动前夹具共享冲突

后续“继续推进”执行了一轮单观看端 raw 取证，不包含 JPEG 或破坏性测试。
证据根 `linshi/issue67-v33-raw-diagnostic-20261009`；13:23:52Z 前置快照中
PC1/PC3 均无 Hook，内存余量约 41.03%/61.32%，相关监听为空。
生成新隔离 TLS 证书（到期 17:24:24Z），未改系统信任，候选 SHA 保持上述 `.33`。

本轮 runner 在 `Start-LocalFixture.ps1 → Assert-ContentMode.ps1:23` 读取
`source-direct/fixture-state.json` 时遭 Windows 文件共享冲突，停止于 `PC1 real capture`。
尚未发布/加入媒体、没有性能样本，也没有诊断到新的媒体重连。不能据此认定 `.33`
的取证功能已原生验收；这次失败不修改原有 `.32` 性能结论。

13:27:25Z 双机清理通过，专用任务 Disabled；13:44:41Z 只读复核两机 Hook 均为 0、
相关监听为空。未退出日常 Hook，未自动重跑，原始失败与清理回执保留。
脱敏索引见 [本轮回执](issue67-v33-raw-attempt-20261009.json)。

测试工具修正在 `staged-fix/`，没有改写已执行的 raw-1 脚本：

- 原启动读取只等待文件出现，未处理短暂共享冲突。相同 SHA 的 fixture 使用
  临时文件加 `File.Replace` 发布完整 JSON；不能把共享冲突解释成产品媒体故障。
- 新 `Read-FixtureStartupState.ps1` 共用最多 3 秒就绪期限，仅对 Windows sharing/lock
  violation 重试；缺失文件仍有界，JSON 错误及其他 I/O 错误直接失败。
- 原有 mode、HWND、revision、timer 和静态内容断言不变，未放宽身份或场景检查。
- 7 项本地回归通过，包括真实独占文件句柄冲突后释放成功、持续占用到期拒绝、
  非法 JSON 不重试、其他 I/O 错误不吞掉及大小上限；语法、UTF-8 无 BOM 和文件
  行数上界检查通过。首次测试的 JSON 错误文案预期与 PowerShell 5.1 不同，修正测试
  匹配并核验只读一次后通过，没有把非法 JSON 改为可接受。

修正尚未原生复测。恢复时需使用 fresh evidence root 并明确装入 staged 两个脚本，
重新检查隔离、容量、全部 TCP 状态和证书；不可覆盖本轮失败或绕过门禁。
本轮还联网核对 Hook #62 exact head 的 16 个 check runs：15 success、1 neutral，
无 pending/failure；neutral 为 `osv-scanner`，不称全部 success，也不替代原生验收。

## `.33` 第二次限定 raw：正常 Close 取证与清理通过

使用 fresh evidence root `linshi/issue67-v33-raw-diagnostic-20261009-r2`，不覆盖上述
失败现场。脱敏指标及 20 项原始文件 SHA256 见 [本轮索引](issue67-v33-raw-r2-20261009.json)，
交付前已逐项重新核对。此轮只执行单观看端 raw，没有计划或运行 JPEG。

14:11:55Z 前置检查中 PC1/PC3 均无 Hook，内存余量约 32.865%/61.057%，相关监听空；
隔离 TLS 有效至 18:12:14Z。两机使用上述 `.33` 候选，Loom 候选不变。修正的 fixture
reader 已装入新模板与 raw-1；启动前 7 项回归及 NativeHarness 检查通过，真实 WGC
捕获、正常 UI 发布/加入和 diagnostic buildVersion `.33` 已确认。原生启动校验通过，
但未记录实际 sharing violation 重试次数，不称本轮复现共享冲突后恢复。

| 指标 | 本轮 raw 实测 | 边界 |
| --- | --- | --- |
| daemon | 118 样本，59.514 秒；发布/成功写入均 1082，约 18.181 次/秒 | 不是显示 FPS |
| daemon 窗口差 | source gaps、viewer skipped、failed writes 均 0 | 不外推为完整负载达标 |
| viewer | 240 样本，62.905 秒；184 个不同绘制帧，56 次缺绘制证明 | 缺证明不补为绘制成功 |
| viewer 计数差 | reconnect 0，overwritten 2 | 未复现 `.32` 两次重连 |
| 媒体尺寸 | 观察为 658×407，启动回执为 439×271 | 保留原始回执，不宣称逐帧尺寸均已验证 |

30 秒预热后执行采样；CPU 是另窗，GPU 仅 process-engine 前后端点，不作平均利用率、
跨 engine 求和或跨机帧龄结论。没有画质配准、跨机时钟校准或物理呈现证据。

PC3 专用进程日志取得一条真实 `live_relay_viewer_disconnect`：
`reason=PeerClose(None) connected_ms=141734 epoch_frame_reconnect=Some((1, 2714, 0)) stop_requested=false remaining_budget=31`。
这是正常停止过程中的 Close 记录，证明该记录路径实际执行；没有异常重连，异常分支
尚未原生复现，`.32` 历史根因仍未知。`stop_requested=false` 是接收 Close 时状态，
不能单凭此认定故障；PC3 时钟未经校准，不直接与 PC1 UTC 相减。

runner 于 14:16:35Z 通过；正常 source stop 后 viewer closed、presentation/rendering
为 null、remainingImages 为 0。14:17:39Z closeout 通过：两机 Hook 为 0、相关监听空、
PC3 专用任务 Disabled，dailyExitCount 为 0。未退出日常 Hook、物理断网或重置驱动。

[Hook #62](https://github.com/aiaimimi0920/Hook/pull/62) 已正常 squash 合入 `main`
`67399b345e793db0fc6d921b6c1523a9d6f4c1d0`，与已验证 head `6acc92f` 的完整 Git tree
一致。合并前 exact head 检查为 15 success、1 neutral，无 pending/failure；不绕过门禁。
内部候选仍归属于原构建 SHA，不重标为合并后构建或正式 release。

**结论：限定 raw 执行、正常 Close 记录、停止清帧和清理通过；不代表诊断改动修复了
性能或历史重连。没有 JPEG/raw 配对收益结论，A3-P 与 #67 不结单，剩余项继续由 #89
承接。** 本轮不为追求异常复现而追加原生循环。

## `.33` 后续 raw → JPEG 配对尝试：JPEG 仍被端口门禁拒绝

后续“很好，继续推进”继续已授权的限定性能对照，使用全新证据根
`linshi/issue67-v33-paired-preflight-20261009`，固定 raw-1 → jpeg-1，每组预热 30 秒、
daemon 采样 60 秒；不是 raw → JPEG → JPEG → raw 完整重复顺序。两机候选字节与上一轮
相同，双方传输身份和 SHA 验证通过。原生执行仍使用无 daily-exit 的单组 runner，
未运行带历史日常退出/恢复分支的 `Run-NativeCross.ps1`。

14:28:51Z 前置回执通过：PC1/PC3 Hook 均为 0，内存余量约 25.449%/60.829%，全部专用
端口无现存 TCP 端点、独占绑定探测通过。新 TLS 有效至 18:30:45Z；模板语法、依赖闭包、
UTF-8 无 BOM、单文件行数上界以及两组 ValidateOnly 均通过。每组启动仍重新执行原门禁。

raw-1 于 14:35:35Z 完成发布/加入、采样、正常停止清帧和 owned cleanup：

- daemon 119 样本、59.906 秒；published/forwarded 均为 516，约 8.613 次/秒；窗口
  source gaps、viewer skipped、failed writes 增量均为 0。
- viewer 240 样本、63.122 秒；197 个不同绘制帧，43 次缺绘制证明，reconnect 与
  overwritten 增量均为 0。仍是离散观察，不是物理显示 FPS。
- 观察尺寸 658×407，启动回执 439×271；原记录保留。CPU 为另窗、GPU 为前后端点，
  不声明完整 GPU 平均、跨机帧龄或画质配准。

首组清理通过后留出 180 秒自然冷却，没有清空网络状态或更改系统参数。JPEG 于
14:38:57Z 的原门禁仍发现本地 49874 一个 `TimeWait` / PID 0 端点，错误为
`Required isolated ports unavailable`。它尚未 claim run、启动媒体或创建专用任务。
其他本地专用端口可绑定；没有放宽门禁、停止无关进程或自动重跑 JPEG。

14:40:50Z 双机只读 closeout 通过：Hook 均为 0、相关监听空、raw 专用任务 Disabled，
JPEG 专用任务不存在，dailyExitCount 为 0。脱敏数值、实际拒绝原因及 22 项证据哈希见
[本轮索引](issue67-v33-pair-attempt-20261009.json)。离线 `--raw-only` 分析器的通用
“JPEG 不在单 raw 范围”标签仅是该分析模式的默认文案；本轮实际情况是 JPEG 已计划但
被端口门禁拒绝，以索引和原始 preflight rejection 为准，不把它改称未获授权。

本轮写入速率低于上一轮同候选约 18.181 次/秒；没有匹配 JPEG 样本或完整受控负载，
不推断产品性能回归、收益或根因。下一次不得直接假定固定 180 秒冷却足以复用端口，
必须重新核验全部 TCP 状态及隔离/资源条件；不扩大物理网络操作权限。A3-P 仍未完成。

交付 CI：Hook 合并后 `67399b3` 已联网确认 17 项 success、公共 release 发布任务 skipped。
Loom 上一证据提交 `bce15b8` 最后检查为 16 项 success，Windows full validation 尚在运行；
不把这些状态冒充本节后续文档提交的 exact-head 检查。
