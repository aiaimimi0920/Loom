import type { WallEndpointStatus, WallLayout, WallPresentation, WallPresentationMode, WallState } from "../../services/loomApi/wallTypes.ts";

const labels = { running: "运行", frozen: "冻结", black: "黑场" } as const;

export function wallOutputStatus(layout: WallLayout, control: WallPresentation | undefined, endpoint: WallEndpointStatus): string {
  if (!control) return endpoint.appliedRevision === layout.revision ? `已应用 v${layout.revision}` : "等待应用";
  const report = endpoint.presentation;
  if (!endpoint.online || report?.revision !== control.revision) return `${labels[control.mode]}待确认`;
  return report.outcome === "applied" ? `已${labels[control.mode]} · 控制 v${report.revision}` : "无完整保留帧，保持黑场";
}

export function WallPresentationControls({ state, wallId, disabled, onMode }: {
  state: WallState;
  wallId: string;
  disabled: boolean;
  onMode: (wallId: string, mode: WallPresentationMode) => Promise<void>;
}) {
  const layout = state.layouts.find((layout) => layout.wallId === wallId);
  if (!layout) return null;
  const control = state.presentations?.find((control) => control.wallId === wallId);
  const mode = control?.mode ?? "running";
  const endpoints = state.endpoints.filter((endpoint) => layout.tiles.some((tile) => tile.endpointId === endpoint.endpoint.endpointId));
  const applied = endpoints.filter((endpoint) => endpoint.online && (control
    ? endpoint.presentation?.revision === control.revision && endpoint.presentation.outcome === "applied"
    : endpoint.appliedRevision === layout.revision)).length;
  return <section className="wall-display" aria-label="墙面显示控制">
    <div className="wall-toolbar">
      <span role="status">显示请求：{labels[mode]} · {applied} / {layout.tiles.length} 个输出已确认</span>
      <div className="wall-toolbar">
        {(["frozen", "black", "running"] as const).map((next) => <button type="button" className="ghost-button" key={next}
          aria-pressed={mode === next} disabled={disabled || mode === next} onClick={() => void onMode(wallId, next)}>
          {next === "running" ? "恢复显示" : next === "frozen" ? "冻结显示" : "黑场"}
        </button>)}
      </div>
    </div>
    <p className="wall-muted">操作作用于已保存的墙面。冻结和黑场立即停止墙面输入，来源继续运行；终端分别应用并确认。未保存的布局草稿仍保留。</p>
  </section>;
}
