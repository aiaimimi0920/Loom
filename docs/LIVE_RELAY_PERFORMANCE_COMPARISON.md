# LiveRelay JPEG/raw 配对分析

本页记录 A3-P 的离线配对分析、阶段微基准与首组原生对照，不关闭端到端性能验收。
它复用 `measure-live-relay.mjs` 的样本选择与统计口径，不访问网络、不修改会话。

## 输入与运行

分别保存 raw/JPEG 的完整采样报告，将每份报告包装为以下 JSON 对象：

```text
{
  "conditions": {
    "hookSha256": "实际 Hook EXE 的 64 位小写 SHA256",
    "loomSha256": "实际 daemon EXE 的 64 位小写 SHA256",
    "contentFixtureSha256": "内容夹具文件的 64 位小写 SHA256",
    "captureSettingsSha256": "公共采集设置文件的 64 位小写 SHA256",
    "networkConditionsSha256": "网络条件记录文件的 64 位小写 SHA256",
    "width": 658,
    "height": 407,
    "targetFps": 30,
    "scenario": "static-text"
  },
  "report": "替换为采样工具产生的完整 JSON 对象，而不是字符串"
}
```

上例是结构说明，不能直接作为有效输入。`scenario` 还可为 `scrolling` 或
`motion`。两份输入必须声明完全相同的条件；公共采集设置不包括本次自变量
codec，但应包含采集方式、尺寸、目标帧率等。另行保留各端完整实际设置、
JPEG 质量和画面证据，不能用相同的声明 hash 证明实际条件或视觉质量相同。

从 Loom 根目录运行（输入路径须替换为真实配对报告）：

```powershell
rtk proxy node scripts/compare-live-relay.mjs C:/Users/Public/nas_home/AI/GameEditor/linshi/raw.json C:/Users/Public/nas_home/AI/GameEditor/linshi/jpeg.json C:/Users/Public/nas_home/AI/GameEditor/linshi/comparison-new.json
```

仅接受普通 UTF-8 JSON 文件，每份不超过 16 MiB。输出使用独占创建，不覆盖已有
路径。创建后若磁盘写入失败，可能保留不完整文件；失败退出不是有效报告，应保留
失败现场并使用新的输出路径重试，不自动删除用户路径。

## 可比性校验

- 仅单观看连接；样本中 source、viewer、epoch 必须稳定，source 在线且未关闭。
- baseline 的可观测 `lastForward` 必须为 `raw_bgra`，candidate 必须为 `jpeg`。
  这只检查采样观测，不能排除采样间隙的编码或连接变化。
- 必须正常采满时长；拒绝失败写入增量、计数重置、源帧/转发帧回退、
  写入计数增长但转发帧不前进、转发观测消失和无实际转发增量。
- 两份采样设置相同；窗口差不超过一个采样间隔，样本数差不超过 1。
  各窗口不得短于 `max(1, durationMs - 2 * intervalMs)`，不得长于
  `durationMs + 1`。速率按各自实际窗口归一化。
- 忽略导入的摘要，使用逐样本数据重新计算统计。不验证输入真实性或文件签名。

## 输出与边界

输出平均每次成功 daemon socket 写入的 binary bytes、JPEG/raw 比值、字节减幅，
以及两份重算的计数、速率与采样延迟分布。减幅为负表示 JPEG 每次写入更大。
固定输出 `conditionsVerified: false` 和 `endToEndVerdict: "not-established"`。

socket 写成功不是接收确认；`lastForward` 分布不是全帧 p95，queue-age 与
adaptation 可能重叠，不能直接相加。本工具没有 CPU/GPU、跨机时钟校准、
画质或物理呈现测量。测试夹具中的 75% 减幅不是产品实测收益。

真实同条件的静态文字、滚动、运动及目标观看端数矩阵仍待完成；原生 JPEG
单边回执不能充当 raw 对照。参见 [A3-P 工作包](LIVE_RELAY_ACCEPTANCE_WORKPACKAGES.md)。

## 聚焦回归

在当前 Windows 工作区将测试临时目录限制到 `linshi`，再执行：

```powershell
$env:TEMP = 'C:/Users/Public/nas_home/AI/GameEditor/linshi'
$env:TMP = $env:TEMP
rtk proxy node --test scripts/tests/compare-live-relay.test.mjs scripts/tests/live-relay-measurement.test.mjs scripts/tests/measure-live-relay.test.mjs
```

## 2026-10-07 本机编码阶段补充证据

复用 Hook 的 `live_jpeg_encode_benchmark`，未修改生产源码或基准算法。
从 Hook 根目录执行：

```powershell
rtk proxy cargo test --locked --release --manifest-path src-tauri/Cargo.toml live_jpeg_encode_benchmark -- --ignored --nocapture --test-threads=1
```

首次构建耗时约 15 分钟，测试 1/1 通过；存在 Cargo `hook_lib` 输出文件名冲突
警告，未变成构建失败。随后对同一测试 EXE 独立启动 3 次相同基准，均通过。
每次每尺寸 18 个合成 RGB 帧编码样本，没有显式预热，不读取桌面像素。

| 尺寸 | 首轮 median / p95（ms） | 后续三轮 median（ms） | 后续三轮 p95（ms） |
| --- | --- | --- | --- |
| 640×360 | 9.064 / 22.005 | 4.201、4.638、4.495 | 5.879、6.029、8.111 |
| 1280×720 | 12.916 / 15.047 | 8.023、8.346、8.017 | 10.371、10.213、12.338 |
| 1920×1080 | 20.541 / 29.407 | 13.423、14.114、13.154 | 18.549、16.825、15.801 |

保留首轮与重复轮次的差异，不将其归因于未经测量的具体原因，也不挑最快轮作为
整体结论。这里沿用测试输出名：`median` 是排序后第 10/18 项（上中位数），
`p95` 是第 18/18 项，即本轮最大值，不是生产全帧 p95。

同一 release 测试 EXE 的 codec 聚焦回归为 2 passed、1 ignored：空尺寸拒绝与
编码后尺寸保留、WIC RGB 通道顺序通过；ignored 项已由上述显式基准命令执行。
包装编码函数优先 WIC、失败时回退软件编码，本基准没有逐次记录后端；WIC 专项
测试通过不能证明所有计时样本都走 WIC。计时包含该函数内的编码、分配及输出释放，
不包含桌面采集、网络、接收解码或呈现。

证据根：`linshi/issue67-a3p-encode-20261007`。`receipt.json` 保存测试 EXE、
codec/WIC 源码与 Cargo.lock 的 SHA256、CPU 信息及重复轮次，原始日志全部保留。
这不是候选 Hook 应用包的性能认证，不是 raw/JPEG 对照，未测 CPU 占用率/GPU、
画质或端到端帧龄，不关闭 A3-P，也不单凭本数据触发 B/C 阶段改造。

