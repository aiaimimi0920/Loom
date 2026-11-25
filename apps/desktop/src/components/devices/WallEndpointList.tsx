import type { WallState } from "../../services/loomApi/wallTypes.ts";
import { Badge, Button, EmptyState, Panel } from "@neuro/ui";
import { wallOutputStatus } from "./WallPresentationControls.tsx";

export function WallEndpointList({
  state,
  disabled,
  identify,
}: {
  state: WallState;
  disabled: boolean;
  identify: (endpointId: string) => Promise<void>;
}) {
  return (
    <section aria-label="终端输出">
      <h3>已注册输出 · {state.endpoints.length}</h3>
      {!state.endpoints.length && <EmptyState title="暂无已注册输出" />}
      <div className="wall-endpoints">
        {state.endpoints.map((status) => {
          const { endpoint, online, identification } = status;
          const wall = state.layouts.find((layout) =>
            layout.tiles.some((tile) => tile.endpointId === endpoint.endpointId),
          );
          const control = state.presentations?.find((p) => p.wallId === wall?.wallId);
          return (
            <Panel className="wall-endpoint" key={endpoint.endpointId} data-endpoint-id={endpoint.endpointId}>
              <strong>{endpoint.display?.name ?? endpoint.outputId}</strong>
              <div className="wall-endpoint-status">
                <Badge tone={online ? "success" : "neutral"}>{online ? "在线" : "离线"}</Badge>
                <span>
                  {endpoint.pixelSize.width} × {endpoint.pixelSize.height}
                </span>
              </div>
              <span className="n-muted">
                {endpoint.deviceId} / {endpoint.outputId}
              </span>
              <span>
                显示：{endpoint.renderModes.join(" / ")}；输入：{endpoint.inputCapabilities.join(" / ") || "无"}
              </span>
              <span className="n-muted">
                场景调度：{endpoint.scheduledPresentation ? "支持" : "未声明"}
                {status.scene
                  ? ` · v${status.scene.revision} ${status.scene.appliedAtMs !== null ? "已提交绘制" : status.scene.prepared ? "准备就绪" : "准备中"}`
                  : ""}
                {status.scene?.clockUncertaintyMs != null ? ` · 时钟估计 ±${status.scene.clockUncertaintyMs} ms` : ""}
              </span>
              <div className="wall-endpoint-assignment">
                <Badge tone={control ? "warning" : "neutral"}>
                  {wall ? wallOutputStatus(wall, control, status) : "未分配墙面"}
                </Badge>
                {wall && <code>{wall.wallId}</code>}
              </div>
              <div className="wall-endpoint-action">
                <Button
                  aria-label={`识别屏幕 ${endpoint.display?.name ?? endpoint.outputId}`}
                  disabled={disabled || !online || !endpoint.display?.canIdentify || Boolean(control || identification)}
                  onClick={() => void identify(endpoint.endpointId)}
                >
                  识别屏幕
                </Button>
                {(identification || !endpoint.display?.canIdentify || control) && (
                  <span role="status">
                    {identification
                      ? `${identification.applied ? "正在识别" : "等待输出确认"} · ${Math.ceil(identification.remainingMs / 1000)} 秒`
                      : !endpoint.display?.canIdentify
                        ? "终端未声明识别能力"
                        : "恢复显示后可识别"}
                  </span>
                )}
              </div>
            </Panel>
          );
        })}
      </div>
    </section>
  );
}
