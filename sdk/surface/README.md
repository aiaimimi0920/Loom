# Neuro Surface SDK 1.0

`neuro-surface.d.ts` 描述 Hook WebView2 沙箱当前实际提供的 JavaScript/TypeScript Surface API。TypeScript 仅用于开发与打包，Art 包内必须放编译后的 JavaScript；Hook 不引入 Node、TypeScript、Python、Deno、QuickJS 或通用 WASM 运行时。

## 最小入口

```ts
/// <reference path="./neuro-surface.d.ts" />

NeuroSurface.define({
    mount({ root, snapshot, emit }) {
        const button = document.createElement("button");
        button.textContent = String(snapshot.authoritativeState ?? "刷新");
        button.addEventListener("click", () => {
            emit({
                nodeId: "refresh",
                event: "click",
                action: "refresh",
                class: "discrete",
                payload: {},
            });
        });
        root.replaceChildren(button);
        return () => button.remove();
    },
    update({ snapshot }) {
        // 使用新的 Snapshot 更新已有 DOM，不在终端执行业务逻辑。
        void snapshot;
    },
});
```

## Art 多视图与完整尺寸

Surface Art 可以在 `metadata.capabilities.surface` 中声明多个面向用户的视图。每个视图必须有唯一、安全的 `id`、非空 `label` 和自己的 `fullSize`；只要声明了 `views`，就必须同时声明一个指向其中某个视图的 `defaultViewId`：

```json
{
  "protocolVersion": "loom.surface.v1",
  "apiVersion": "1.0",
  "variants": [{ "runtime": "javascript", "entry": "surface/main.js" }],
  "views": [
    { "id": "full", "label": "全视图", "fullSize": { "width": 960, "height": 820 } },
    { "id": "price", "label": "交易价格视图", "fullSize": { "width": 620, "height": 620 } }
  ],
  "defaultViewId": "full"
}
```

- 新建 Art 节点时，Hook 使用默认视图的 `fullSize`，并把当前视图作为 `snapshot.viewId` 传给 Surface。
- 用户通过 `Ctrl+E` 打开 Art 编辑栏并切换视图；切换后节点恢复到目标视图声明的完整尺寸。
- 用户通过 `Ctrl+鼠标滚轮` 对节点做等比例缩放。Surface 始终在声明的逻辑完整尺寸内排版，再由 Hook 统一缩放整个内容，因此文字、图形和交互坐标会一起缩放。
- Surface 应按每个完整尺寸完成布局，不应依赖节点根滚动条才能显示该视图的目标内容。
- JSON Schema 能要求 `views` 与 `defaultViewId` 同时出现；`loom-plugin validate` 还会强制检查视图 ID 唯一且默认 ID 确实属于 `views`。

## 安全边界

- `fetch`、`XMLHttpRequest`、`WebSocket`、`EventSource` 和 `sendBeacon` 不可用于网络访问。
- 不提供 Node API、Tauri IPC、文件系统、宿主 DOM 或原生插件。
- 每个 attachment 使用隔离 iframe、CSP、MessageChannel token、心跳、事件速率、计时器与 DOM 节点预算。
- `resources` 与 `NeuroSurface.resource()` 只返回 Host 已校验并已租用资源的本地 URL。
- 业务事件必须通过 `emit()` 上报 Loom；正式结果、权威状态和权限确认由 Loom/Host 提交。

使用 `loom-plugin validate <ART_DIR>` 校验 Surface manifest、声明式场景、动作引用、高风险确认和包内入口，再使用 `loom-plugin pack <ART_DIR> <OUTPUT_ZIP>` 生成确定性包。