## 同输入源端适配微基准

Hook 的 `live_relay_paired_adaptation_benchmark` 直接调用生产函数
`encode_live_relay_capture_frame`，使用相同 JPEG 输入比较两个协商 profile。
当前 `Legacy` 是把已有 JPEG 解码为 BGRA 再封装，并非未经过 JPEG 的原始桌面
像素流；`Jpeg` 保留压缩 payload，但仍执行校验、协议封装、分配与复制，不是零拷贝。

从 Hook 根目录执行：

```powershell
rtk proxy cargo test --locked --release --lib --manifest-path src-tauri/Cargo.toml live_relay_jpeg_tests -- --include-ignored --nocapture --test-threads=1
```

该命令同时运行原有 JPEG 合约回归和显式忽略的性能基准。基准对 `658×407`、
`1280×720`、`1920×1080` 分别生成固定合成图，先用软件 JPEG encoder、质量参数
82 生成一份共同输入。夹具生成不计时，也不是 WGC/WIC 捕获或真实文字/运动场景。

每尺寸先预热 4 对，再记录 20 对；逐对交替 profile 顺序，输出 `first` 与每次
`jpegMs` / `legacyRawMs`，保留输入 SHA256 和含 64 字节 NLLV 头的封包长度。
计时覆盖完整源端适配函数，不含输出释放、接收解码与断言。每次都核验 JPEG
payload 保持原样，raw BGRA 等于输入 JPEG 解码后的像素，而非原始合成图的像素。

两次计时之间的验证会影响缓存、分配器和调度；交替顺序只缓解偏差，不证明消除了
偏差。固定图像、少量样本不能推出总体尾延迟、画质、CPU/GPU 占用率或网络吞吐。
`paired_adaptation=` 后是 JSON 原始记录，其 `evidence` 为
`synthetic-source-adaptation-only`，`endToEndVerdict` 为 `not-established`。
这不是 daemon 采样报告，不应伪装成前述离线比较工具的输入。

2026-10-07 实测：release 聚焦命令 4/4 通过，随后同一个测试 EXE 的基准重复
2 次通过，共 3 轮。下表是各轮 20 样本中位数的范围，中位数取排序后第 10、11
项的平均值；不是跨轮汇总分位数，也不与上一节的上中位数混用。

| 尺寸 | JPEG 适配中位数范围（ms） | legacy raw 适配中位数范围（ms） | JPEG / raw 封包字节数 |
| --- | --- | --- | --- |
| 658×407 | 0.08890–0.09250 | 9.56175–10.19660 | 68,928 / 1,071,288 |
| 1280×720 | 0.13155–0.18015 | 31.69655–32.41300 | 235,059 / 3,686,464 |
| 1920×1080 | 0.19205–0.21715 | 71.32795–75.09070 | 527,605 / 8,294,464 |

这三份固定输入的 NLLV 封包字节减幅分别约为 93.57%、93.62%、93.64%；仅为
本合成夹具的封包长度差，不是实际网络吞吐、TCP/TLS 开销或产品全负载收益。
两条路径均从同一有损 JPEG 出发，像素对照通过不代表对原始桌面无损。

证据根：`linshi/issue67-a3p-adaptation-20261007`。保留完整构建/测试日志、
两轮重复日志和 `receipt.json`；回执包含全部 180 对计时样本、输入 hash、测试
EXE/源码/Cargo.lock SHA256、CPU 信息及审查边界。独立只读审查未发现本限定
范围内的阻塞问题。此证据支持既有 JPEG 路径避开源端额外解码/展开的阶段性
判断；不据此修改生产策略、关闭 A3-P 或宣称端到端流畅度提升。

## 首组双机原生 motion 对照（2026-10-07）

真实 WGC 动态窗口经 PC1 → LAN HTTPS/WSS → PC3 原生 Hook 单观看端传输，
正常 UI 发布并用 Surface 参数加入；SSH 仅用于管理/CDP，媒体不走 SSH。
固定顺序 raw → JPEG，各一次，每轮预热 30 秒、请求 daemon 采样 60 秒。
唯一 raw CDP 观察器未启用 `Network.enable`，另采集 240 次浏览器呈现状态。

两组使用同一内部 `.25` 候选、daemon 和 motion 夹具，独立隔离数据与相同代理路径：

- Hook SHA256：`401c0c94f7953d3eeb2a993a940e7a622920618e6f5844f44ee512e172db15a1`。
- Loom SHA256：`acf6fa5e70ca5ad826f85c0ef9ff6145adaa7b8d68168081771ead225dd7df44`。
- 内容夹具 SHA256：`db45d8e33398d9a0164fb0a5da8b4a94556e10633fd27237b2bdc02e962a4305`。

专用 Caddy 只对 `/v1/live/media` 的 source/viewer WebSocket upgrade 请求改写
`Sec-WebSocket-Protocol` 为 `loom.live.v1` 或 `loom.live.jpeg.v1`；不改响应选择、
控制接口或其他 WebSocket。raw 路径仍是捕获 JPEG 后解码展开 BGRA，**不是未经
JPEG 的无损原始采集**。本轮未改产品代码、未编译新包，也不是正式发布认证。

启动 capture/publish 回执为 `439×271`、`targetFps=60`，但两组各 240 次稳定
presentation 采样均为 **`658×407`**；实际媒体尺寸不能写成启动尺寸，差异原因尚未
归因。raw payload 为 `658×407×4 = 1,071,224` bytes，加 64 字节 NLLV 头后与
daemon 实际记录的 `1,071,288` bytes 一致。目标 60 FPS 不代表已达成的发送或显示 FPS。

### 发送层与离散绘制证据

| 指标 | raw | JPEG |
| --- | ---: | ---: |
| daemon 快照数 | 119 | 119 |
| 首尾计数比较窗口（秒） | 59.537 | 59.567 |
| 成功 socket writes 增量 | 1,214 | 886 |
| forwarded binary bytes 增量 | 1,300,543,632 | 32,928,331 |
| 成功写入次数/秒 | 20.390681 | 14.874007 |
| 平均 binary bytes/成功写入 | 1,071,288 | 37,165.159142 |
| forwarded binary bytes/秒 | 21,844,292.322 | 552,794.853 |
| 离散呈现采样数 | 240 | 240 |
| 不同已核验浏览器绘制帧 | 169 | 189 |
| 无当前绘制证据的采样数 | 71 | 51 |

