// Owns MCP server draft editing, validation and modal focus lifecycle.

import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

import type { LoomMcpServer } from "../../services/loomApi";
import { isValidMcpRemoteUrl, parseMcpKeyValueLines } from "../../services/mcpMarketplace";
import { McpIcon } from "./McpIcon";
import type { McpEditorState } from "./McpHubTypes";

const parseLines = (value: string) => value
  .split(/\r?\n/)
  .map((line) => line.trim())
  .filter(Boolean);

const environmentText = (environment: Record<string, string> = {}) => Object.entries(environment)
  .map(([key, value]) => `${key}=${value}`)
  .join("\n");

const normalizedServerId = (value: string) => value
  .trim()
  .replace(/[^a-zA-Z0-9_.@/-]/g, "-")
  .replace(/^-+|-+$/g, "") || "local-mcp-server";

export function McpServerDialog({
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
