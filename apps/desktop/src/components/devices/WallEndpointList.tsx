import type { WallState } from "../../services/loomApi/wallTypes.ts";
import { Badge, Button, EmptyState, Panel } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
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
  const { t, tf } = useUiText();
  return (
    <section aria-label={t("devices.wall.endpoints.ariaLabel")}>
      <h3>{tf("devices.wall.endpoints.heading", { count: state.endpoints.length })}</h3>
      {!state.endpoints.length && <EmptyState title={t("devices.wall.endpoints.empty")} />}
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
                <Badge tone={online ? "success" : "neutral"}>
                  {online ? t("devices.status.online") : t("devices.status.offline")}
                </Badge>
                <span>
                  {endpoint.pixelSize.width} × {endpoint.pixelSize.height}
                </span>
              </div>
              <span className="n-muted">
                {endpoint.deviceId} / {endpoint.outputId}
              </span>
              <span>
                {tf("devices.wall.endpoints.capabilities", {
                  render: endpoint.renderModes.join(" / "),
                  input: endpoint.inputCapabilities.join(" / ") || t("devices.common.none"),
                })}
              </span>
              <span className="n-muted">
                {tf("devices.wall.endpoints.scheduling", {
                  status: endpoint.scheduledPresentation
                    ? t("devices.wall.endpoints.schedulingSupported")
                    : t("devices.wall.endpoints.schedulingUndeclared"),
                })}
                {status.scene
                  ? tf("devices.wall.endpoints.sceneStatus", {
                      revision: status.scene.revision,
                      state:
                        status.scene.appliedAtMs !== null
                          ? t("devices.wall.endpoints.sceneCommitted")
                          : status.scene.prepared
                            ? t("devices.wall.endpoints.scenePrepared")
                            : t("devices.wall.endpoints.scenePreparing"),
                    })
                  : ""}
                {status.scene?.clockUncertaintyMs != null
                  ? tf("devices.wall.endpoints.clockUncertainty", { ms: status.scene.clockUncertaintyMs })
                  : ""}
              </span>
              <div className="wall-endpoint-assignment">
                <Badge tone={control ? "warning" : "neutral"}>
                  {wall ? wallOutputStatus(wall, control, status) : t("devices.wall.endpoints.unassigned")}
                </Badge>
                {wall && <code>{wall.wallId}</code>}
              </div>
              <div className="wall-endpoint-action">
                <Button
                  aria-label={tf("devices.wall.endpoints.identifyAriaLabel", {
                    name: endpoint.display?.name ?? endpoint.outputId,
                  })}
                  disabled={disabled || !online || !endpoint.display?.canIdentify || Boolean(control || identification)}
                  onClick={() => void identify(endpoint.endpointId)}
                >
                  {t("devices.wall.endpoints.identifyAction")}
                </Button>
                {(identification || !endpoint.display?.canIdentify || control) && (
                  <span role="status">
                    {identification
                      ? tf("devices.wall.endpoints.identifyingStatus", {
                          state: identification.applied
                            ? t("devices.wall.endpoints.identifyingActive")
                            : t("devices.wall.endpoints.identifyingWaiting"),
                          seconds: Math.ceil(identification.remainingMs / 1000),
                        })
                      : !endpoint.display?.canIdentify
                        ? t("devices.wall.endpoints.cannotIdentify")
                        : t("devices.wall.endpoints.resumeBeforeIdentify")}
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
