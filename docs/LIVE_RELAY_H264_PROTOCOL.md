# C1：H.264 连续帧合同

本文件定义 C1 的中继数据边界，跟踪 [#91](https://github.com/aiaimimi0920/Loom/issues/91)。
当前已实现 payload 校验、存储/游标连续性及 Loom 端显式协商；Hook 产品尚未 offer 新 profile。
已有 `loom.live.v1`、`loom.live.jpeg.v1` 明确拒绝 codec 2，不会将其当图片发送给旧端。

## Wire 与资源边界

- 沿用 NLLV v1 的 64 字节头，codec 2；保留字节继续必须全零。
- 每条消息携带一个 Annex-B access unit，支持三/四字节起始码；不接受 AVCC。
- 负载至多 1 MiB、至多 256 个 NAL；禁止 forbidden bit、空 NAL 和不支持的类型。
- 基线输入为 SDR，wire `srgb` 指原始画面色彩；H.264 编码器使用 BT.709 limited-range
  YUV。尺寸必须为偶数、每边 2..4096、总像素不超过 8,294,400，不支持 HDR10。
- keyframe 位当且仅当该 AU 含 IDR；SPS/PPS 必须在 IDR 之前，不能与 delta picture 混合。
  delta AU 不得重新携带参数集。此检查不是完整 SPS/slice parser 或像素解码证明；
  接收 decoder 仍必须限制实际协商输出尺寸及分配，不能仅信任 wire 的 width/height。

## 连续性与恢复

- 源端首个 H.264 AU、JPEG/raw 切入 H.264、尺寸变化、源 frame ID gap 都要求新的
  SPS/PPS/IDR；非法 delta 在修改 session 计数、游标或缓存前拒绝。
- 编码后的 AU ID 必须连续；丢弃未编码原始输入不应造成编码 AU ID gap。
- 缓存仍为既有的 2..3 帧，不扩大为无界 GOP 存储。
- JPEG/raw 仍返回最新帧。H.264 返回观看游标的下一连续 AU，不能跳到最新 delta。
- 新观看者、epoch 变化或缓存淘汰导致游标落后时，只能从保留的新 IDR 恢复；没有 IDR
  则等待。条件变量使用同一“可投递帧”谓词，不因存在不可解码的 delta 而忙循环。
- codec/尺寸变化及带 IDR 的源 gap 会清理旧表示。授权、epoch、单调游标及终止检查不变。

## 推进与兼容顺序

1. 本增量先部署被动校验和连续性规则；正常 JPEG/raw 读写无需迁移，不改保存数据。
2. Loom 以显式 `loom.live.h264.v1` 选择视频 profile，携带 H.264 或 JPEG/raw 回退；
   旧 profile 选择逻辑不变。Hook 必须在 encoder/decoder 产品 owner 就绪后才 offer 它。
3. Hook decoder 在本地 latest-image 缓存之前按顺序消费 AU，只允许丢已解码图像；
   重连、epoch、权限撤销、取消和尺寸变化需重置 decoder 并等待独立 IDR。
4. 完成一源一收、late join、overflow 恢复、旧端回退与停止后再关闭 C1。

回退版本仍使用既有 JPEG/raw profile；不存在数据库迁移或破坏性数据回滚。新客户端面对
旧 Loom 必须选择旧 profile 并禁用 H.264，而不是将 codec 2 塞入旧连接。

## 连接级协商与控制

- 每个标准 viewer socket 和墙 socket 都持有独立、有界 RAII 媒体需求。墙只登记图片
  需求，不增加 Surface membership、controller 或输入权限。同设备的多个 socket 独立计数。
- 仅在至少一个消费者且全部消费者声明支持 H.264 时，源端收到 `h264_allowed=true`。
  无观看者、旧端、墙或明确请求 fallback 的连接均使策略为 false。
- 新 profile 的源端接收以下 Text；字段使用 snake_case，身份仍来自 WebSocket grant：

  ```json
  {"type":"video_policy","epoch":1,"h264_allowed":true,"keyframe_sequence":1}
  ```

  `h264_allowed` 是可用性而非强制选择；硬件失败时仍可发送 JPEG/raw。首次启用、重新启用、
  epoch 变化及 `keyframe_sequence` 更新均要求源端输出独立 SPS/PPS/IDR。
- 仅新 profile 的 viewer 可发送 `{"type":"keyframe_request","epoch":1}` 或
  `{"type":"video_fallback","epoch":1}`。每条控制至多 256 字节，拒绝未知字段/类型、
  旧 epoch 和未授权请求。fallback 对当前 socket 不可逆；重新建连才重新声明 decoder 能力。
- 所有关键帧请求聚合为一个 pending 位，通知序号至多每 250 ms 增加一次；限频延迟通知，
  不丢弃恢复需求。源循环最长一次 250 ms read deadline 后刷新策略。
- 入库准入和消费者需求共用 session 锁。图片回退期间的在途 H.264，以及恢复前的 delta，
  被丢弃而不关闭源；恢复必须收到新 IDR。非法 wire、旧 epoch/序号仍按原规则拒绝。
- 旧端和墙在 H.264 缓存尚未被图片替换时等待，不透传 codec 2、不将它当 JPEG/raw 解码。
  兼容需求退出、权限撤销或 worker 退出时释放 lease；store-wide lease ID 防止旧 socket
  操作另一连接的能力记录。
- 新视频观看连接不继承旧媒体游标的 decoder 参考链。缺少可投递 IDR/连续帧时自动请求
  恢复；没有新帧的正常 idle 不触发关键帧请求。

## 验证边界

protocol 测试覆盖语法、预算与 keyframe 标志；daemon 测试覆盖源 gap/尺寸变化的原子拒绝、
按序游标、overflow 等待 IDR、表示切换及旧 profile 拒绝。真实 WebSocket 测试确认两个旧
profile 都在入库前关闭 H.264 源连接。协商回归覆盖真实 socket 的视频/图片切换、旧端加入、
sticky fallback、stale control 和墙在 H.264 后等待图片；状态测试覆盖短暂兼容需求导致
在途 AU 丢失后的 IDR 恢复，以及 pending 限频和 lease 清理。
语法夹具不冒充可解码视频；真实硬件像素证据由 Hook 的 WGC 编码测试提供，Hook 产品源端
控制接线、网络 decoder/呈现和配套独立 release 验收仍待完成。
