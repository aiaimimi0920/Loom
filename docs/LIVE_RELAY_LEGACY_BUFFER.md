# LiveRelay 旧端兼容转换：最终缓冲内解码

## 本轮范围

用户在 Hook IPC 优化交付后明确选择 B2：检查 Loom 帧分发与兼容转换，只修改有代码
证据的问题，不恢复 A3-P 性能矩阵，不新增 GPU 编码能力。关联剩余任务 Issue #91。

检查发现，JPEG 兼容旧端时原先经过 `DynamicImage -> RGBA -> BGRA`，随后为协议头
扩容并将全部像素后移 64 字节。新实现预先分配 `64 + width * height * 4` 的最终帧，
使用现有 `ImageDecoder::read_image` 写入其 payload 前部，再反向原地扩展为 BGRA。
协议头从原帧复制，只更新 payload 长度和 codec；不改变帧身份、时间戳及原始 JPEG。

当前锁定 image JPEG 解码器支持 L8、La8、Rgb8、Rgba8 输出，其他 JPEG 颜色空间由
该解码器转换为 RGB。四种输出均保留原颜色与 alpha 语义，未来未知类型显式拒绝。
反向处理先读取完整源像素，再写目标像素，避免紧凑像素在扩展时被覆盖。没有 unsafe。

RGB/灰度路径不再需要独立的解码像素数组与 RGBA 数组；也不再执行插入协议头的
`reserve_exact` / `copy_within`。这是本层分配和搬运步骤的减少，不是端到端零拷贝，
不包含解码库内部工作内存，不声称实际 RSS、CPU、FPS 或延迟改善比例。

## 保持原样的分发边界

- 环形缓存仍为 2–3 帧，观看端仍选最新帧，不改为无界队列。
- JPEG 直通仍共享不可变 `Arc<Vec<u8>>`，不等待旧端转换锁。
- 同一帧的旧端结果/错误仍只缓存一份；转换并发上限仍为两个。
- 转换不持会话锁，发送前仍复核取消、凭据与 epoch/成员权限。
- 每连接独立发送与写超时保持不变，未改网络协议或鉴权。
- tungstenite 0.24 的 Binary 仍要求拥有 Vec，现有发送副本保留；本轮不为去掉该副本
  升级依赖、绕过 WebSocket 实现或引入自行维护的发送协议。
- 原有尺寸、JPEG 长度及至多 64 MiB raw payload 检查仍在最终缓冲分配之前执行。

## Piik 参考

2026-10-09 联网复核固定提交
`1b9f5bd2a27eb32d35e81e8f2c5d8d6e952bbd0a` 的
[output_mailbox.h](https://github.com/TNTcraftHIM/Piik/blob/1b9f5bd2a27eb32d35e81e8f2c5d8d6e952bbd0a/native/capture/windows/output_mailbox.h)。
参考的是不可变共享帧、pending 有界以及 generation/停止失效边界。Loom 已有对应的
latest-frame/epoch/授权保护，故不重写分发架构。本次缓冲转换独立实现，未复制 Piik
源码、引入新依赖或外推其视频编码性能。

## 聚焦验证

- 改前四项 JPEG 测试通过。
- 改后 Live 相关测试 64 通过；一项既有跨进程手工启动入口保持 ignored。
- 新增 RGB/灰度 JPEG 的四组尺寸对照，完整 wire bytes 与旧转换算法一致。
- 新增 1/2/3/4 通道的原地扩展检查，覆盖重叠区、alpha、单像素和多个像素；指针、
  capacity 与前置保护字节不变。
- 既有真实 loopback WebSocket 新旧观看端混合投送、缓存复用、JPEG 直通、撤销、
  续期、分发与屏幕墙适配回归通过。这不是双机原生 Hook 或物理呈现验收。

本轮不修改桌面 UI，不退出日常 Hook/Loom，不处理无关历史告警。完整 A3-P 仍按用户
决定跳过；后续实际使用有新问题再单独定位，不把更多优化当成本轮完成前提。
