# Surface 一期验收原型

该目录包含三个独立、可安装的 `loom.surface.v1` Art 包：

- `stock-card`：文本、按钮、输入、持续事件、提交事件和流式价格 Patch。
- `dashboard`：列表、进度、Host 托管资源更新，以及同一实例的多 attachment。
- `form`：必填与可选参数、错误、Host 高风险确认、正式提交和可取消执行。

每个包都包含自己的 `manifest.json`、`art.runtime.json`、声明式场景和 PowerShell process runtime。运行时只能响应该包清单声明的动作。仪表板图片通过 `resourceUploads` 交给 Loom 注册，运行时不得伪造资源摘要或租约。

构建：

```powershell
.\scripts\build-surface-prototypes.ps1
```

脚本先调用 `loom-plugin validate`，再生成确定性 ZIP、SHA-256 文件和 `surface-prototypes.catalog.json`。