平均每次成功写入的字节数下降 **96.530797%**。但 **JPEG 本轮写入频率低于 raw**，
尚未归因，不能将字节减幅或采样绘制帧数解释为更流畅。socket 写入不是接收确认或
呈现 FPS；无当前绘制证据的样本不是丢帧数，也不是全帧历史。binary bytes 不含全部
TCP/TLS 开销。两组窗口内 failedWrites、viewerSkippedFrames、sourceSequenceGaps
增量均为 0；raw 窗口起点累计 sourceSequenceGaps 已为 4，不能说从启动起无 gap。
bufferEvictions 表示有界缓存淘汰，不直接等于丢帧。

### CPU 与资源端点

CPU 按进程组累计 CPU seconds ÷ 实际端点间隔 ÷ 本机逻辑核数归一化，source 为
12 核、viewer 为 16 核；两组进程集稳定、端点完整。此窗口不等于 daemon 约 60 秒窗口。

| 指标 | raw | JPEG |
| --- | ---: | ---: |
| source / viewer 端点间隔（秒） | 88.778900 / 88.450058 | 89.637089 / 89.171716 |
| source Hook tree 平均整机 CPU 占比 | 3.917445% | 2.343071% |
| viewer Hook tree 平均整机 CPU 占比 | 7.437106% | 4.114472% |
| daemon 平均整机 CPU 占比 | 0.605730% | 0.174314% |
| source Hook Private MiB 净差 | -0.593750 | -2.730469 |
| viewer Hook Private MiB 净差 | +7.234375 | +5.335938 |

这不是峰值、GPU 测量或无泄漏证明；两轮实际帧量不同，不能直接归因为每帧效率收益。
离线比较工具本身不测 CPU，以上数据来自独立资源端点回执。

### 证据入口、恢复与未覆盖项

证据根：`linshi/issue67-a3p-native-pair-20261007T1325Z`（相对 GameEditor）。
正式入口为 `native-pair-summary.json` 和 `native-comparison-observed-dimensions.json`。
raw/jpeg 各自保留 `pair-input.json`、派生 `pair-input-observed-dimensions.json` 与
`pair-dimension-provenance.json`；旧 `native-comparison.json` 也保留。派生仅根据完整
呈现采样修正 envelope 尺寸及对应 settings hash，不修改原始 daemon report，不能证明
采样间隙所有帧尺寸或严格画质一致。输出仍为 `conditionsVerified=false`、
`endToEndVerdict="not-established"`。

初次代理 smoke 因 Node fetch 的 `Sec-Fetch-Mode` 被管理 API 拒绝，未进入媒体握手；
改用 raw HTTP 管理请求后 16 项通过，未放宽安全策略。raw 媒体与停止清帧已通过，
但 cleanup guard 把 null Trigger 误算为一个触发器，原父/子 runner 因此失败；修正
null 过滤并补断言后，fresh cleanup 通过。同一授权退出窗口内仅续跑 JPEG，未重跑
raw、未再次退出日常。原失败回执不覆盖；`pair-resume-receipt.json` 记录恢复通过。

两组停止清帧、`presentation/rendering=null`、双机专用进程/监听清理通过，截图已核看；
日常 `.23` 已恢复。本轮只正常退出日常一次，不改系统网络或驱动。此许可已使用完毕，
后续需再次退出日常的测试应另行确认。

仅一个固定顺序、一次/codec、单观看端运动场景，没有交叉顺序重复、严格画质配准、
GPU、跨机时钟校准或物理显示测量。静态文字、滚动与目标观看端数矩阵仍待完成；
应先解释频率差异并补交叉顺序证据，不据此关闭 A3-P、A3-D 或总 Issue #67。

## 已有采样的节奏判因（不重跑原生）

2026-10-07 对上述原始 daemon/presentation 报告做离线分段，不重新退出日常程序。
按首个 daemon 样本起，以首个达到 10 秒的后续样本为分段终点，相邻分段共享边界，
最后一段保留余量；按每段实际时长归一化，而非对采样速率取平均。

| 分段顺序 | raw 发布次数/秒 | JPEG 发布次数/秒 |
| --- | ---: | ---: |
| 1（约前 10 秒） | 20.785679 | 7.989614 |
| 2（约 10–20 秒） | 20.178042 | 7.684729 |
| 3（约 20–30 秒） | 20.402100 | 15.567675 |
| 4（约 30–40 秒） | 20.144884 | 19.690854 |
| 5（约 40–50 秒） | 20.424351 | 18.604651 |
| 6（最后约 9 秒） | 20.412643 | 20.205366 |

**JPEG 的较低整轮均值包含采样窗口前段低速、后段恢复；不是整轮持续维持相同低速
的现象，不能据此确认 codec 固定限速。**
本次 raw 段长依次为 10.055/10.110/10.097/10.077/10.086/9.112 秒，JPEG 为
10.013/10.150/10.085/10.157/10.105/9.057 秒；两组不是绝对时钟对齐实验。
保留完整窗口结果，不删除 JPEG 前段、换选后段或追认更长预热来提高通过率。

两组各 118 个相邻采样区间的 source frameId 增量均等于 publishedFrames 增量，
没有零发布区间；各上述分段 forwardedFrames 增量也等于 publishedFrames 增量。
全窗口 sourceSequenceGaps、viewerSkippedFrames、failedWrites 无新增。这把观察到的
低速定位到 **daemon 收到源帧时已经存在**，没有证据表明是 daemon 丢弃已收到的帧
造成；不能因此排除网络背压、采样间隙或编码前丢弃，更不能称 WGC 从未丢帧。

viewer 独立采样窗口 raw 为 63.159800 秒、JPEG 为 63.751200 秒，receivedFrames
增量为 1,283/1,096，按各自窗口折算为 20.313554/17.191833 次/秒；这是接收计数，
不是显示 FPS，也不能直接与未对齐的 daemon 窗口相减求丢帧。两组 reconnectCount
增量均为 0；viewer overwrittenFrames 增量为 0/1，与 daemon viewerSkippedFrames
是不同层的计数，不相互替代。

每组 240 个不同的已提交 presentation 槽样本中，源端 `encodeTimestampMs` 减
`captureTimestampMs` 的中位数为 raw 29 ms、JPEG 20 ms，范围为 13–88/7–74 ms。
当前 `Hook/src-tauri/src/native/live_relay_protocol.rs` 在 representation 适配后写入
wire encode timestamp，因此该差值包含自 capture timestamp 起的排队、readback/编码
及源端适配，不是纯 JPEG
编码耗时；两时间戳来自源端，不做未校准的跨机相减。此离散总体没有显示 JPEG 的
该项中位数更高，但不能证明缺失帧、前段低速或所有帧的原因。

### 已核代码机制与下一次测量边界

