# Loom Script 图片混合 Art 任务

## Goal

新增一个真正走 `script` 框架的 repo-owned Art node，用来验证 Loom 的
`script` framework 已经可以承载实际的双图图像处理插件。

## Scope

- 新增一个“图片混合” Script Art：
  - 输入：
    - `input`：源图
    - `reference`：参考图
  - 参数：
    - `mix_ratio`：混合比例，`0..100`
  - 输出：
    - 一张 PNG 混合结果
- 使用 `PowerShell (.ps1)` 实现图像混合逻辑。
- 产出 repo-owned 安装脚本，支持把该 Art 安装进 Loom control-plane。
- 补自动化测试，至少覆盖：
  - `loom_tool_registry` 直接执行；
  - `loom-daemon` Hook Bridge `art_loom/execute_art_node` 执行。

## Acceptance

- Script Art 能正确读取两张输入图并输出混合图。
- 自动化测试能证明 `script` framework 的实际图像处理链路可用。
- 任务完成后构建一个新的 Loom release 包，放入：
  - `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Loom`
