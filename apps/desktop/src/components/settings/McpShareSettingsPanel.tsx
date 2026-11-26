// Outbound MCP Share Token & Cloud Relay settings panel inside Settings -> MCP.
import { useCallback, useEffect, useState } from "react";
import { Button, Input, Switch } from "@neuro/ui";
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

const SCOPE_OPTIONS: { id: McpShareScope; label: string }[] = [
  { id: "arts:execute", label: "流程包/Art 执行" },
  { id: "capabilities:invoke", label: "能力包与全局 AI" },
  { id: "screen:read", label: "实时画面读取" },
  { id: "devices:control", label: "设备与 Otter 同步" },
  { id: "projection:control", label: "投屏与屏幕墙控制" },
];

export function McpShareSettingsPanel({ baseUrl }: { baseUrl: string }) {
  const [status, setStatus] = useState<McpShareStatusResponse | null>(null);
  const [tokens, setTokens] = useState<ShareTokenSummary[]>([]);
  const [label, setLabel] = useState("Claude / Cursor 共享令牌");
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
    <section className="settings-mcp-panel" aria-label="Loom 对外 MCP 共享">
      <header className="settings-mcp-panel__header">
        <SettingsSectionIcon kind="mcp" />
        <strong>对外 MCP 服务共享与云中继</strong>
      </header>
      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>HTTP / Stdio 接入端点</strong>
          <span>
            HTTP: <code>{status?.httpEndpoint ?? `${baseUrl}/mcp`}</code> ｜ Stdio:{" "}
            <code>{status?.stdioCommand ?? "loom mcp-stdio"}</code>
          </span>
        </div>
      </div>

      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>NAccount 互联网云中继</strong>
          <span>
            登录态下支持异地三层投射中转（Otter → Loom 1 → 云中继 → Loom 2 → Otter）与远程 MCP 调用
          </span>
        </div>
        <Switch
          className="settings-general-toggle"
          aria-label="启用 NAccount 云中继"
          checked={Boolean(status?.cloudRelay.enabled)}
          disabled={busy}
          onChange={(event) => void handleRelayPatch({ enabled: event.target.checked })}
        />
      </div>

      {status?.cloudRelay.enabled ? (
        <div className="settings-network-row">
          <div className="settings-network-row__label">
            <strong>允许经云中继转发 MCP</strong>
            <span>仍强制校验下方签发的最小权限 MCP 共享令牌</span>
          </div>
          <Switch
            className="settings-general-toggle"
            aria-label="允许经云中继转发 MCP"
            checked={Boolean(status.cloudRelay.exposeMcp)}
            disabled={busy}
            onChange={(event) => void handleRelayPatch({ exposeMcp: event.target.checked })}
          />
        </div>
      ) : null}

      <div className="settings-network-row">
        <div className="settings-network-row__label">
          <strong>签发权限范围令牌</strong>
          <span>按需勾选暴露给外部 MCP 客户端的能力范围</span>
        </div>
        <div style={{ display: "flex", gap: "0.5rem", flexWrap: "wrap" }}>
          {SCOPE_OPTIONS.map((opt) => (
            <label key={opt.id} style={{ display: "inline-flex", alignItems: "center", gap: "0.25rem", fontSize: "0.85rem" }}>
              <input
                type="checkbox"
                checked={selectedScopes.includes(opt.id)}
                onChange={() => toggleScope(opt.id)}
              />
              {opt.label}
            </label>
          ))}
        </div>
      </div>

      <div className="settings-network-row">
        <Input
          aria-label="MCP 共享令牌名称"
          value={label}
          placeholder="令牌备注名称"
          onChange={(event) => setLabel(event.target.value)}
        />
        <Button disabled={busy || selectedScopes.length === 0} onClick={() => void handleCreateToken()}>
          生成共享令牌
        </Button>
      </div>

      {newlyIssuedToken ? (
        <div className="settings-network-row" role="status">
          <div className="settings-network-row__label">
            <strong>新生成令牌（仅显示一次）</strong>
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
            <span>权限: {tok.scopes.join(", ")}</span>
          </div>
          <Button disabled={busy} onClick={() => void handleRevoke(tok.id)}>
            吊销
          </Button>
        </div>
      ))}
    </section>
  );
}