当前工作区 source loop（Hook `src-tauri/src/native/live_relay_websocket.rs`）对两种
profile 共用同步 socket service → latest frame → representation → send → 10 ms sleep；
`live_relay_connection.rs` 的 source read timeout 请求值为 1 ms，不能等同实际等待时间。
`live_relay_protocol.rs` 仅 raw 分支额外做 JPEG 解码/BGRA 展开，没有 JPEG 专属的
低目标帧率分支。静态代码检查是机制候选，不是运行中逐阶段耗时证据。

两种 profile 都依赖同一 JPEG 生产链。`live_gpu/work_budget.rs` 的 process-wide CPU
预算为 24,000,000 pixels/s、30 frames/s；permit 释放后的 due 还受实测持有时间冷却
约束，WGC/latest mailbox 与动态 cadence 也能降低生产率。目标 60 FPS 因此不是 CPU
编码链的保证；这些共同机制不能直接解释本次两组差异。已有源端日志没有保存完整
WGC arrival、permit admission/rejection、readback/编码耗时及 source read/send 时间线，
不足以区分生产较少、预算拒绝、发送等待或系统负载变化；**根因尚未建立，不改产品策略**。

下一次对照应事先固定交叉顺序及重复次数，保留每组完整相同窗口，并在相同观察负载下
补充 source capture/frame/drop 状态、预算与分阶段计时。若现有诊断无法区分，应先做
有界测量接线及聚焦测试，不直接修改 10 ms sleep、预算阈值或重跑后挑选较快片段。
任何再次退出日常的原生测试仍需另行确认。

分析证据根：`linshi/issue67-a3p-cadence-20261007`。`cadence-analysis.json` 保存四份
输入 SHA256、精确分段、独立 viewer 窗口与限定结论；离线分析工具聚焦测试 7/7 通过。
无新增原生运行、无产品源码修改；该判因记录不关闭 A3-P。

### 源端状态采样准备（未执行原生）

已在 `linshi/issue67-a3p-source-observer-20261007` 增加下一轮使用的只读观察模块，
复用现有 `get_live_capture_status` 与 `get_live_relay_status`，不新增产品接口或依赖。
只取 capture 编码帧状态、source relay 发送计数、尺寸/可见性/epoch/重连状态；不读
像素、不消费帧队列、不改变 producer demand，不启用 Network 记录。父 runner 必须
传入已经绑定的唯一 raw CDP session，并执行精确 owner 的前后校验和统一清理。

默认60秒、每次响应后等待1000ms，配置上限120秒/241样本，单次返回最多8192 UTF-8
字节；保存真实请求/响应时刻。身份漂移、epoch变化、重连、计数回退、取消或异常即
停止，保留失败前的样本。两个状态命令顺序执行，不是原子快照；不得把一对 frameId
相减当队列深度，source `receivedFrames` 也不是远端接收确认。

本地夹具覆盖只读命令集合、隐私字段投影、顺序时钟偏差、身份/epoch/reset/重连、
取消/超时/大小上限和 owner 失败；尚未接入真实双机 runner，不能宣称原生验证通过。
现有接口仍无 WGC arrival、预算拒绝和逐阶段耗时，不能代替完整瓶颈测量。完整接线
约束见工具目录 README；下一次原生运行需要新的日常退出许可。

随后在独立 `linshi/issue67-a3p-source-runner-20261007` 完成 staged runner 接线：
daemon、viewer、source 三个观察任务由测量主进程 `Promise.allSettled` 汇合，源端
报告通过与CDP连接关闭均为停止媒体前的门禁。旧试验目录和回执不改写。
审查发现以RTK wrapper作为子进程时，timeout不能证明内部observer已退出；新模板
改为进程内观察函数，OS查询直接持有实际PowerShell helper并等待close，不把wrapper
退出当成子树退出。前后owner仍核路径/创建时间/SHA、listener与target身份。

本地20项通过，包括3项真实本机无业务PowerShell helper的正常/超时/取消退出核验；
其余为状态与生命周期夹具，不是原生双机媒体证据。模板要求绑定新证据根的新批准，
未装配运行资产、未启动Hook或远端、未再次退出日常。下一步为获批后的交叉顺序对照，
仍不能以本工具接线关闭A3-P或认定JPEG前段低速已修复。

### 首次交叉顺序尝试：源端结束校验失败，停止后续组

用户重新确认后，第一次前置检查被本机内存门禁拦截，没有退出日常；后来本机三次
余量恢复至38.63/38.66/38.90%、PC3约58.76%，重新核验同SHA `.25` 与daemon，
四组文件传输及188项工具/资产一致性检查通过，才按新许可正常退出日常一次。
计划顺序 raw-1 → jpeg-1 → jpeg-2 → raw-2；实际仅启动 raw-1。

raw-1 的daemon得到119个样本、59.777秒窗口、1,239次发布与约20.727035次/秒；
viewer的240次采样核到175个不同软件绘制帧，截图已查看。source取得60个状态样本，
首尾跨度59.864秒，capture.frameId与source relay发送计数各增加1,245，约20.797140
次/秒。这是**失败试验中的诊断数据**，窗口不对齐，不作跨层丢帧相减，也不构成
新raw/JPEG配对或原生通过结论。

source报告为`owner-postflight-failed`、`cleanupPassed=false`。其after归属回执留下
0字节文件，写入时刻约在8秒helper期限附近；时间关系支持辅助进程deadline打断结束
校验/写入的判断，但旧脱敏报告未保存退出码或signal，**不能确证唯一根因**，也没有
产品JPEG退化证据。原0字节文件、source失败报告和父/子失败回执全部保留。

父runner按门禁停止，jpeg-1/jpeg-2/raw-2均未启动，没有重跑raw。测量失败后没有进入
正常UI停止清帧验收；而是执行失败清理，双机专用进程/监听及远端任务清理通过，
日常 `.23` 恢复通过，父回执`dailyExitCount=1`、`cleanupErrors=[]`。不要把失败清理
写成正常停止清帧通过。本次退出许可已使用，后续原生运行需另行确认。

试验证据根：`linshi/issue67-a3p-cross-order-20261007T1500Z`，入口为
`pair-runner-receipt.json`、`raw-1/source-cadence.json`、`raw-1/part-cleanup-verified.json`
及`original-restored.json`。A3-P仍未完成。

随后仅修正staged工具：source归属校验去掉无关CPU/资源全量快照，改为一次进程树
查询及一次端口定向TCP查询，保留PID/path/creation/SHA、listener与连接数检查；
JSON先序列化再独占创建文件。helper失败新增脱敏退出原因、期限、用时与close证据，
不记录命令参数/stderr，不延长8秒helper或10秒owner期限。7项PowerShell owner夹具
及20项Node测试用于保护这些边界；**修正尚未重跑原生，不能宣称故障已原生修复**。

### 再次交叉顺序尝试：实时选区取消，发布前失败

