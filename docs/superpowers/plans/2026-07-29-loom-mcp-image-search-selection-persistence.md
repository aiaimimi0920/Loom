# Loom MCP 图片搜索结果选择持久化

## Goal

在 Phase 59 已经做出 MCP `图片搜索` 多结果选择 UI 之后，把用户选中的
结果真正写回 live Hook 工作流 / session，而不是只存在于 Loom daemon 的
runtime overlay 里。

## Scope

- 增加一个可从 desktop 调用的 `update_workflow_node` HTTP 兼容入口；
- 在点击候选结果时先持久化 `result_index`，再重新执行节点；
- 成功执行后把候选列表、当前选中索引、以及预览图一起写回 live
  Hook/session 形态；
- Hook canvas 重新加载时能从持久化数据恢复 `selectedResultIndex`、
  `resultCandidates` 和预览。

## Done definition

- Loom 点击 `图片搜索` 候选结果后会先写入 `result_index`；
- 清空 daemon runtime 状态后，Loom 仍能重新读到已选中的结果；
- Hook canvas 解析层能从持久化 live 数据恢复选择状态；
- Rust、desktop 测试、以及 repo-owned fake store smoke 通过。
