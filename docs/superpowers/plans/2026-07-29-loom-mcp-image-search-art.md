# Loom MCP 图片搜索 Art 任务

## Goal

把 Loom 的通用 MCP 可安装框架，推进到真正可用于 `图片搜索` Art node 的状态，
而不是只停留在 `echo` 文本工具能跑通。

## Scope

- 先补一条 Loom 内部任务记录，明确这不是已有 Phase 45-47 的重复。
- 用 `brave_image_search` 形状的 MCP Art 作为真实验证对象。
- 当 MCP 返回结构化图片搜索结果 URL 时，Loom 需要把首张图片抓回并转成
  Hook/Loom 现有可预览的 base64 图片输出。
- 更新 repo-owned fake art-store smoke，让 MCP 框架证明点从“文本 echo”
  升级成“图片搜索输出图片”。
- 补一个 repo-owned 安装脚本，便于本地把 `图片搜索` Art 安装到 Loom。

## Done definition

- `ToolExecution::Mcp` 在图片输出场景下，能把结构化搜索结果适配成图片内容。
- daemon Hook bridge 能让 `图片搜索` Art node 返回 `output_base64`。
- `Invoke-LoomFrameworkArtStoreHookSmoke.ps1` 使用 `图片搜索` Art 并通过。
- 新安装脚本能生成/发布 repo-owned `图片搜索` Art 包。
