import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Badge, Button, EmptyState, Field, Notice, Select } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
import { parseWallLayout } from "../../services/loomApi/wallValidation.ts";
import { requestAppConfirmation } from "../feedback/AppFeedback.tsx";
import { useWallCatalog } from "./useWallCatalog.ts";
import { WallLayoutEditor } from "./WallLayoutEditor.tsx";
import { WallPresentationControls } from "./WallPresentationControls.tsx";
import { WallEndpointList } from "./WallEndpointList.tsx";

export function WallManagementPanel({ baseUrl, online }: { baseUrl: string; online: boolean }) {
  const { t, tf } = useUiText();
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
  const wallSelector = useRef<HTMLSelectElement>(null);
  const focusAfterClose = useRef(false);
  // The confirmation closes before the asynchronous editor removal; restore after the actual commit.
  useLayoutEffect(() => {
    if (busy || !focusAfterClose.current) return;
    focusAfterClose.current = false;
    if (!draft && online) {
      if (createButton.current && !createButton.current.disabled) createButton.current.focus();
      else wallSelector.current?.focus();
    }
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
      invalid = error instanceof Error ? error.message : t("devices.wall.error.invalidLayout");
    }
  }
  async function abandon(action: () => void) {
    const current = generation.current;
    if (
      draft?.dirty &&
      !(await requestAppConfirmation({
        title: t("devices.wall.confirmAbandon.title"),
        message: t("devices.wall.confirmAbandon.message"),
        confirmLabel: t("devices.wall.confirmAbandon.confirm"),
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
        title: t("devices.wall.confirmDelete.title"),
        message: t("devices.wall.confirmDelete.message"),
        confirmLabel: t("devices.wall.confirmDelete.confirm"),
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
    <section
      className="wall-management"
      aria-label={t("devices.wall.managementAriaLabel")}
      aria-busy={busy || loading || undefined}
    >
      <header className="wall-toolbar">
        <h2>{t("devices.wall.heading")}</h2>
        <div className="wall-toolbar">
          <Button
            variant="quiet"
            loading={catalog.sourcesLoading}
            disabled={!online || catalog.sourcesLoading}
            onClick={catalog.refreshSources}
          >
            {t("devices.wall.refreshSourcesAction")}
          </Button>
          <Button variant="quiet" disabled={!online || busy} onClick={() => void abandon(catalog.refresh)}>
            {t("devices.wall.reloadCatalogAction")}
          </Button>
          <Button
            ref={createButton}
            variant={draft ? "quiet" : "primary"}
            disabled={!state || busy || !online || state.layouts.length >= 64}
            onClick={() => void abandon(() => catalog.select())}
          >
            {t("devices.wall.createWallAction")}
          </Button>
        </div>
      </header>
      {!online && <Notice tone="warning">{t("devices.wall.offlineNotice")}</Notice>}
      {loading && (
        <div role="status">
          <EmptyState title={t("devices.wall.loading")} />
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
          <nav className="wall-list" aria-label={t("devices.wall.selectWallNavAriaLabel")}>
            {!state.layouts.length ? (
              <EmptyState title={t("devices.wall.empty")} />
            ) : state.layouts.length > 6 ? (
              <Field label={t("devices.wall.wallSelectLabel")}>
                <Select
                  ref={wallSelector}
                  value={draft?.persisted ? draft.layout.wallId : ""}
                  disabled={busy}
                  onChange={(event) => {
                    const layout = state.layouts.find((item) => item.wallId === event.target.value);
                    if (layout) void abandon(() => catalog.select(layout));
                  }}
                >
                  <option value="" disabled>
                    {t("devices.wall.selectWallPlaceholder")}
                  </option>
                  {state.layouts.map((layout) => (
                    <option key={layout.wallId} value={layout.wallId}>
                      {tf("devices.wall.wallOptionSummary", { id: layout.wallId, count: layout.tiles.length })}
                    </option>
                  ))}
                </Select>
              </Field>
            ) : (
              state.layouts.map((layout) => (
                <Button
                  className="wall-list-choice"
                  key={layout.wallId}
                  aria-label={tf("devices.wall.editWallAriaLabel", { id: layout.wallId })}
                  aria-pressed={draft?.layout.wallId === layout.wallId}
                  disabled={busy}
                  onClick={() => void abandon(() => catalog.select(layout))}
                >
                  <span className="wall-list-id" title={layout.wallId}>
                    {layout.wallId}
                  </span>
                  <Badge>{tf("devices.wall.tileCountBadge", { count: layout.tiles.length })}</Badge>
                </Button>
              ))
            )}
          </nav>
          {draft && (
            <section className="wall-draft" aria-label={t("devices.wall.draftSectionAriaLabel")}>
              <div className="wall-toolbar">
                <h3>{draft.persisted ? t("devices.wall.editWallHeading") : t("devices.wall.newWallHeading")}</h3>
                <div className="wall-draft-status">
                  <code>
                    {tf("devices.wall.revisionComparison", { draft: draft.baseRevision, catalog: state.revision })}
                  </code>
                  <Badge tone={draft.dirty ? "warning" : "neutral"}>
                    {draft.dirty ? t("devices.common.unsaved") : t("devices.common.saved")}
                  </Badge>
                </div>
              </div>
              {conflict && <Notice tone="warning">{t("devices.wall.conflictWarning")}</Notice>}
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
              <Field label={t("devices.wall.activationDelayLabel")}>
                <Select
                  value={activationDelayMs}
                  disabled={busy || !online}
                  onChange={(event) => setActivationDelayMs(Number(event.target.value))}
                >
                  <option value={0}>{t("devices.wall.activationDelay.immediate")}</option>
                  <option value={5000} disabled={!schedulable}>
                    {t("devices.wall.activationDelay.fiveSeconds")}
                  </option>
                  <option value={10000} disabled={!schedulable}>
                    {t("devices.wall.activationDelay.tenSeconds")}
                  </option>
                </Select>
              </Field>
              {activationDelayMs > 0 && !schedulable && (
                <Notice tone="warning">{t("devices.wall.unschedulableWarning")}</Notice>
              )}
              <footer className="wall-toolbar">
                {draft.persisted ? (
                  <Button variant="danger" disabled={busy || conflict || !online} onClick={() => void removeWall()}>
                    {t("devices.wall.confirmDelete.confirm")}
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
                    {t("devices.wall.closeEditorAction")}
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
                    {t("devices.wall.saveLayoutAction")}
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
