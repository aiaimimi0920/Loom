import type { WallState } from "../../services/loomApi/wallTypes.ts";
import { wallOutputStatus } from "./WallPresentationControls.tsx";

export function WallEndpointList({ state, disabled, identify }: {
  state: WallState; disabled: boolean; identify: (endpointId: string) => Promise<void>;
}) {
  return <section aria-label="终端输出"><h3>已注册输出 · {state.endpoints.length}</h3>
    {!state.endpoints.length && <p className="wall-muted">尚无瓷砖终端注册。请在 Hook 终端选择屏幕，并在设备页批准配对。</p>}
    <div className="wall-endpoints">{state.endpoints.map((status) => {
      const { endpoint, online, identification } = status;
      const wall = state.layouts.find((layout) => layout.tiles.some((tile) => tile.endpointId === endpoint.endpointId));
      const control = state.presentations?.find((p) => p.wallId === wall?.wallId);
      return <div className="wall-endpoint" key={endpoint.endpointId} data-endpoint-id={endpoint.endpointId}>
        <strong>{endpoint.display?.name ?? endpoint.outputId}</strong>
        <span className={online ? "wall-online" : "wall-muted"}>{online ? "在线" : "离线"} · {endpoint.pixelSize.width} × {endpoint.pixelSize.height}</span>
        <span className="wall-muted">{endpoint.deviceId} / {endpoint.outputId}</span>
        <span>显示：{endpoint.renderModes.join(" / ")}；输入：{endpoint.inputCapabilities.join(" / ") || "无"}</span>
        <span className="wall-muted">场景调度：{endpoint.scheduledPresentation ? "支持" : "未声明"}
          {status.scene ? ` · v${status.scene.revision} ${status.scene.appliedAtMs !== null ? "已提交绘制" : status.scene.prepared ? "准备就绪" : "准备中"}` : ""}
          {status.scene?.clockUncertaintyMs != null ? ` · 时钟估计 ±${status.scene.clockUncertaintyMs} ms` : ""}</span>
        <span className={control ? "wall-warning" : undefined}>{wall ? `墙面 ${wall.wallId} · ${wallOutputStatus(wall, control, status)}` : "尚未分配墙面"}</span>
        <div className="wall-endpoint-action">
          <button type="button" className="ghost-button" disabled={disabled || !online || !endpoint.display?.canIdentify || Boolean(control || identification)}
            onClick={() => void identify(endpoint.endpointId)}>识别屏幕</button>
          <span role="status" className={identification?.applied ? "wall-online" : "wall-muted"}>
            {identification ? `${identification.applied ? "正在识别" : "等待输出确认"} · ${Math.ceil(identification.remainingMs / 1000)} 秒`
              : !endpoint.display?.canIdentify ? "终端未声明识别能力" : control ? "恢复显示后可识别" : "标识最多显示 10 秒"}
          </span>
        </div>
      </div>;
    })}</div>
  </section>;
}
