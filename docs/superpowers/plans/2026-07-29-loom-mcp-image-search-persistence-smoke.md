# Loom MCP 图片搜索选择持久化 smoke

## Goal

把 Phase 60 已经完成的 `图片搜索` 结果选择持久化，从“daemon/desktop 测试 +
repo fake store smoke”推进到“正式 release smoke 证据”。

## Scope

- 扩展 fake MCP / cloud fixture，使其能返回两张不同图片；
- 在 fake store Hook smoke 中执行第二个搜索结果；
- 清掉 Hook Bridge runtime 状态后重新读取 Hook canvas 和 preview；
- 验证 `selectedResultIndex`、`resultCandidates`、`params.result_index` 和
  preview 都能恢复。

## Done definition

- fake store smoke 能证明第二张结果被选中并在 runtime clear 后恢复；
- `verify-release.ps1 -RunSmoke` 通过；
- 生成新的 Loom release 并记录 phase 文档。
