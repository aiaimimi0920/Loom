import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Badge, Button, EmptyState, Field, Notice, Select } from "@neuro/ui";
import { parseWallLayout } from "../../services/loomApi/wallValidation.ts";
import { requestAppConfirmation } from "../feedback/AppFeedback.tsx";
import { useWallCatalog } from "./useWallCatalog.ts";
import { WallLayoutEditor } from "./WallLayoutEditor.tsx";
import { WallPresentationControls } from "./WallPresentationControls.tsx";
import { WallEndpointList } from "./WallEndpointList.tsx";

export function WallManagementPanel({ baseUrl, online }: { baseUrl: string; online: boolean }) {
  const catalog = useWallCatalog(baseUrl, online);
  const [saving, setSaving] = useState(false);
  const generation = useRef(0);
  useEffect(() => {
    generation.current++;
    setSaving(false);
    return () => {
      generation.current++;
    };
  }, [baseUrl, online]);
  const [activationDelayMs, setActivationDelayMs] = useState(0);
  const { state, draft, inventory, busy, error, loading } = catalog;
  const createButton = useRef<HTMLButtonElement>(null);
  const focusAfterClose = useRef(false);
  // The confirmation closes before the asynchronous editor removal; restore after the actual commit.
  useLayoutEffect(() => {
    if (busy || !focusAfterClose.current) return;
    focusAfterClose.current = false;
    if (!draft && online) createButton.current?.focus();
  }, [busy, draft, online]);
  const conflict = Boolean(draft && state && draft.baseRevision !== state.revision);
  const schedulable = Boolean(
    draft &&
      state &&
      draft.layout.tiles.every((tile) =>
        state.endpoints.some(
          ({ endpoint }) => endpoint.endpointId === tile.endpointId && endpoint.scheduledPresentation,
        ),
      ),
  );
  let invalid = "";
  if (draft) {
    try {
      parseWallLayout(draft.layout);
    } catch (error) {
      invalid = error instanceof Error ? error.message : "布局无效";
    }
  }
  async function abandon(action: () => void) {
    const current = generation.current;
    if (
      draft?.dirty &&
      !(await requestAppConfirmation({
        title: "放弃未保存的布局？",
        message: "重新载入或切换墙面会丢弃当前草稿。已保存的墙面继续运行。",
        confirmLabel: "放弃草稿",
      }))
    )
      return;
    if (current !== generation.current) return;
    action();
  }
  async function removeWall() {
    const current = generation.current;
    if (
      !(await requestAppConfirmation({
        title: "删除墙面",
        message: "终端将不再呈现此墙面；注册的输出和原始内容仍然保留。",
        confirmLabel: "删除墙面",
      })) ||
      current !== generation.current
    )
      return;
    focusAfterClose.current = true;
    await catalog.remove();
  }
  async function saveLayout() {
    const current = generation.current;
    setSaving(true);
    try {
      await catalog.save(activationDelayMs);
    } finally {
      if (current === generation.current) setSaving(false);
    }
  }
  return (
    <section className="wall-management" aria-label="屏幕墙管理" aria-busy={busy || loading || undefined}>
      <header className="wall-toolbar">
        <h2>屏幕墙</h2>
        <div className="wall-toolbar">
          <Button
            variant="quiet"
            loading={catalog.sourcesLoading}
            disabled={!online || catalog.sourcesLoading}
            onClick={catalog.refreshSources}
          >
            刷新内容来源
          </Button>
          <Button variant="quiet" disabled={!online || busy} onClick={() => void abandon(catalog.refresh)}>
            重新载入目录
          </Button>
          <Button
            ref={createButton}
            variant={draft ? "quiet" : "primary"}
            disabled={!state || busy || !online || state.layouts.length >= 64}
            onClick={() => void abandon(() => catalog.select())}
          >
            创建墙面
          </Button>
        </div>
      </header>
      {!online && <Notice tone="warning">Loom 中控离线，无法读取或保存布局。</Notice>}
      {loading && (
        <div role="status">
          <EmptyState title="正在读取屏幕墙" />
        </div>
      )}
      {error && <Notice tone="danger">{error}</Notice>}
      {inventory.errors.map((message) => (
        <Notice tone="warning" key={message}>
          {message}
        </Notice>
      ))}
      {state && (
        <>
          <WallEndpointList state={state} disabled={busy || !online} identify={catalog.identify} />
          <nav className="wall-list" aria-label="选择墙面">
            {!state.layouts.length ? (
              <EmptyState title="暂无墙面" />
            ) : state.layouts.length > 6 ? (
              <Field label="墙面">
                <Select
                  value={draft?.persisted ? draft.layout.wallId : ""}
                  disabled={busy}
                  onChange={(event) => {
                    const layout = state.layouts.find((item) => item.wallId === event.target.value);
                    if (layout) void abandon(() => catalog.select(layout));
                  }}
                >
                  <option value="" disabled>
                    选择墙面
                  </option>
                  {state.layouts.map((layout) => (
                    <option key={layout.wallId} value={layout.wallId}>
                      {layout.wallId} · {layout.tiles.length} 块
                    </option>
                  ))}
                </Select>
              </Field>
            ) : (
              state.layouts.map((layout) => (
                <Button
                  className="wall-list-choice"
                  key={layout.wallId}
                  aria-label={`编辑墙面 ${layout.wallId}`}
                  aria-pressed={draft?.layout.wallId === layout.wallId}
                  disabled={busy}
                  onClick={() => void abandon(() => catalog.select(layout))}
                >
                  <span className="wall-list-id" title={layout.wallId}>
                    {layout.wallId}
                  </span>
                  <Badge>{layout.tiles.length} 块</Badge>
                </Button>
              ))
            )}
          </nav>
          {draft && (
            <section className="wall-draft" aria-label="墙面草稿">
              <div className="wall-toolbar">
                <h3>{draft.persisted ? "编辑墙面" : "新墙面"}</h3>
                <div className="wall-draft-status">
                  <code>
                    草稿 v{draft.baseRevision} / 目录 v{state.revision}
                  </code>
                  <Badge tone={draft.dirty ? "warning" : "neutral"}>{draft.dirty ? "未保存" : "已保存"}</Badge>
                </div>
              </div>
              {conflict && (
                <Notice tone="warning">
                  目录已被其他操作更新，草稿仍保留。请重新载入并核对后编辑，当前草稿不能覆盖新版本。
                </Notice>
              )}
              {invalid && <Notice tone="danger">{invalid}</Notice>}
              {draft.persisted && (
                <WallPresentationControls
                  state={state}
                  wallId={draft.layout.wallId}
                  disabled={busy || !online}
                  onMode={catalog.present}
                />
              )}
              <fieldset className="wall-edit-fields" disabled={busy || !online}>
                <WallLayoutEditor
                  key={baseUrl}
                  baseUrl={baseUrl}
                  layout={draft.layout}
                  state={state}
                  sources={inventory.sources}
                  persisted={draft.persisted}
                  onChange={catalog.edit}
                />
              </fieldset>
              <Field label="场景准备时间">
                <Select
                  value={activationDelayMs}
                  disabled={busy || !online}
                  onChange={(event) => setActivationDelayMs(Number(event.target.value))}
                >
                  <option value={0}>准备就绪后立即显示</option>
                  <option value={5000} disabled={!schedulable}>
                    5 秒后生效
                  </option>
                  <option value={10000} disabled={!schedulable}>
                    10 秒后生效
                  </option>
                </Select>
              </Field>
              {activationDelayMs > 0 && !schedulable && (
                <Notice tone="warning">部分终端未声明场景调度能力，请升级终端或选择立即显示。</Notice>
              )}
              <footer className="wall-toolbar">
                {draft.persisted ? (
                  <Button variant="danger" disabled={busy || conflict || !online} onClick={() => void removeWall()}>
                    删除墙面
                  </Button>
                ) : (
                  <span />
                )}
                <div className="wall-toolbar">
                  <Button
                    variant="quiet"
                    disabled={busy}
                    onClick={() =>
                      void abandon(() => {
                        focusAfterClose.current = true;
                        catalog.close();
                      })
                    }
                  >
                    关闭编辑
                  </Button>
                  <Button
                    variant="primary"
                    loading={saving}
                    disabled={
                      busy ||
                      conflict ||
                      Boolean(invalid) ||
                      !draft.dirty ||
                      !online ||
                      (activationDelayMs > 0 && !schedulable)
                    }
                    onClick={() => void saveLayout()}
                  >
                    保存布局
                  </Button>
                </div>
              </footer>
            </section>
          )}
        </>
      )}
    </section>
  );
}
