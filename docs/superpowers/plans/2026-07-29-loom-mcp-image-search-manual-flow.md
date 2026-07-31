# Loom MCP 图片搜索手工测试流与多结果选择 UI

## Goal

在 Phase 58 已经打通 MCP 图片搜索底层结果适配后，继续补两条用户侧闭环：

1. 把 `图片搜索` 真正接到 Loom / Hook 的桌面手工测试流里；
2. 提供多结果选择 UI，而不是只能固定吃第一张图。

## Scope

- MCP 页面增加 repo-owned 的图片搜索手工测试入口；
- 工作流 / Hook 画布中允许对无上游输入的 `图片搜索` 节点直接执行；
- MCP 图片搜索结果保留候选列表和当前选中索引；
- Hook 画布节点检查器展示候选列表，并允许点击切换结果后重新执行。

## Done definition

- 用户能在 Loom 桌面页保存 Brave Search MCP 服务并注册 `custom-image-search`；
- `hook-live` / Hook 画布选中 `图片搜索` 节点后可以手工执行；
- Loom 画布能显示多张候选结果，并切换到指定结果；
- 相关 Rust / desktop 测试和 repo-owned smoke 通过。
