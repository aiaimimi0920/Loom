import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { createPortal } from "react-dom";
import { invoke, isTauri } from "@tauri-apps/api/core";

import {
  deleteMcpServer,
  saveMcpServer,
  testMcpConnection,
  type LoomMcpServer,
} from "../../services/loomApi";
import {
  MCP_MARKET_CATEGORIES,
  MCP_MARKET_SERVERS,
  buildMcpPaginationItems,
  buildMarketplaceServerConfig,
  findInstalledMcpServer,
  getMarketplaceHealth,
  isValidMcpRemoteUrl,
  mcpMarketCategoryLabel,
  parseMcpKeyValueLines,
  type McpMarketCategory,
  type McpMarketServer,
  type McpMarketplaceTestSnapshot,
} from "../../services/mcpMarketplace";

type McpWorkspaceId = "services" | "store";
type NotificationLevel = "info" | "warning" | "error";

interface McpHubProps {
  servers: LoomMcpServer[];
  baseUrl: string;
  refresh: () => Promise<void>;
  notify: (level: NotificationLevel, text: string) => void;
  confirmRemove: (server: LoomMcpServer) => Promise<boolean>;
}

interface McpEditorState {
  mode: "create" | "edit" | "install" | "link";
  server: LoomMcpServer;
  marketItem?: McpMarketServer;
}

const workspaceItems: Array<{ id: McpWorkspaceId; label: string }> = [
  { id: "services", label: "服务" },
  { id: "store", label: "商店" },
];

const MCP_STORE_PAGE_SIZE = 24;

const createRemoteMcpDraft = (): LoomMcpServer => ({
  id: `remote-mcp-${Date.now().toString(36)}`,
  name: "远程 MCP",
  description: "",
  transport: "streamable-http",
  command: "",
  args: [],
  env: {},
  url: "",
  headers: {},
  enabled: true,
});

const parseLines = (value: string) => value
  .split(/\r?\n/)
  .map((line) => line.trim())
  .filter(Boolean);

const environmentText = (environment: Record<string, string> = {}) => Object.entries(environment)
  .map(([key, value]) => `${key}=${value}`)
  .join("\n");

const isArtManagedServer = (server: LoomMcpServer) =>
  server.managed === true && server.source === "art";

const normalizedServerId = (value: string) => value
  .trim()
  .replace(/[^a-zA-Z0-9_.@/-]/g, "-")
  .replace(/^-+|-+$/g, "") || "local-mcp-server";

function McpIcon({ kind }: { kind: "plug" | "edit" | "power" | "trash" | "test" | "close" | "external" }) {
  const props = {
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.8,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };

  switch (kind) {
    case "edit":
      return <svg {...props}><path d="M4 20h4l11-11a2.8 2.8 0 0 0-4-4L4 16v4Z" /><path d="m13.5 6.5 4 4" /></svg>;
    case "power":
      return <svg {...props}><path d="M12 3v9" /><path d="M7.1 5.8a8 8 0 1 0 9.8 0" /></svg>;
    case "trash":
      return <svg {...props}><path d="M4 7h16M9 7V4h6v3M7 7l1 13h8l1-13M10 11v5M14 11v5" /></svg>;
    case "test":
      return <svg {...props}><path d="M5 12h14" /><path d="m13 6 6 6-6 6" /><circle cx="6" cy="12" r="2" /></svg>;
    case "close":
      return <svg {...props}><path d="m6 6 12 12M18 6 6 18" /></svg>;
    case "external":
      return <svg {...props}><path d="M14 4h6v6M20 4l-9 9" /><path d="M18 13v6a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h6" /></svg>;
    default:
      return <svg {...props}><path d="M8 3v5M16 3v5M6 8h12v2a6 6 0 0 1-6 6v5M9 21h6" /></svg>;
  }
}

