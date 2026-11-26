import type {
  WallEndpointStatus,
  WallLayout,
  WallPresentation,
  WallPresentationMode,
  WallState,
} from "../../services/loomApi/wallTypes.ts";
import { Badge, Button, Notice } from "@neuro/ui";
import { uiFormat, uiText, useUiText } from "../../i18n/index.ts";

export function wallOutputStatus(
  layout: WallLayout,
  control: WallPresentation | undefined,
  endpoint: WallEndpointStatus,
): string {
  if (!control) {
    return endpoint.appliedRevision === layout.revision
      ? uiFormat("devices.wall.outputStatus.appliedRevision", { revision: layout.revision })
      : uiText("devices.wall.outputStatus.pendingApply");
  }
  const report = endpoint.presentation;
  if (!endpoint.online || report?.revision !== control.revision) {
    return uiText(`devices.wall.outputStatus.pending.${control.mode}`);
  }
  return report.outcome === "applied"
    ? uiFormat(`devices.wall.outputStatus.applied.${control.mode}`, { revision: report.revision })
    : uiText("devices.wall.outputStatus.frameUnavailable");
}

export function WallPresentationControls({
  state,
  wallId,
  disabled,
  onMode,
}: {
  state: WallState;
  wallId: string;
  disabled: boolean;
  onMode: (wallId: string, mode: WallPresentationMode) => Promise<void>;
}) {
  const { t, tf } = useUiText();
  const layout = state.layouts.find((layout) => layout.wallId === wallId);
  if (!layout) return null;
  const control = state.presentations?.find((control) => control.wallId === wallId);
  const mode = control?.mode ?? "running";
  const endpoints = state.endpoints.filter((endpoint) =>
    layout.tiles.some((tile) => tile.endpointId === endpoint.endpoint.endpointId),
  );
  const applied = endpoints.filter(
    (endpoint) =>
      endpoint.online &&
      (control
        ? endpoint.presentation?.revision === control.revision && endpoint.presentation.outcome === "applied"
        : endpoint.appliedRevision === layout.revision),
  ).length;
  return (
    <section className="wall-display" aria-label={t("devices.wall.presentation.ariaLabel")}>
      <div className="wall-toolbar">
        <div className="wall-display-status" role="status">
          <Badge tone={mode === "running" ? "success" : "warning"}>{t(`devices.wall.mode.${mode}`)}</Badge>
          <span>{tf("devices.wall.presentation.confirmedOutputs", { applied, total: layout.tiles.length })}</span>
        </div>
        <div className="wall-toolbar">
          {(["frozen", "black", "running"] as const).map((next) => (
            <Button
              key={next}
              aria-pressed={mode === next}
              disabled={disabled || mode === next}
              onClick={() => void onMode(wallId, next)}
            >
              {next === "running"
                ? t("devices.wall.presentation.resumeAction")
                : next === "frozen"
                  ? t("devices.wall.presentation.freezeAction")
                  : t("devices.wall.presentation.blackoutAction")}
            </Button>
          ))}
        </div>
      </div>
      <Notice tone="warning">{t("devices.wall.presentation.inputPausedWarning")}</Notice>
    </section>
  );
}
