# C1：H.264 连续帧合同

本文件定义 C1 的中继数据边界，跟踪 [#91](https://github.com/aiaimimi0920/Loom/issues/91)。
当前只完成 payload 校验和存储/游标连续性；尚未对产品连接开放 H.264 协商。
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
2. 后续增加显式 H.264 profile、经过授权的关键帧/回退通知，以及旧观看端加入时的源端
   JPEG/raw 回退；未完成这些部分前不能自动开放 H.264 源。
3. Hook decoder 在本地 latest-image 缓存之前按顺序消费 AU，只允许丢已解码图像；
   重连、epoch、权限撤销、取消和尺寸变化需重置 decoder 并等待独立 IDR。
4. 完成一源一收、late join、overflow 恢复、旧端回退与停止后再关闭 C1。

回退版本仍使用既有 JPEG/raw profile；不存在数据库迁移或破坏性数据回滚。新客户端面对
旧 Loom 必须选择旧 profile 并禁用 H.264，而不是将 codec 2 塞入旧连接。

## 验证边界

protocol 测试覆盖语法、预算与 keyframe 标志；daemon 测试覆盖源 gap/尺寸变化的原子拒绝、
按序游标、overflow 等待 IDR、表示切换及旧 profile 拒绝。真实 WebSocket 测试确认两个旧
profile 都在入库前关闭 H.264 源连接。语法夹具不冒充可解码视频；真实硬件像素证据由
Hook 的 WGC 编码测试提供，网络 decoder/呈现验收仍待完成。