function McpServerDialog({
  editor,
  busy,
  onClose,
  onSave,
}: {
  editor: McpEditorState | null;
  busy: boolean;
  onClose: () => void;
  onSave: (server: LoomMcpServer) => Promise<void>;
}) {
  const dialogRef = useRef<HTMLElement | null>(null);
  const nameInputRef = useRef<HTMLInputElement | null>(null);
  const busyRef = useRef(busy);
  const [id, setId] = useState("");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [transport, setTransport] = useState<"stdio" | "streamable-http">("stdio");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const [environment, setEnvironment] = useState("");
  const [url, setUrl] = useState("");
  const [headers, setHeaders] = useState("");
  const [installOptionId, setInstallOptionId] = useState("");
  const [enabled, setEnabled] = useState(true);

  busyRef.current = busy;

  useEffect(() => {
    if (!editor) return;
    setId(editor.server.id);
    setName(editor.server.name);
    setDescription(editor.server.description || "");
    setTransport(editor.server.transport === "streamable-http" ? "streamable-http" : "stdio");
    setCommand(editor.server.command);
    setArgs((editor.server.args || []).join("\n"));
    setEnvironment(environmentText(editor.server.env));
    setUrl(editor.server.url || "");
    setHeaders(environmentText(editor.server.headers));
    const selectedOption = editor.marketItem?.installOptions.find((option) =>
      option.transport === editor.server.transport &&
      (option.transport === "stdio" ? option.command === editor.server.command : option.url === editor.server.url)) ||
      editor.marketItem?.installOptions.find((option) => option.transport === editor.server.transport) ||
      editor.marketItem?.installOptions[0];
    setInstallOptionId(selectedOption?.id || "");
    setEnabled(editor.server.enabled !== false);
  }, [editor]);

  useEffect(() => {
    if (!editor) return;
    const previouslyFocused = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const focusTimer = window.setTimeout(() => nameInputRef.current?.focus(), 0);
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        if (!busyRef.current) onClose();
        return;
      }
      if (event.key !== "Tab") return;
      const focusable = Array.from(dialogRef.current?.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ) ?? []);
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const focusInsideDialog = dialogRef.current?.contains(document.activeElement) === true;
      if (!focusInsideDialog) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus();
      } else if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => {
      window.clearTimeout(focusTimer);
      window.removeEventListener("keydown", handleKeyDown, true);
      document.body.style.overflow = previousOverflow;
      if (previouslyFocused?.isConnected) previouslyFocused.focus();
    };
  }, [editor, onClose]);

  if (!editor) return null;

  const parsedEnvironment = parseMcpKeyValueLines(environment);
  const parsedHeaders = parseMcpKeyValueLines(headers);
  const invalidEnvironmentLines = transport === "stdio" ? parsedEnvironment.invalidLineNumbers : [];
  const invalidHeaderLines = transport === "streamable-http" ? parsedHeaders.invalidLineNumbers : [];
  const selectedInstallOption = editor.marketItem?.installOptions.find((option) => option.id === installOptionId);
  const missingEnvironmentKeys = (selectedInstallOption?.requiredEnvKeys || [])
    .filter((key) => !parsedEnvironment.values[key]?.trim());
  const missingHeaderKeys = (selectedInstallOption?.requiredHeaderKeys || [])
    .filter((key) => !parsedHeaders.values[key]?.trim());
  const unresolvedArguments = transport === "stdio" && selectedInstallOption?.requiresManualConfiguration === true &&
    parseLines(args).some((argument) => /^<[^>]+>$/.test(argument));
  const unresolvedRemoteUrl = transport === "streamable-http" && /\{[^}]+\}/.test(url);
  const invalidRemoteUrl = transport === "streamable-http" && Boolean(url.trim()) &&
    !unresolvedRemoteUrl && !isValidMcpRemoteUrl(url);
  const canSave = Boolean(id.trim() && name.trim() && (transport === "stdio" ? command.trim() : url.trim())) &&
    invalidEnvironmentLines.length === 0 && invalidHeaderLines.length === 0 &&
    missingEnvironmentKeys.length === 0 && missingHeaderKeys.length === 0 &&
    !unresolvedArguments && !unresolvedRemoteUrl && !invalidRemoteUrl;
  const title = editor.mode === "create" ? "添加 MCP" : editor.mode === "install" ? "安装 MCP" : editor.mode === "link" ? "链接添加" : "编辑 MCP";
  const validationDescription = [
    missingEnvironmentKeys.length > 0 ? "mcp-server-missing-environment" : null,
    missingHeaderKeys.length > 0 ? "mcp-server-missing-headers" : null,
    unresolvedArguments ? "mcp-server-unresolved-arguments" : null,
    unresolvedRemoteUrl ? "mcp-server-unresolved-url" : null,
    invalidRemoteUrl ? "mcp-server-invalid-url" : null,
    invalidEnvironmentLines.length > 0 ? "mcp-server-invalid-environment" : null,
    invalidHeaderLines.length > 0 ? "mcp-server-invalid-headers" : null,
  ].filter(Boolean).join(" ") || undefined;

  return createPortal(
    <div className="framework-dialog-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.target === event.currentTarget && !busy) onClose();
    }}>
      <section
        ref={dialogRef}
        className="framework-dialog mcp-server-dialog"
        role="dialog"
        aria-modal="true"
        aria-busy={busy}
        aria-labelledby="mcp-server-dialog-title"
        aria-describedby={validationDescription}
      >
        <header className="framework-dialog__header">
          <div>
            <h2 id="mcp-server-dialog-title">{title}</h2>
            {editor.marketItem ? <p>{editor.marketItem.installSource.packageName}</p> : null}
          </div>
          <button className="icon-button" type="button" aria-label="关闭" title="关闭" disabled={busy} onClick={onClose}>
            <McpIcon kind="close" />
          </button>
        </header>

        <form className="mcp-server-dialog__form" onSubmit={(event) => {
          event.preventDefault();
          if (!canSave || busy) return;
          void onSave({
            id: normalizedServerId(id),
            name: name.trim(),
            description: description.trim(),
            transport,
            command: transport === "stdio" ? command.trim() : "",
            args: transport === "stdio" ? parseLines(args) : [],
            env: transport === "stdio" ? parsedEnvironment.values : {},
            url: transport === "streamable-http" ? url.trim() : "",
            headers: transport === "streamable-http" ? parsedHeaders.values : {},
            enabled,
          });
        }}>
          <div className="mcp-server-dialog__scroll">
            <div className="mcp-server-dialog__grid">
              <label>
                <span>名称</span>
                <input ref={nameInputRef} className="studio-input" value={name} onChange={(event) => setName(event.target.value)} />
              </label>
              <label>
                <span>ID</span>
                <input className="studio-input" value={id} disabled={editor.mode !== "create"} onChange={(event) => setId(event.target.value)} />
              </label>
            </div>
            <label>
              <span>描述</span>
              <input className="studio-input" value={description} onChange={(event) => setDescription(event.target.value)} />
            </label>
            <label>
              <span>连接方式</span>
              {editor.marketItem?.installOptions.length ? (
                <select className="studio-input" value={installOptionId} onChange={(event) => {
                  const option = editor.marketItem?.installOptions.find((candidate) => candidate.id === event.target.value);
                  if (!option) return;
                  setInstallOptionId(option.id);
                  setTransport(option.transport);
                  setCommand(option.command);
                  setArgs(option.args.join("\n"));
                  setEnvironment(environmentText(option.env));
                  setUrl(option.url);
                  setHeaders(environmentText(option.headers));
                }}>
                  {editor.marketItem.installOptions.map((option) => <option key={option.id} value={option.id}>{option.label}</option>)}
                </select>
              ) : (
                <select className="studio-input" value={transport} onChange={(event) => setTransport(event.target.value as "stdio" | "streamable-http") }>
                  <option value="stdio">本地 · stdio</option>
                  <option value="streamable-http">远程 · Streamable HTTP</option>
                </select>
              )}
            </label>
            {transport === "stdio" ? <>
              <label>
                <span>启动命令</span>
                <input className="studio-input" value={command} onChange={(event) => setCommand(event.target.value)} placeholder="npx、uvx、docker 或本地程序" />
              </label>
              <div className="mcp-server-dialog__grid">
                <label>
                  <span>参数</span>
                  <textarea className="studio-textarea" value={args} onChange={(event) => setArgs(event.target.value)} placeholder="每行一个参数" />
                </label>
                <label>
                  <span>环境变量</span>
                  <textarea className="studio-textarea" value={environment} aria-invalid={invalidEnvironmentLines.length > 0} onChange={(event) => setEnvironment(event.target.value)} placeholder="KEY=value，每行一个" />
                </label>
              </div>
            </> : <>
              <label>
                <span>MCP 地址</span>
                <input className="studio-input" value={url} aria-invalid={unresolvedRemoteUrl || invalidRemoteUrl} onChange={(event) => setUrl(event.target.value)} placeholder="https://example.com/mcp" />
              </label>
              <label>
                <span>请求头</span>
                <textarea className="studio-textarea" value={headers} aria-invalid={invalidHeaderLines.length > 0} onChange={(event) => setHeaders(event.target.value)} placeholder="Authorization=Bearer ...，每行一个" />
              </label>
            </>}
            {missingEnvironmentKeys.length > 0 ? (
              <p id="mcp-server-missing-environment" className="mcp-server-dialog__warning">需要填写：{missingEnvironmentKeys.join("、")}</p>
            ) : null}
            {unresolvedArguments ? (
              <p id="mcp-server-unresolved-arguments" className="mcp-server-dialog__warning">请将参数中的占位符替换为实际值。</p>
            ) : null}
            {missingHeaderKeys.length > 0 ? (
              <p id="mcp-server-missing-headers" className="mcp-server-dialog__warning">需要填写请求头：{missingHeaderKeys.join("、")}</p>
            ) : null}
            {unresolvedRemoteUrl ? (
              <p id="mcp-server-unresolved-url" className="mcp-server-dialog__warning">请将地址中的变量替换为实际值。</p>
            ) : null}
            {invalidRemoteUrl ? (
              <p id="mcp-server-invalid-url" className="mcp-server-dialog__warning">地址必须是有效的 HTTP 或 HTTPS 地址，且不能包含用户名或密码。</p>
            ) : null}
            {invalidEnvironmentLines.length > 0 ? (
              <p id="mcp-server-invalid-environment" className="mcp-server-dialog__warning">环境变量第 {invalidEnvironmentLines.join("、")} 行格式错误，请使用 KEY=value。</p>
            ) : null}
            {invalidHeaderLines.length > 0 ? (
              <p id="mcp-server-invalid-headers" className="mcp-server-dialog__warning">请求头第 {invalidHeaderLines.join("、")} 行格式错误，请使用 KEY=value。</p>
            ) : null}
            <label className="mcp-server-dialog__enabled">
              <input type="checkbox" checked={enabled} onChange={(event) => setEnabled(event.target.checked)} />
              <span>启用</span>
            </label>
          </div>
          <footer className="mcp-server-dialog__actions">
            <button className="ghost-button" type="button" disabled={busy} onClick={onClose}>取消</button>
            <button className="signal-button" type="submit" disabled={busy || !canSave}>
              {busy ? <><span className="mcp-busy-indicator" aria-hidden="true" />处理中</> : editor.mode === "install" ? "安装" : "保存"}
            </button>
          </footer>
        </form>
      </section>
    </div>,
    document.body,
  );
}

