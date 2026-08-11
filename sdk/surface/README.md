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

## 安全边界

- `fetch`、`XMLHttpRequest`、`WebSocket`、`EventSource` 和 `sendBeacon` 不可用于网络访问。
- 不提供 Node API、Tauri IPC、文件系统、宿主 DOM 或原生插件。
- 每个 attachment 使用隔离 iframe、CSP、MessageChannel token、心跳、事件速率、计时器与 DOM 节点预算。
- `resources` 与 `NeuroSurface.resource()` 只返回 Host 已校验并已租用资源的本地 URL。
- 业务事件必须通过 `emit()` 上报 Loom；正式结果、权威状态和权限确认由 Loom/Host 提交。

使用 `loom-plugin validate <ART_DIR>` 校验 Surface manifest、声明式场景、动作引用、高风险确认和包内入口，再使用 `loom-plugin pack <ART_DIR> <OUTPUT_ZIP>` 生成确定性包。