新的批准窗口使用证据根 `linshi/issue67-a3p-cross-order-20261007T1554Z`。初次前置
检查因本机内存余量低于15%停止，`pair-runner-receipt.json`记录零退出、零组启动。
保留原回执后只读有界观察，连续三次余量33.00/32.15/32.96%，才用独立续行回执
`capacity-retry-runner-receipt.json`继续；没有停止无关进程或降低正式容量门槛。

本轮正常退出日常一次，实际只启动raw-1。实时捕获日志出现计划区域外的
`capture_mouse_down :: x=989 y=926 flags=0`，随后是`selection-end-small :: w=0 h=0`
及`set_capture_input_active :: false`。这只能证明捕获被零面积点击取消，不能确定
该输入来自谁。旧helper仅检查曾出现active=true，取消后仍继续计划拖选，最终
`.unit-live-input`等待20秒超时，失败阶段为`normal publish and join`。

本次没有进入性能采样；jpeg-1/jpeg-2/raw-2均未启动，也没有验证上一轮source
observer修正的原生效果。失败清理后恢复日常`.23`；16:14:12 UTC重新核验PID32052
的路径、创建时间及SHA一致，本地专用监听为零，PC3 Hook/专用监听为零且任务Disabled。
`failure-closeout.json`保存该核验。失败清理不等于正常UI停止清帧验收。

仅修正staged工具，不改写本轮脚本和历史失败证据：新增Capture-Readiness门禁，
检查最新active状态、取消后立即失败；成功回执要求唯一的计划注入起点在前、唯一
完整非零有限宽高的selection-end在后。远端传输清单同步加入依赖。20项聚焦检查
通过，包含本轮真实失败日志、顺序错误、伪造数值前缀及非有限数值负控。
这只是检测与证据修正，不能保证消除外部输入竞争，尚未原生复测。A3-P仍未关闭，
本轮退出许可已经使用；下一轮退出须另行确认。

### PC3重新登录后的尝试：采样与正常清帧通过，退出身份门禁失败

新证据根为`linshi/issue67-a3p-cross-order-20261007T163624Z`。PC3管理连接及SSH曾
失败，此时未退出日常；用户重新登录后连接恢复，确认远端四组目录均不存在，再完成
四组上传/hash核验。原`preparation-blocked.json`保留，恢复记录为`transfer-recovery.json`。
本机容量短时低于15%，有界观察末尾及补充核验恢复至约33%后，才正常退出日常一次。

实际raw-1正常捕获、发布、加入、采样通过；source取得60个样本，首尾59.8278秒，
capture计数增加431（约7.204009/s），source发送计数增加430。source报告和清理通过，
四个helper前后检查均为`passed=true`、`childClosed=true`。viewer采样240次，核到
196个不同软件绘制帧、44次无当前绘制证据；这不是屏幕FPS。此次证明新的选区门禁
及source observer接线在该raw试验中完成，不是完整四组对照或故障永不复发证明。

正常UI停止及观看端清帧通过：`reversed-stop-receipt.json`与`reversed-closed-receipt.json`
均通过，remainingImages=0，presentation/rendering为空。随后清理报
`source normal exit: Normal quit owner changed`，因此子组和父组均保留失败，后三组
未启动。源端日志包含本次PID23164的tray_quit、tauri_exit_requested和tauri_run_returned；
说明发生过正常退出路径，但旧helper没有保存身份不匹配当时的实际字段，不能确定是
退出期间查询竞争、PID复用还是其他原因，不放宽owner门禁，也不追认整个试验通过。

17:01:06 UTC重新核验日常`.23` PID49396路径/创建时间/SHA一致，本机专用监听为零，
PC3 Hook/专用监听为零且任务Disabled；见`failure-closeout.json`。最终资源清理通过
不抹去原始正常退出身份门禁失败。raw速率也出现低值，仅说明不能把既往JPEG低速
直接归为codec问题；不同轮次不是同条件配对，尚不能判断原因或收益。A3-P仍未完成。

随后只补staged正常退出诊断：每次查询保留预期与实际PID/path/creation、阶段、
重试序号、等待结果及UTC；查询异常不再静默视作进程不存在。身份不符仍立即拒绝，
最多三次正常菜单退出且无强杀。诊断采用唯一文件名、CreateNew、32KiB上限和字段
投影，不记录命令行/原始异常文本，写入错误不覆盖原业务错误。远端依赖清单同步。
18项离线检查通过，含真实脚本适配器的退出请求前分支；没有再次退出日常或原生
复测，也没有确认上一轮身份不匹配的唯一原因。查询/probe总deadline及按PID等待
句柄的既有边界未在本次扩大修改，最终身份绑定清理仍必需。

### 新退出诊断首次运行：管理转发端口冲突，媒体启动前停止

`linshi/issue67-a3p-cross-order-20261007T173049Z`按新许可正常退出日常一次。四组传输
及修正工具hash核验通过，本机三次容量约27%、PC3约53%。日常退出的新诊断记录
身份一致、首次请求后`waitResult=exited`。raw-1随后停在`management tunnel`：SSH报
`bind [127.0.0.1]:49881: Permission denied`，本机独立socket绑定复现错误10013。
PC3 HTTPS前置检查为200，但尚未启动远端观看任务/本机候选捕获，更未进入采样；
后三组未启动。此次不能验证此前source退出身份不匹配的原因或四组性能。

只读查询发现本地49881存在PID17348（FlClashCore.exe）的IPv6 Bound/Established
端点，而不是Listen。只检查监听的旧门禁漏过了该冲突；未停止该进程，未修改网络、
防火墙、驱动或端口排除配置。证据为`port-conflict-diagnosis.json`。17:34:57 UTC
fresh核验日常`.23` PID43744身份/hash一致，本地测试监听为零，PC3 Hook/测试监听
为零且本轮远端任务未创建；`failure-closeout.json`通过，不抹去原试验失败。

仅在staged父runner退出日常之前新增双机端口检查：本地49873/49874/49875/49876/
49879/49881、远端49880/49882，任何TCP状态占用均拒绝，再做立即释放的独占IPv4
loopback bind探测；不启动监听、不修改系统配置。13项聚焦测试通过，当前49881
真实冲突也被新门禁拒绝。此检查是时点快照，不保证之后无抢占或LAN地址可绑定，
运行期原有失败与清理门禁保留。本轮许可已使用，未自动换端口或再次退出重跑；
A3-P仍未完成。