export function McpHub({ servers, baseUrl, refresh, notify, confirmRemove }: McpHubProps) {
  const [activeWorkspace, setActiveWorkspace] = useState<McpWorkspaceId>("services");
  const [serviceSearchText, setServiceSearchText] = useState("");
  const [storeSearchText, setStoreSearchText] = useState("");
  const [marketCategory, setMarketCategory] = useState<McpMarketCategory | "All">("All");
  const [marketPage, setMarketPage] = useState(1);
  const [busyServerId, setBusyServerId] = useState<string | null>(null);
  const [busyMarketplaceId, setBusyMarketplaceId] = useState<string | null>(null);
  const [editorBusy, setEditorBusy] = useState(false);
  const [testSnapshots, setTestSnapshots] = useState<Record<string, McpMarketplaceTestSnapshot>>({});
  const [editor, setEditor] = useState<McpEditorState | null>(null);
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const serverOperationRef = useRef(false);
  const editorBusyRef = useRef(false);
  const operationBusy = busyServerId !== null || busyMarketplaceId !== null;
  editorBusyRef.current = editorBusy;
  const closeEditor = useCallback(() => {
    if (!editorBusyRef.current) setEditor(null);
  }, []);

  const openMarketplaceSource = async (marketItem: McpMarketServer) => {
    try {
      if (isTauri()) {
        await invoke("open_mcp_source_url", { url: marketItem.sourceUrl });
      } else {
        window.open(marketItem.sourceUrl, "_blank", "noopener,noreferrer");
      }
    } catch (error) {
      notify(
        "error",
        error instanceof Error
          ? `无法打开 ${marketItem.name} 的介绍页面：${error.message}`
          : `无法打开 ${marketItem.name} 的介绍页面。`,
      );
    }
  };

  useEffect(() => {
    setMarketPage(1);
  }, [storeSearchText]);

  const recordTestResult = (server: LoomMcpServer, result: Awaited<ReturnType<typeof testMcpConnection>>) => {
    const tools = Array.isArray(result.tools) ? result.tools : [];
    setTestSnapshots((current) => ({
      ...current,
      [server.id]: {
        status: result.success === false ? "error" : "success",
        toolCount: tools.length,
        testedAt: new Date().toISOString(),
        error: result.success === false ? result.error || "MCP 测试失败" : undefined,
      },
    }));
    return tools.length;
  };

  const recordTestError = (server: LoomMcpServer, error: unknown) => {
    const message = error instanceof Error ? error.message : "MCP 测试失败";
    setTestSnapshots((current) => ({
      ...current,
      [server.id]: {
        status: "error",
        toolCount: 0,
        testedAt: new Date().toISOString(),
        error: message,
      },
    }));
    return message;
  };

  const testServer = async (server: LoomMcpServer) => {
    if (isArtManagedServer(server)) return;
    if (serverOperationRef.current) return;
    serverOperationRef.current = true;
    setBusyServerId(server.id);
    try {
      const result = await testMcpConnection(baseUrl, server);
      const toolCount = recordTestResult(server, result);
      notify(
        result.success === false ? "error" : "info",
        result.success === false
          ? `${server.name} 连接失败：${result.error || "未知错误"}`
          : `${server.name} 已连接，发现 ${toolCount} 个工具。`,
      );
    } catch (error) {
      const message = recordTestError(server, error);
      notify("error", `${server.name} 连接失败：${message}`);
    } finally {
      serverOperationRef.current = false;
      setBusyServerId(null);
    }
  };

  const persistServer = async (server: LoomMcpServer, testAfterSave: boolean) => {
    if (isArtManagedServer(server)) return;
    if (serverOperationRef.current) return;
    serverOperationRef.current = true;
    setBusyServerId(server.id);
    let saved: LoomMcpServer;
    try {
      saved = await saveMcpServer(baseUrl, server);
    } catch (error) {
      notify("error", error instanceof Error ? error.message : "无法保存 MCP 服务。");
      serverOperationRef.current = false;
      setBusyServerId(null);
      setBusyMarketplaceId(null);
      return;
    }

    setEditor(null);
    try {
      await refresh();
    } catch (error) {
      notify("warning", error instanceof Error ? `${saved.name} 已保存，但列表刷新失败：${error.message}` : `${saved.name} 已保存，但列表刷新失败。`);
    }

    try {
      if (testAfterSave && saved.enabled !== false) {
        try {
          const result = await testMcpConnection(baseUrl, saved);
          const toolCount = recordTestResult(saved, result);
          notify(
            result.success === false ? "error" : "info",
            result.success === false
              ? `${saved.name} 已安装，但连接测试失败：${result.error || "未知错误"}`
              : `${saved.name} 已安装，发现 ${toolCount} 个工具。`,
          );
        } catch (error) {
          const message = recordTestError(saved, error);
          notify("error", `${saved.name} 已安装，但连接测试失败：${message}`);
        }
      } else {
        notify("info", `${saved.name} 已保存。`);
      }
    } finally {
      serverOperationRef.current = false;
      setBusyServerId(null);
      setBusyMarketplaceId(null);
    }
  };

  const installMarketplaceServer = async (marketItem: McpMarketServer) => {
    const existing = findInstalledMcpServer(servers, marketItem);
    if (existing) return;
    const server = buildMarketplaceServerConfig(marketItem);
    const health = getMarketplaceHealth(marketItem, server);
    if (marketItem.installOptions.length > 1 || marketItem.requiresManualConfiguration || !health.requiredEnvPresent) {
      setEditor({ mode: "install", server, marketItem });
      return;
    }
    setBusyMarketplaceId(marketItem.id);
    await persistServer(server, true);
  };

  const toggleServer = async (server: LoomMcpServer) => {
    if (isArtManagedServer(server)) return;
    await persistServer({ ...server, enabled: server.enabled === false }, false);
  };

  const removeServer = async (server: LoomMcpServer) => {
    if (isArtManagedServer(server)) return;
    if (serverOperationRef.current) return;
    serverOperationRef.current = true;
    setBusyServerId(server.id);
    try {
      if (!await confirmRemove(server)) return;
      await deleteMcpServer(baseUrl, server.id);
      notify("info", `${server.name} 已删除。`);
      await refresh();
    } catch (error) {
      notify("error", error instanceof Error ? error.message : "无法删除 MCP 服务。");
    } finally {
      serverOperationRef.current = false;
      setBusyServerId(null);
    }
  };

  const normalizedServiceSearch = serviceSearchText.trim().toLowerCase();
  const normalizedStoreSearch = storeSearchText.trim().toLowerCase();
  const filteredServers = useMemo(() => servers.filter((server) => !normalizedServiceSearch ||
    server.name.toLowerCase().includes(normalizedServiceSearch) ||
    server.id.toLowerCase().includes(normalizedServiceSearch) ||
    (server.serverId || "").toLowerCase().includes(normalizedServiceSearch) ||
    (server.ownerArtId || "").toLowerCase().includes(normalizedServiceSearch) ||
    (server.toolName || "").toLowerCase().includes(normalizedServiceSearch) ||
    (server.description || "").toLowerCase().includes(normalizedServiceSearch)), [normalizedServiceSearch, servers]);
  const filteredMarketServers = useMemo(() => MCP_MARKET_SERVERS.filter((server) => {
    const matchesSearch = !normalizedStoreSearch || server.name.toLowerCase().includes(normalizedStoreSearch) ||
      server.id.toLowerCase().includes(normalizedStoreSearch) || server.description.toLowerCase().includes(normalizedStoreSearch);
    return matchesSearch && (marketCategory === "All" || server.category === marketCategory);
  }), [marketCategory, normalizedStoreSearch]);
  const marketTotalPages = Math.ceil(filteredMarketServers.length / MCP_STORE_PAGE_SIZE);
  const resolvedMarketPage = marketTotalPages > 0 ? Math.min(marketPage, marketTotalPages) : 1;
  const pagedMarketServers = useMemo(() => {
    const offset = (resolvedMarketPage - 1) * MCP_STORE_PAGE_SIZE;
    return filteredMarketServers.slice(offset, offset + MCP_STORE_PAGE_SIZE);
  }, [filteredMarketServers, resolvedMarketPage]);
  const paginationItems = useMemo(
    () => buildMcpPaginationItems(resolvedMarketPage, Math.max(1, marketTotalPages)),
    [marketTotalPages, resolvedMarketPage],
  );

  useEffect(() => {
    if (marketTotalPages > 0 && marketPage > marketTotalPages) setMarketPage(marketTotalPages);
  }, [marketPage, marketTotalPages]);

  const selectAdjacentWorkspace = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const lastIndex = workspaceItems.length - 1;
    const nextIndex = event.key === "Home" ? 0 : event.key === "End" ? lastIndex :
      event.key === "ArrowRight" ? (index + 1) % workspaceItems.length : (index - 1 + workspaceItems.length) % workspaceItems.length;
    setActiveWorkspace(workspaceItems[nextIndex].id);
    tabRefs.current[nextIndex]?.focus();
  };

  return (
    <section className="art-hub mcp-hub" aria-label="MCP">
      <div className="art-hub__navigation">
        <div className="art-hub__tabs art-hub__tabs--with-filter mcp-hub__tabs" role="tablist" aria-label="MCP 工作区">
          {workspaceItems.map((item, index) => {
            const active = activeWorkspace === item.id;
            return (
              <button
                key={item.id}
                ref={(element) => { tabRefs.current[index] = element; }}
                id={`mcp-tab-${item.id}`}
                className={active ? "art-hub__tab art-hub__tab--active" : "art-hub__tab"}
                type="button"
                role="tab"
                aria-selected={active}
                aria-controls={`mcp-panel-${item.id}`}
                tabIndex={active ? 0 : -1}
                onClick={() => setActiveWorkspace(item.id)}
                onKeyDown={(event) => selectAdjacentWorkspace(event, index)}
              >
                <span>{item.label}</span>
              </button>
            );
          })}
        </div>

        <div className="framework-filter mcp-hub__toolbar">
          <div className="mcp-hub__toolbar-primary">
            {activeWorkspace === "services" ? (
              <button className="ghost-button" type="button" disabled={operationBusy} onClick={() => setEditor({
                mode: "create",
                server: {
                  id: "local-mcp-server",
                  name: "本地 MCP 服务",
                  description: "",
                  transport: "stdio",
                  command: "npx",
                  args: ["-y", "@modelcontextprotocol/server-memory"],
                  env: {},
                  url: "",
                  headers: {},
                  enabled: true,
                },
              })}>添加 MCP</button>
            ) : (
              <button
                className="ghost-button"
                type="button"
                disabled={operationBusy}
                onClick={() => setEditor({ mode: "link", server: createRemoteMcpDraft() })}
              >
                链接添加
              </button>
            )}
          </div>
          <div className="framework-filter__actions mcp-hub__toolbar-actions">
            <input
              className="framework-filter__search"
              type="search"
              aria-label={activeWorkspace === "services" ? "搜索 MCP 服务" : "搜索 MCP 商店"}
              placeholder={activeWorkspace === "services" ? "搜索服务" : "搜索商店"}
              value={activeWorkspace === "services" ? serviceSearchText : storeSearchText}
              onChange={(event) => activeWorkspace === "services"
                ? setServiceSearchText(event.target.value)
                : setStoreSearchText(event.target.value)}
            />
            {activeWorkspace === "store" ? (
              <>
                <select
                  className="studio-input mcp-hub__category"
                  aria-label="MCP 商店分类"
                  value={marketCategory}
                  onChange={(event) => {
                    setMarketCategory(event.target.value as McpMarketCategory | "All");
                    setMarketPage(1);
                  }}
                >
                  <option value="All">全部</option>
                  {MCP_MARKET_CATEGORIES.map((category) => (
                    <option key={category} value={category}>{mcpMarketCategoryLabel(category)}</option>
                  ))}
                </select>
              </>
            ) : null}
          </div>
        </div>
      </div>

      <div
        className="art-hub__surface mcp-hub__surface"
        id="mcp-panel-services"
        role="tabpanel"
        aria-labelledby="mcp-tab-services"
        aria-busy={busyServerId !== null}
        hidden={activeWorkspace !== "services"}
      >
        <div className="art-registry-grid mcp-card-grid">
          {filteredServers.length ? filteredServers.map((server) => {
            const enabled = server.enabled !== false;
            const artManaged = isArtManagedServer(server);
            const snapshot = testSnapshots[server.id];
            const busy = busyServerId === server.id;
            return (
              <article
                className={`glass-card art-registry-card mcp-service-card ${enabled ? "art-registry-card--enabled" : "art-registry-card--disabled"}${busy ? " mcp-service-card--busy" : ""}`}
                key={server.id}
                title={artManaged ? "由 Art 管理" : enabled ? "已启用" : "已禁用"}
                aria-label={`${server.name}，${artManaged ? "由 Art 管理" : enabled ? "已启用" : "已禁用"}`}
                aria-busy={busy}
              >
                <div className="art-registry-card__head">
                  <h3>{server.name}</h3>
                  <span className="art-registry-card__framework-icon mcp-service-card__icon" title="MCP" aria-label="MCP">
                    <McpIcon kind="plug" />
                    {snapshot ? (
                      <i
                        className={`mcp-service-card__test-dot mcp-service-card__test-dot--${snapshot.status}`}
                        role="img"
                        title={snapshot.status === "success" ? `已发现 ${snapshot.toolCount} 个工具` : "连接失败"}
                        aria-label={snapshot.status === "success" ? `已发现 ${snapshot.toolCount} 个工具` : "连接失败"}
                      />
                    ) : null}
                  </span>
                </div>
                <div className="mcp-card__body">
                  <p className="art-registry-card__description" title={server.description || "暂无描述"}>{server.description || "暂无描述"}</p>
                  <p className="mcp-card__identity" title={artManaged ? `${server.ownerArtId} · ${server.serverId || server.id} · ${server.toolName}` : server.transport === "streamable-http" ? `${server.id} · ${server.url}` : `${server.id} · ${server.command} ${(server.args || []).join(" ")}`}>
                    {artManaged ? `Art · ${server.serverId || server.id} · ${server.toolName || "MCP tool"}` : server.transport === "streamable-http" ? `远程 · ${server.url}` : `本地 · ${server.command}`}
                  </p>
                  {artManaged ? (
                    <p
                      className={`mcp-card__state ${server.credentialRequired && !server.credentialBound ? "mcp-card__state--error" : "mcp-card__state--success"}`}
                      role="status"
                    >
                      {server.credentialRequired
                        ? server.credentialBound
                          ? "由 Art 管理 · 凭据已绑定"
                          : "由 Art 管理 · 凭据待绑定"
                        : "由 Art 管理 · 无需凭据"}
                    </p>
                  ) : busy ? (
                    <p className="mcp-card__state mcp-card__state--loading" role="status"><span className="mcp-busy-indicator" aria-hidden="true" />处理中</p>
                  ) : snapshot ? (
                    <p
                      className={`mcp-card__state mcp-card__state--${snapshot.status}`}
                      role={snapshot.status === "error" ? "alert" : "status"}
                      title={snapshot.status === "success" ? `已发现 ${snapshot.toolCount} 个工具` : snapshot.error || "连接失败"}
                    >
                      {snapshot.status === "success" ? `${snapshot.toolCount} 个工具` : snapshot.error || "连接失败"}
                    </p>
                  ) : null}
                </div>
                <div className="art-registry-card__actions">
                  {artManaged ? (
                    <span className="mcp-card__state" title={server.ownerArtId}>只读 · 请在 Art 管理中配置</span>
                  ) : (
                    <div className="mcp-card__action-buttons">
                      <button className="art-card-action" type="button" aria-label={`编辑 ${server.name}`} title="编辑" disabled={operationBusy} onClick={() => setEditor({ mode: "edit", server })}>
                        <McpIcon kind="edit" />
                      </button>
                      <button className="art-card-action" type="button" aria-label={`测试 ${server.name}`} title="测试连接" disabled={operationBusy || !enabled} onClick={() => void testServer(server)}>
                        <McpIcon kind="test" />
                      </button>
                      <button className={enabled ? "art-card-action art-card-action--active" : "art-card-action"} type="button" aria-label={`${enabled ? "禁用" : "启用"} ${server.name}`} title={enabled ? "禁用" : "启用"} disabled={operationBusy} onClick={() => void toggleServer(server)}>
                        <McpIcon kind="power" />
                      </button>
                      <button className="art-card-action art-card-action--danger" type="button" aria-label={`删除 ${server.name}`} title="删除" disabled={operationBusy} onClick={() => void removeServer(server)}>
                        <McpIcon kind="trash" />
                      </button>
                    </div>
                  )}
                </div>
              </article>
            );
          }) : <div className="mcp-hub__empty">暂无 MCP 服务</div>}
        </div>
      </div>

      <div
        className="art-hub__surface mcp-hub__surface"
        id="mcp-panel-store"
        role="tabpanel"
        aria-labelledby="mcp-tab-store"
        hidden={activeWorkspace !== "store"}
      >
        <div className="art-store-grid mcp-card-grid">
          {pagedMarketServers.length ? pagedMarketServers.map((marketItem) => {
            const configured = findInstalledMcpServer(servers, marketItem);
            const configuredSnapshot = configured ? testSnapshots[configured.id] : undefined;
            const configuredBusy = configured ? busyServerId === configured.id : false;
            const server = buildMarketplaceServerConfig(marketItem, configured);
            const health = getMarketplaceHealth(marketItem, server, configuredSnapshot);
            const requiresConfiguration = marketItem.requiresManualConfiguration || !health.requiredEnvPresent;
            const sourceTone = configured ? "installed" : marketItem.sourceKind;
            const sourceLabel = configured ? "已安装" : marketItem.sourceKind === "registry" ? "官方仓库" : "Loom 精选";
            const categoryLabel = mcpMarketCategoryLabel(marketItem.category);
            const connectionLabel = marketItem.installOptions.length > 1
              ? `${marketItem.installOptions.length} 种连接方式`
              : marketItem.installOptions[0]?.label || "待配置";
            const identityLabel = connectionLabel.startsWith(`${categoryLabel} ·`)
              ? connectionLabel
              : `${categoryLabel} · ${connectionLabel}`;
            return (
              <article className={`glass-card art-store-card mcp-store-card mcp-store-card--${sourceTone}`} key={marketItem.id}>
                <div className="art-store-card__head">
                  <h3>{marketItem.name}</h3>
                  <div className="art-store-card__badges">
                    <span className={`mcp-store-card__source mcp-store-card__source--${sourceTone}`}>
                      {sourceLabel}
                    </span>
                    <span className="mcp-store-card__icon" title="MCP" aria-label="MCP"><McpIcon kind="plug" /></span>
                  </div>
                </div>
                <div className="art-store-card__body">
                  <p className="art-store-card__description" title={marketItem.description}>{marketItem.description}</p>
                  <p className="art-store-card__identity" title={marketItem.installOptions.map((option) => option.label).join(" / ")}>
                    {identityLabel}
                  </p>
                </div>
                <div className="art-store-card__actions mcp-store-card__actions">
                  <button className="art-card-action" type="button" aria-label={`打开 ${marketItem.name} 介绍`} title="打开介绍" onClick={() => void openMarketplaceSource(marketItem)}>
                    <McpIcon kind="external" />
                  </button>
                  {requiresConfiguration && !configured ? (
                    <button
                      className="mcp-store-card__configuration"
                      type="button"
                      aria-label={`配置 ${marketItem.name}`}
                      disabled={operationBusy}
                      onClick={() => setEditor({ mode: "install", server, marketItem })}
                    >
                      安装前需配置
                    </button>
                  ) : null}
                  {configured ? (
                    <button
                      className="signal-button mcp-store-card__install"
                      type="button"
                      aria-busy={configuredBusy}
                      aria-label={`${configuredSnapshot?.status === "error" ? "重试连接" : "连接"} ${marketItem.name}`}
                      title={configured.enabled === false ? "请先在服务页面启用" : "测试连接"}
                      disabled={operationBusy || configured.enabled === false}
                      onClick={() => void testServer(configured)}
                    >
                      {configuredBusy
                        ? <><span className="mcp-busy-indicator" aria-hidden="true" />连接中</>
                        : configuredSnapshot?.status === "success"
                          ? "已连接"
                          : configuredSnapshot?.status === "error"
                            ? "重试"
                            : configured.enabled === false ? "已禁用" : "连接"}
                    </button>
                  ) : (
                    <button
                      className="signal-button mcp-store-card__install"
                      type="button"
                      aria-busy={busyMarketplaceId === marketItem.id}
                      disabled={operationBusy}
                      onClick={() => void installMarketplaceServer(marketItem)}
                    >
                      {busyMarketplaceId === marketItem.id ? <><span className="mcp-busy-indicator" aria-hidden="true" />安装中</> : "安装"}
                    </button>
                  )}
                </div>
              </article>
            );
          }) : <div className="mcp-hub__empty">没有匹配的 MCP 服务</div>}
        </div>
        <nav className="mcp-hub__pagination" aria-label="MCP 商店分页">
          <span className="mcp-hub__pagination-summary" aria-live="polite">
            {marketTotalPages > 0
                ? `共 ${filteredMarketServers.length} 个 · 第 ${resolvedMarketPage}/${marketTotalPages} 页`
                : "共 0 个"}
          </span>
          {marketTotalPages > 1 ? (
            <div className="mcp-hub__pagination-controls">
              <button className="ghost-button" type="button" aria-label="上一页" disabled={resolvedMarketPage <= 1} onClick={() => setMarketPage((page) => Math.max(1, page - 1))}>上一页</button>
              {paginationItems.map((item) => typeof item === "number" ? (
                <button
                  className={item === resolvedMarketPage ? "mcp-hub__page mcp-hub__page--active" : "mcp-hub__page"}
                  type="button"
                  key={item}
                  aria-label={`第 ${item} 页`}
                  aria-current={item === resolvedMarketPage ? "page" : undefined}
                  onClick={() => setMarketPage(item)}
                >
                  {item}
                </button>
              ) : <span className="mcp-hub__page-ellipsis" aria-hidden="true" key={item}>…</span>)}
              <button className="ghost-button" type="button" aria-label="下一页" disabled={resolvedMarketPage >= marketTotalPages} onClick={() => setMarketPage((page) => Math.min(marketTotalPages, page + 1))}>下一页</button>
            </div>
          ) : null}
        </nav>
      </div>

      <McpServerDialog
        editor={editor}
        busy={editorBusy}
        onClose={closeEditor}
        onSave={async (server) => {
          setEditorBusy(true);
          try {
            await persistServer(server, editor?.mode === "install" || editor?.mode === "link");
          } finally {
            setEditorBusy(false);
          }
        }}
      />
    </section>
  );
}
