// Outbound MCP Share Token & Cloud Relay settings panel inside Settings -> MCP.
import { useCallback, useEffect, useState } from "react";
import { Button, Input, Switch } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
import { deleteJson, getJson, postJson, putJson } from "../../services/loomApi/transport";
import { SettingsSectionIcon } from "./settingsModel";

type McpShareScope =
  | "arts:execute"
  | "capabilities:invoke"
  | "screen:read"
  | "devices:control"
  | "projection:control";

interface ShareTokenSummary {
  id: string;
  label: string;
  tokenPrefix: string;
  scopes: McpShareScope[];
  createdAtMs: number;
}

interface CloudRelayStatus {
  enabled: boolean;
  signedIn: boolean;
  accountId?: string | null;
  relayUrl?: string | null;
  exposeMcp: boolean;
  exposeProjectionRelay: boolean;
}

interface McpShareStatusResponse {
  httpEndpoint: string;
  stdioCommand: string;
  activeTokenCount: number;
  cloudRelay: CloudRelayStatus;
}

const SCOPE_OPTIONS: readonly { id: McpShareScope; labelKey: string }[] = [
  { id: "arts:execute", labelKey: "settings.mcpShare.scope.artsExecute" },
  { id: "capabilities:invoke", labelKey: "settings.mcpShare.scope.capabilitiesInvoke" },
  { id: "screen:read", labelKey: "settings.mcpShare.scope.screenRead" },
  { id: "devices:control", labelKey: "settings.mcpShare.scope.devicesControl" },
  { id: "projection:control", labelKey: "settings.mcpShare.scope.projectionControl" },
];

