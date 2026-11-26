import { useEffect, useRef, useState } from "react";
import { Badge, Button, Notice } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
import {
  loadProjectionSettings,
  saveProjectionSettings,
  projectionGroupNameError,
  type ProjectionSettings,
  type ProjectionSettingsView,
} from "../../services/loomApi/projectionSettings.ts";
import { errorMessage } from "../../services/loomApi/transport.ts";
import { requestAppConfirmation } from "../feedback/AppFeedback";
import { ProjectionGroupsEditor } from "./ProjectionGroupsEditor";
import { ProjectionRulesEditor } from "./ProjectionRulesEditor";
import "./ProjectionSettingsPanel.css";
import "./DeviceConnectionLayout.css";

export function ProjectionSettingsPanel({ baseUrl, online }: { baseUrl: string; online: boolean }) {
  const { t, tf } = useUiText();
  const [view, setView] = useState<ProjectionSettingsView | null>(null);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [operation, setOperation] = useState<"save" | "refresh" | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const generation = useRef(0);
  const running = useRef(false);
  useEffect(() => {
    const current = ++generation.current;
    setView(null);
    setDirty(false);
    setError("");
    setNotice("");
    setBusy(online);
    running.current = online;
    setOperation(online ? "refresh" : null);
    if (online)
      void loadProjectionSettings(baseUrl)
        .then((next) => {
          if (generation.current === current) setView(next);
        })
        .catch((reason: unknown) => {
          if (generation.current === current) setError(errorMessage(reason));
        })
        .finally(() => {
          if (generation.current === current) {
            running.current = false;
            setBusy(false);
            setOperation(null);
          }
        });
    return () => {
      generation.current++;
    };
  }, [baseUrl, online]);

  async function perform(save: boolean) {
    if (
      !online ||
      running.current ||
      (save && (!view || view.settings.groups.some((group) => projectionGroupNameError(group.name))))
    )
      return;
    const current = generation.current;
    running.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    setOperation(save ? "save" : "refresh");
    try {
      if (
        !save &&
        dirty &&
        !(await requestAppConfirmation({
          title: t("devices.projectionSettings.confirmRefresh.title"),
          message: t("devices.projectionSettings.confirmRefresh.message"),
          confirmLabel: t("devices.projectionSettings.confirmRefresh.confirm"),
        }))
      )
        return;
      if (generation.current !== current) return;
      if (save && view) {
        const settings = await saveProjectionSettings(baseUrl, view.settings);
        if (generation.current === current) {
          setView({ ...view, settings });
          setDirty(false);
          setNotice(t("devices.projectionSettings.notice.saved"));
        }
      } else {
        const next = await loadProjectionSettings(baseUrl);
        if (generation.current === current) {
          setView(next);
          setDirty(false);
        }
      }
    } catch (reason) {
      if (generation.current === current)
        setError(tf("devices.projectionSettings.error.unconfirmedDraftKept", { reason: errorMessage(reason) }));
    } finally {
      if (generation.current === current) {
        running.current = false;
        setBusy(false);
        setOperation(null);
      }
    }
  }
  function change(settings: ProjectionSettings) {
    if (!view || busy || !online) return;
    setView({ ...view, settings });
    setDirty(true);
    setNotice("");
  }
  const disabled = busy || !online;
  return (
    <section
      className="loom-device-settings projection-settings"
      aria-label={t("devices.projectionSettings.ariaLabel")}
      aria-busy={busy || undefined}
    >
      <header>
        <h2>{t("devices.projectionSettings.heading")}</h2>
      </header>
      {!online && <Notice tone="warning">{t("devices.projection.localOfflineNotice")}</Notice>}
      {error && <Notice tone="danger">{error}</Notice>}
      {notice && <Notice tone="success">{notice}</Notice>}
      <div className="loom-device-settings__actions">
        <Button
          variant="quiet"
          loading={operation === "refresh"}
          disabled={disabled}
          onClick={() => void perform(false)}
        >
          {t("devices.projectionSettings.refreshAction")}
        </Button>
        <Button
          variant="primary"
          loading={operation === "save"}
          disabled={
            disabled || !dirty || !view || view.settings.groups.some((group) => !!projectionGroupNameError(group.name))
          }
          onClick={() => void perform(true)}
        >
          {t("devices.projectionSettings.saveAction")}
        </Button>
        {view && (
          <>
            <code>{tf("devices.common.revisionBadge", { revision: view.settings.revision })}</code>
            <Badge tone={dirty ? "warning" : "neutral"}>
              {dirty ? t("devices.common.unsaved") : t("devices.common.saved")}
            </Badge>
          </>
        )}
      </div>
      {view && (
        <>
          {view.directoryStatus !== "complete" && (
            <Notice tone="warning">{t("devices.projectionSettings.partialDirectoryWarning")}</Notice>
          )}
          <ProjectionRulesEditor
            devices={view.devices}
            groups={view.settings.groups}
            rules={view.settings.rules}
            disabled={disabled}
            onChange={(rules) => change({ ...view.settings, rules })}
          />
          <ProjectionGroupsEditor
            devices={view.devices}
            groups={view.settings.groups}
            disabled={disabled}
            onChange={(groups) =>
              change({
                ...view.settings,
                groups,
                rules: view.settings.rules.map((rule) => ({
                  ...rule,
                  whitelist: {
                    ...rule.whitelist,
                    groups: rule.whitelist.groups.filter((id) => groups.some((group) => group.groupId === id)),
                  },
                })),
              })
            }
          />
        </>
      )}
    </section>
  );
}