随后在staged工具中将本地viewer管理转发入口迁至47881，同步SSH、CDP发现与地址
重写、UI控制、父前置和清理，远端49880/49882、source49879及媒体入口保持不变。
本机实查动态TCP范围49152–65535，新端口在范围外且bind通过，但不是永久预留。
六项接线测试通过，独立只读复核无遗漏。短时SSH管理转发第二次smoke确认双向
loopback监听和exit=0，退出后两端监听消失、日常PID43744身份/hash未变；第一次
smoke的active观察错过进程窗口，失败回执保留，不用exit=0追认完整观察通过。
证据在staged目录management-port-smoke-r2.json和management-port-smoke-closeout.json。
未启动Hook或媒体、未改系统网络、未停止FlClash；管理转发通过不代替A3-P原生验收。

### 新端口轮次：父子授权合同不一致，子程序claim前拒绝

`linshi/issue67-a3p-cross-order-20261007T182134Z`按新许可装配当前模板；端口、双机
容量及传输检查通过，父程序正常退出日常一次。完整模板覆盖时带入旧子runner的
userAnswer逐字检查，只接受旧回复“允许，当前编辑已保存”，因此拒绝了当前已明确
授权的回复。根因是测试工具父子合同不一致，不是用户授权缺失，也不是媒体失败。
raw-1子进程执行了前置检查，但未claim、未启动服务/捕获/观看；后三组未启动。

子stdout保留`Explicit normal-exit approval required`，父随后缺runner-receipt错误
也保留。18:25:42 UTC核验日常`.23` PID33936路径/创建时间/hash一致，本地测试监听
为零，PC3无Hook/测试监听且本轮任务未创建；见`failure-closeout.json`。

staged父子改为共用结构授权/root绑定函数，原始用户回复仅归档，不逐字匹配；新增
子`-ValidateOnly`，父在退出前演练全部四组授权。子前置拒绝保存独立回执，父缺回执
保留退出码和明确日志指针。14项真实脚本离线授权回归及26项Node回归通过；不放宽
明确permission、证据根及observer批准要求。此次修正尚未原生复测，A3-P仍未完成。

### 三组通过，末组源端结束校验超时（2026-10-07）

`183441Z`前置检查发现日常Hook已不存在，零退出、零组启动；没有证据解释其退出
原因。随后新根`linshi/issue67-a3p-cross-order-20261007T183819Z`采用明确的
`-DailyAlreadyAbsent`分支：要求无任何Hook、已知日常`.23`路径/hash匹配，记录
`dailyExitCount=0`、`dailyAlreadyAbsent=true`、`normallyExited=false`。未虚构正常退出；
本轮结束后仍启动已核验的日常包。该分支的3项离线负控/正控通过。

四组均使用前述同SHA `.25`、daemon、motion夹具、单观看端与相同预热/采样窗口。
raw-1、jpeg-1、jpeg-2完整通过采样、正常停止清帧和清理；源端每组60个状态样本、
四个owner helper全部通过并关闭。前三组截图已核看，但不是严格像素质量配准。

| 组 | daemon发布次数/秒 | source发送计数/秒 | 不同已核验离散绘制帧 |
| --- | ---: | ---: | ---: |
| raw-1 | 20.416025 | 20.358797 | 184 |
| jpeg-1 | 20.592709 | 20.462879 | 194 |
| jpeg-2 | 20.444415 | 20.324119 | 190 |

独立窗口不可相减求丢帧，离散绘制帧不是屏幕FPS。首对平均每次成功daemon写入的
binary bytes减幅为96.530740%；jpeg-2只作为未配对重复组。两次JPEG的六个约10秒
分段均约20.2–21.0次/秒，本轮没有重现旧JPEG前段约8次/秒的现象；不能据此确认
旧现象根因、宣称已修复，或推断所有负载端到端更流畅。

raw-2保留失败：60秒观察窗口完成59次成功状态采样，末样本仍streaming/connected；
随后after helper在8053.5351ms、closed helper在8030.8805ms触及各自8000ms期限，
均收到SIGTERM且`childClosed=true`。对应`owner-postflight-failed`和
`cleanupPassed=false`，不是已证实的采样evaluate超时、身份漂移或连接泄漏。
没有进入正常UI停止清帧验收，仅失败清理通过；不拿清理通过追认该组通过。

父回执保持`passed=false`。18:55:13 UTC重新核验日常`.23` PID39056路径、创建时间、
SHA一致，本地测试监听为零，PC3无Hook及测试监听，四组远端任务均Disabled。
入口为`pair-runner-receipt.json`和`failure-closeout.json`。仅首对及独立JPEG重复的
离线派生分析保存为`completed-subset-analysis.json`，明确
`nativeCrossOrderPassed=false`；未运行要求四组成功的`Summarize-Cross.mjs`。
该旧摘要脚本仍有dailyExitCount硬编码1的已知问题，后续复用前必须修正，不能将其
用于当前0退出分支的正式报告。原始输入和失败回执未覆盖。

当前不能确定helper内部哪一步耗时。恢复后的三次只读空闲测量中，CIM约436–480ms、
TCP查询约490–991ms、包hash约18–81ms；这不是故障时的阶段追踪，不排除启动、I/O或
瞬时调度压力。仅给staged Snapshot-SourceObserver新增最多9条固定阶段/相对时间
JSONL，逐条flush、独占创建，不记录路径、命令行、凭据或原始异常。不放宽8秒helper
及10秒owner期限，不改产品行为。7项owner夹具新增完整/中断轨迹、字段、释放和禁止
覆盖验证，26项Node回归通过；阶段诊断本身尚未原生复测。

四组交叉顺序仍不通过，A3-P和总Issue #67继续保持未完成。静态/滚动、目标观看端数、
GPU、跨机时钟校准、严格画质及物理呈现等边界均未由本轮补齐。

### 只读跨机时钟偏差探测（非媒体窗口校准）

19:04:58–19:05:01 UTC，在日常程序运行时通过现有PC3管理入口执行7次固定时间读取，
没有启动原生测试或修改系统时钟/网络。本地wall读时各自用monotonic前后界包围，
核验时计入实测读时边界开销；固定响应只含主机名和UTC毫秒。工具位于
`linshi/issue67-a3p-source-runner-20261007`，17项离线正负控通过。

每次远端读时位于本地请求起止之间，因此给出remote-minus-local区间，不假设延迟
对称。第二次真实探测通过；7个区间在恒定偏差假设下的交集为[-5273,-4988]ms，
即PC3时钟在这些探测时刻约落后本地5秒。证据为`clock-offset-probe-20261007-2.json`。
初次探测因工具把wall读时边界外开销纳入单一monotonic间隔而拒绝，失败报告保留，
不是已证实的系统时钟跳变；没有调宽5ms容差，而是补测真实边界。

