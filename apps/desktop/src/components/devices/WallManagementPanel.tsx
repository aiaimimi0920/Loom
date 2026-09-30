import { useState } from "react";
import { parseWallLayout } from "../../services/loomApi/wallValidation.ts";
import { requestAppConfirmation } from "../feedback/AppFeedback.tsx";
import { useWallCatalog } from "./useWallCatalog.ts";
import { WallLayoutEditor } from "./WallLayoutEditor.tsx";
import { WallPresentationControls } from "./WallPresentationControls.tsx";
import { WallEndpointList } from "./WallEndpointList.tsx";

export function WallManagementPanel({ baseUrl, online }: { baseUrl: string; online: boolean }) {
  const catalog = useWallCatalog(baseUrl, online);
  const [activationDelayMs, setActivationDelayMs] = useState(0);
  const { state, draft, inventory, busy, error, loading } = catalog;
  const conflict = Boolean(draft && state && draft.baseRevision !== state.revision);
  const schedulable = Boolean(draft && state && draft.layout.tiles.every((tile) =>
    state.endpoints.some(({ endpoint }) => endpoint.endpointId === tile.endpointId && endpoint.scheduledPresentation)));
  let invalid = "";
  if (draft) {
    try { parseWallLayout(draft.layout); } catch (error) { invalid = error instanceof Error ? error.message : "布局无效"; }
  }
  async function abandon(action: () => void) {
    if (draft?.dirty && !await requestAppConfirmation({ title: "放弃未保存的布局？",
      message: "重新载入或切换墙面会丢弃当前草稿。已保存的墙面继续运行。", confirmLabel: "放弃草稿" })) return;
    action();
  }
  return <section className="wall-management" aria-label="屏幕墙管理">
    <header className="wall-toolbar"><div><h2>屏幕墙</h2>
      <p className="wall-muted">布局保存在 Loom 中控。关闭管理页面后，已连接终端继续运行。</p></div>
      <div className="wall-toolbar">
        <button type="button" className="ghost-button" disabled={!online || catalog.sourcesLoading}
          onClick={catalog.refreshSources}>{catalog.sourcesLoading ? "读取来源..." : "刷新内容来源"}</button>
        <button type="button" className="ghost-button" disabled={!online || busy} onClick={() => void abandon(catalog.refresh)}>重新载入目录</button>
        <button type="button" className={draft ? "ghost-button" : "signal-button"} disabled={!state || busy || !online || state.layouts.length >= 64}
          onClick={() => void abandon(() => catalog.select())}>创建墙面</button>
      </div>
    </header>
    {!online && <p role="status" className="wall-warning">Loom 中控离线，无法读取或保存布局。</p>}
    {loading && <p role="status">正在读取屏幕墙...</p>}
    {error && <p role="alert" className="wall-error">{error}</p>}
    {inventory.errors.map((message) => <p className="wall-warning" key={message}>{message}</p>)}
    {state && <>
      <WallEndpointList state={state} disabled={busy || !online} identify={catalog.identify} />
      <nav className="wall-list" aria-label="选择墙面">
        {!state.layouts.length && <p className="wall-muted">还没有墙面。创建后可添加输出并安排内容。</p>}
        {state.layouts.map((layout) => <button type="button" className="ghost-button" key={layout.wallId}
          aria-pressed={draft?.layout.wallId === layout.wallId} disabled={busy}
          onClick={() => void abandon(() => catalog.select(layout))}>{layout.wallId} · {layout.tiles.length} 块</button>)}
      </nav>
      {draft && <section className="wall-draft" aria-label="墙面草稿">
        <div className="wall-toolbar"><h3>{draft.persisted ? "编辑墙面" : "新墙面"}</h3>
          <span>草稿目录 v{draft.baseRevision} / 当前目录 v{state.revision}{draft.dirty ? " · 未保存" : " · 已保存"}</span></div>
        {conflict && <p role="alert" className="wall-warning">目录已被其他操作更新，草稿仍保留。请重新载入并核对后编辑，当前草稿不能覆盖新版本。</p>}
        {invalid && <p role="alert" className="wall-error">{invalid}</p>}
        {draft.persisted && <WallPresentationControls state={state} wallId={draft.layout.wallId} disabled={busy || !online} onMode={catalog.present} />}
        <fieldset className="wall-edit-fields" disabled={busy || !online}>
          <WallLayoutEditor key={baseUrl} baseUrl={baseUrl} layout={draft.layout} state={state} sources={inventory.sources}
            persisted={draft.persisted} onChange={catalog.edit} />
        </fieldset>
        <label className="wall-toolbar"><span>场景准备时间</span>
          <select className="studio-input" value={activationDelayMs} disabled={busy || !online}
            onChange={(event) => setActivationDelayMs(Number(event.target.value))}>
            <option value={0}>准备就绪后立即显示</option>
            <option value={5000} disabled={!schedulable}>5 秒后生效</option>
            <option value={10000} disabled={!schedulable}>10 秒后生效</option>
          </select></label>
        {activationDelayMs > 0 && !schedulable && <p className="wall-warning">部分终端未声明场景调度能力，请升级终端或选择立即显示。</p>}
        <footer className="wall-toolbar">
          {draft.persisted ? <button type="button" className="danger-button" disabled={busy || conflict || !online}
            onClick={() => void (async () => {
              if (await requestAppConfirmation({ title: "删除墙面", message: "终端将不再呈现此墙面；注册的输出和原始内容仍然保留。", confirmLabel: "删除墙面" })) await catalog.remove();
            })()}>删除墙面</button> : <span />}
          <div className="wall-toolbar"><button type="button" className="ghost-button" disabled={busy}
            onClick={() => void abandon(catalog.close)}>关闭编辑</button>
            <button type="button" className="signal-button" disabled={busy || conflict || Boolean(invalid) || !draft.dirty || !online || (activationDelayMs > 0 && !schedulable)}
              onClick={() => void catalog.save(activationDelayMs)}>{busy ? "保存中..." : "保存布局"}</button></div>
        </footer>
      </section>}
    </>}
  </section>;
}