export function McpShareSettingsPanel({ baseUrl }: { baseUrl: string }) {
  const { t, tf } = useUiText();
  const [status, setStatus] = useState<McpShareStatusResponse | null>(null);
  const [tokens, setTokens] = useState<ShareTokenSummary[]>([]);
  const [label, setLabel] = useState(() => t("settings.mcpShare.defaultTokenLabel"));
  const [selectedScopes, setSelectedScopes] = useState<McpShareScope[]>([
    "arts:execute",
    "capabilities:invoke",
  ]);
  const [newlyIssuedToken, setNewlyIssuedToken] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = useCallback(async () => {
    try {
      const [statusData, tokensData] = await Promise.all([
        getJson<McpShareStatusResponse>(baseUrl, "/v1/mcp-share/status"),
        getJson<{ tokens: ShareTokenSummary[] }>(baseUrl, "/v1/mcp-share/tokens"),
      ]);
      setStatus(statusData);
      setTokens(tokensData.tokens ?? []);
    } catch {
      // Non-fatal in disconnected preview/test mocks.
    }
  }, [baseUrl]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const toggleScope = (scope: McpShareScope) => {
    setSelectedScopes((prev) =>
      prev.includes(scope) ? prev.filter((item) => item !== scope) : [...prev, scope],
    );
  };

  const handleCreateToken = async () => {
    if (!label.trim() || selectedScopes.length === 0) {
      return;
    }
    setBusy(true);
    try {
      const created = await postJson<{ token: string; summary: ShareTokenSummary }>(
        baseUrl,
        "/v1/mcp-share/tokens",
        { label: label.trim(), scopes: selectedScopes },
      );
      setNewlyIssuedToken(created.token);
      await reload();
    } finally {
      setBusy(false);
    }
  };

  const handleRevoke = async (id: string) => {
    setBusy(true);
    try {
      await deleteJson(baseUrl, `/v1/mcp-share/tokens/${encodeURIComponent(id)}`);
      await reload();
    } finally {
      setBusy(false);
    }
  };

  const handleRelayPatch = async (patch: Partial<CloudRelayStatus>) => {
    if (!status) {
      return;
    }
    const next = { ...status.cloudRelay, ...patch };
    setBusy(true);
    try {
      await putJson(baseUrl, "/v1/cloud-relay/config", {
        enabled: next.enabled,
        relayUrl: next.relayUrl ?? null,
        exposeMcp: next.exposeMcp,
        exposeProjectionRelay: next.exposeProjectionRelay,
      });
      await reload();
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="settings-mcp-panel" aria-label={t("settings.mcpShare.panelAria")}>
      <header className="settings-mcp-panel__header">
        <SettingsSectionIcon kind="mcp" />
        <strong>{t("settings.mcpShare.header")}</strong>
      </header>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>{t("settings.mcpShare.endpointsTitle")}</strong>
          <span>
            HTTP: <code>{status?.httpEndpoint ?? `${baseUrl}/mcp`}</code> | Stdio:{" "}
            <code>{status?.stdioCommand ?? "loom mcp-stdio"}</code>
          </span>
        </div>
      </div>

      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>{t("settings.mcpShare.cloudRelayTitle")}</strong>
          <span>{t("settings.mcpShare.cloudRelayDescription")}</span>
        </div>
        <Switch
          className="settings-general-toggle"
          aria-label={t("settings.mcpShare.enableCloudRelayAria")}
          checked={Boolean(status?.cloudRelay.enabled)}
          disabled={busy}
          onChange={(event) => void handleRelayPatch({ enabled: event.target.checked })}
        />
      </div>

      {status?.cloudRelay.enabled ? (
        <div className="settings-network-row">
          <div className="settings-network-row__label">
            <strong>{t("settings.mcpShare.exposeMcpTitle")}</strong>
            <span>{t("settings.mcpShare.exposeMcpDescription")}</span>
          </div>
          <Switch
            className="settings-general-toggle"
            aria-label={t("settings.mcpShare.exposeMcpTitle")}
            checked={Boolean(status.cloudRelay.exposeMcp)}
            disabled={busy}
            onChange={(event) => void handleRelayPatch({ exposeMcp: event.target.checked })}
          />
        </div>
      ) : null}

      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>{t("settings.mcpShare.scopeTokenTitle")}</strong>
          <span>{t("settings.mcpShare.scopeTokenDescription")}</span>
        </div>
        <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap" }}>
          {SCOPE_OPTIONS.map((opt) => (
            <label key={opt.id} style={{ display: "inline-flex", alignItems: "center", gap: "0.25rem", fontSize: "0.85rem" }}>
              <input
                type="checkbox"
                checked={selectedScopes.includes(opt.id)}
                onChange={() => toggleScope(opt.id)}
              />
              {t(opt.labelKey)}
            </label>
          ))}
        </div>
      </div>

      <div className="settings-network-row">
        <Input
          aria-label={t("settings.mcpShare.tokenLabelAria")}
          value={label}
          placeholder={t("settings.mcpShare.tokenLabelPlaceholder")}
          onChange={(event) => setLabel(event.target.value)}
        />
        <Button disabled={busy || selectedScopes.length === 0} onClick={() => void handleCreateToken()}>
          {t("settings.mcpShare.createToken")}
        </Button>
      </div>

      {newlyIssuedToken ? (
        <div className="settings-network-row" role="status">
          <div className="settings-network-row__label">
            <strong>{t("settings.mcpShare.newlyIssuedTitle")}</strong>
            <code>{newlyIssuedToken}</code>
          </div>
        </div>
      ) : null}

      {tokens.map((tok) => (
        <div key={tok.id} className="settings-network-row">
          <div className="settings-network-row__label">
            <strong>
              {tok.label} (<code>{tok.tokenPrefix}…</code>)
            </strong>
            <span>{tf("settings.mcpShare.tokenScopes", { scopes: tok.scopes.join(", ") })}</span>
          </div>
          <Button disabled={busy} onClick={() => void handleRevoke(tok.id)}>
            {t("settings.mcpShare.revoke")}
          </Button>
        </div>
      ))}
    </section>
  );
}