这些数据不能追溯校准之前的raw/JPEG试验，更不证明未来或整段媒体窗口时钟稳定。
285ms交集宽度仍不足以认证几十毫秒延迟差；中点不是无误差真值。当前工具明确
`clockStabilityProven=false`、`endToEndFrameAgeMeasured=false`，没有将探测结果
伪装成端到端帧龄。后续需要在真实测量窗口绑定更窄误差及稳定性证据，A3-P仍未完成。

随后改用持续SSH会话，使远端PowerShell启动在每组读时窗口之外；固定9次只读语句
逐次关联响应，不增加转发/监听，不改变系统时钟或日常Hook。两次成功探测的恒定
偏差交集分别为[-5214,-5196]ms（18ms宽）与[-5220,-5211]ms（9ms宽）。这两个
独立窗口不能相互取交集来假定跨窗口稳定，也不能追溯套用旧媒体帧；两次数据均保留。
协议实现、回归及边界见staged目录README，结果为persistent-clock-analysis-1.json
和persistent-clock-validation.json。

初期Console.ReadLine输入方案两次停滞失败，未确定底层原因；第一份远端辅助进程
按PID/path/精确创建时间清理并独立核验不存在，第二份由新增12秒远端watchdog退出。
当前以PowerShell宿主单一输入owner运行后成功，正常exit0及开放stdin等待watchdog
exit7的真实负控均通过，随后独立查询远端PID消失，日常`.23`身份未变。watchdog
安装前的异常及本地kill后的独立总等待上限仍是工具边界，不把正常退出证明扩大到
任意异常恢复保证。未启动媒体，没有新增A3-P完整帧龄或物理呈现验收结论。

### 源worker四阶段生产测量接线（尚未打包原生验证）

Hook新增固定大小的sourceTiming会话累计统计，接入source worker的循环入口socket
service、latest frame锁/clone、representation适配及binary socket.send，并通过原
get_live_relay_status快照提供。每阶段记录attempts/成功/失败/empty、总/最大微秒；
不存帧历史或错误正文，不改预算、帧率、sleep、队列和重连策略。它不是capture侧JPEG
编码、GPU/readback或远端ACK。完整字段语义和失败边界见Hook的LIVE_RELAY_DIAGNOSTICS.md。

4项新Rust回归及既有真实loopback WebSocket源worker测试验证固定统计、返回值保持、
早退合并和源发送计数接线；最终live_relay聚焦为38 passed、4 ignored。真实WGC等
ignored入口未执行。staged只读source采样投影新增字段，28项Node回归通过，额外敏感
字段不进入报告，旧包缺失指标不被伪装为0。Hook formatter及有效行数ratchet通过。

证据根为linshi/issue67-source-stage-timing-20261007。新接线尚未编译成独立候选包、
未运行双机原生测试；原`.25`不包含此代码。只能报告测量实现与软件回归通过，不能
据此认定低速根因、整体收益或A3-P完成。capture/readback/JPEG及预算拒绝测点仍缺。

随后补充Hook captureTiming三阶段生产接线：mailbox交接（复合范围）、实际JPEG编码、
帧入队。与sourceTiming共享固定统计值而不共享会话状态，不改raw/permit释放、预算
或节流。capture聚焦25 passed/2 ignored、relay38 passed/4 ignored、预算10 passed及
staged29项Node回归通过，行数/格式门禁通过。详细字段边界见Hook诊断文档；证据同根
capture-receipt.json。尚缺独立readback/预算拒绝分类及新包原生验收，不能把handoff
合计耗时解释成GPU耗时，也不能使用`.25`旧包认证新测量。

随后补充每个CaptureBudget实例的cpuAdmission三分类累计：granted、policyDenied、
lockUnavailable（含竞争及poisoned）。handoff返回后取非整体原子的快照，合并时覆盖
而非重复累加；不改变准入判定、公平性或permit释放，但既有锁内新增计数有少量开销。
预算12 passed、capture26 passed/2 ignored、relay38 passed/4 ignored、staged30项
Node回归通过。独立readback、新候选打包及真实双机仍未验证，A3-P不因此完成。
边界详见Hook诊断文档，同证据根新增admission-receipt.json；不追认旧试验已具备这些测点。

随后补充消费端readback.stagingCopy/mapRgb，两条mailbox/fallback路径均接入；前者
为staging分配及GPU copy提交的CPU调用，后者含Map等待、转换及Unmap，均不是GPU完成
时间。它们属于handoff内部，不可重复加总；capture callback copy与WGC获取仍不在内。
按职责提取worker_fallback模块，保持锁/permit生命周期。最终capture28 passed/2 ignored、
live_gpu28 passed/6 ignored、relay38 passed/4 ignored、staged31项Node回归及行数/格式
门禁通过。新增原生fallback断言尚未运行；未打新候选，未进行两机复验，不关闭A3-P。

### v0.2.32.26 四组真实双机及阶段计时验证

2026-10-07随后构建独立内部`.26`候选，Hook SHA-256为
`4a4119bb1b08c48f5d16cbe497d1be505b9cd5670ab88e263b00ab1ecea49024`。
构建/源码哈希及headless已核验；dirty候选不作为正式Release。真实试验证据根：
`linshi/issue67-a3p-v26-20261007-r3`。同一包、单观看端、motion、每组30秒warmup和
60秒采样，raw-1→jpeg-1→jpeg-2→raw-2四组均通过正常停止清帧及清理。新增九阶段与
CPU admission在每组原生源端快照中存在且推进；不是沿用旧包兼容读取作为通过。

| 组 | daemon published/s | source sent/s | 离散采样绘制帧数 |
| --- | ---: | ---: | ---: |
| raw-1 | 20.657701 | 20.643578 | 198 |
| jpeg-1 | 20.253080 | 20.232282 | 194 |
| jpeg-2 | 20.602536 | 20.589853 | 178 |
| raw-2 | 20.458388 | 20.501040 | 190 |

两种顺序的bytes/write下降分别为96.530723%和96.530698%。各组240次presentation
采样均为658×407；初始发布声明439×271，两者分开保留，不能声称已核全部媒体帧尺寸
或解释尺寸变化原因。以上速率不是物理显示FPS，绘制帧数也不是全帧吞吐或漏帧率。

按源端累计差值除以各自attempts，raw adaptation均值5.87/6.03ms，JPEG为0.089/0.081ms；
raw socketSend为4.97/4.83ms，JPEG为0.240/0.232ms。四组mapRgb为8.91–9.69ms，
capture JPEG编码为1.59–1.81ms。handoff含readback，不能重复加总；这些不是跨机帧龄、
GPU执行时间或p95，也不能单据均值认定瓶颈/因果。本轮未重现历史JPEG前段约8/s，
但不追认历史失败为通过，不宣称已修复该现象或提升端到端流畅度。

第一次装配漏父目录退出helper，第二次在退出前检测到FlClashCore占用49873，均在
实际退出前停止，失败目录保留。r3仅将测试daemon loopback端口改为47873；媒体端口
及系统网络/驱动未改变，无关进程未停止。日常`.23`正常退出一次，20:28:07 UTC恢复，
20:29:33 UTC fresh closeout核PID33424/path/开始时间/SHA通过，本地测试监听0，PC3
Hook及测试监听0，四个测试任务Disabled。可见/关闭截图抽查符合原生观看与停止清帧。

本轮只通过单观看端motion两对交叉顺序及新测点验收；static/scroll、多观看端、质量
匹配、跨机时钟/帧龄、GPU及物理呈现等完整性能条件仍未证明，A3-P和Issue67保持未完成。

#### 同轮CPU与资源端点补算

对上述四组已有before/after原始资源快照复用PairResourceSummary进行离线补算，
未再次退出程序或重跑媒体。核两端包绑定、PID/创建时间、零CDP连接、无缺失指标、
无重复进程身份；两端端点之间进程集合均相同。helper三项聚焦回归通过，输入SHA
绑定在同根`resource-endpoint-summary.json`。同名PID不足以匹配，使用PID/创建时间/组。

CPU百分比为CPU秒差/本机Stopwatch间隔/逻辑核数×100，源机12核，观看机16核。
它是整个主机算力口径下该进程组的区间平均，不是整机CPU、单核占比或峰值。

| 组 | 源端Hook树CPU% | 观看端Hook树CPU% | daemon CPU% | 源端/观看端Private MiB差 |
| --- | ---: | ---: | ---: | ---: |
| raw-1 | 3.369 | 3.410 | 0.565 | -6.328 / -60.535 |
| jpeg-1 | 2.644 | 2.085 | 0.281 | -3.086 / +2.090 |
| jpeg-2 | 2.608 | 2.031 | 0.239 | -5.840 / +3.621 |
| raw-2 | 3.611 | 3.844 | 0.572 | -5.375 / -71.859 |

各CPU端点窗口为79.876–80.618秒，包含采样启动/退出开销，不与60秒daemon或源端
阶段差分窗口拼成精确全链路归因。每个进程的读取也非原子；端点集合相同不证明期间
没有短命进程，数据不是全进程历史。fixture单列而非算入Hook，GPU、Caddy和整机负载
不在该CPU表中。Private差为端点差，不是峰值或泄漏判定，不替代A3-R长稳。

#### B1/B2/C1/D1当前条件项决策

这些是依据当前单观看端motion证据的阶段决策，不代替缺失的A3矩阵或承诺永久不做。

- **B1：当前不调整呈现调度/IPC预算。** 当前没有同窗口轮询/解码/合成/物理帧龄的
  因果对照，发送约20/s和离散绘制计数不能证明80ms观看预算是主要瓶颈。保留单在途、
  latest-frame及取消约束；待观看阶段和帧龄证据支持后才做单变量试验。
- **B2：当前不新增分发/兼容转换优化。** 现有JPEG直通在本轮减少bytes/write，源端
  adaptation/send及CPU端点均较低；这支持保留既有协商与raw回退，不证明daemon是
  剩余主要瓶颈。多原生观看端、慢端及并行负载证据不足，不重构分发或放松队列界限。
- **C1：当前不引入GPU视频编码POC。** capture JPEG均值约1.6–1.8ms，尚未证明A/B
  不足且GPU编码整体收益成立；readback约9ms也不等于视频编码能消除该成本。完整
  协商、decoder、关键帧、late join、fallback及许可工作不因一个枚举被视为完成。
- **D1：本轮不启动屏幕墙缓存对照。** 当前是LiveRelay单观看端，不是屏幕墙1/2/4
  同/异profile负载；保留已有PNG共享缓存，不用此处JPEG结论替代墙缓存性能证据。

后续若新增场景触发上述条件，重开对应单变量决策；当前没有为优化而新增生产策略。

#### 静态readback独立硬件回归

在不退出日常Hook的情况下，执行现有ignored原生测试
`live_gpu::snapshot::native_tests::static_staging_copy_progresses_without_other_gpu_work`
（`cargo test --locked --lib --manifest-path src-tauri/Cargo.toml <test> -- --exact --ignored --test-threads=1`）。
实际1 passed，0.33秒；独立D3D11硬件设备、32×16初始化纹理，连续8次staging copy/Map
均在现有期限内完成，RGB像素、尺寸和时间戳一致，不依赖WGC/Present或其他context
生产者提交。没有新窗口、输入或驱动重置；测试进程退出后已核不存在，日常`.23`
PID33424/path/开始时间/SHA重新核验。源码和测试EXE哈希见
`linshi/issue67-static-readback-check-20261007/verified-receipt.json`。

这是debug原生测试EXE的真实硬件纹理回归，不是`.26`发行EXE业务验收、WGC静态/滚动
双机对照或device-loss证明。静态/滚动模板已另准备，尚待新的正常退出许可后运行；
不因本项通过关闭A3-P/A3-L，也不修改生产Map策略。

#### 静态/滚动首轮被容量门禁拦截（2026-10-08 UTC）

用户另行确认正常退出一次并恢复后，装配`.26`静态raw/JPEG及滚动JPEG/raw四组。
新根`linshi/issue67-a3p-static-scroll-v26-20261007`的候选传输与授权预检通过，
日常正常退出一次。raw-1在本机源端启动前触发15%内存余量门禁：子runner表面报
`source-direct\\owned.json readiness timeout`，实际source-launcher stderr明确为
`Host memory headroom below 15-percent native-start gate`。未创建源端Hook、未进入
内容捕获或性能采样，后三组未启动，不能据此评价静态/滚动产品行为。

本机快照余量从preflight20.310%降至services-ready14.489%、pc3-viewer-ready11.980%；
最终独立closeout为7.735%。这些时点不足以证明某个进程或服务导致变化，不停止
无关负载、不降低门槛。双机失败清理通过，日常`.23`于00:11:38 UTC恢复PID38724，
00:12:32 UTC再次核path/创建时间/SHA，本地测试监听0、PC3 Hook/测试监听0，唯一
已创建raw-1任务Disabled。原始失败及`failure-closeout.json`保留，未自动重跑。

随后只改staged测试工具：内存快照保存后可选择执行同一15%门禁，子runner在
services-ready等4个节点显式启用，避免已知不足后继续启动后续阶段或只等ready超时。
14.99%拒绝、15%通过、20%通过和纯观察低余量不拒绝四项边界回归通过，并核四处接线。
首轮测试夹具变量作用域错误已修正，失败夹具证据保留。该工具修正未原生复验，
没有修改Hook或Loom生产策略；再次运行仍需足够容量与新的正常退出许可。
