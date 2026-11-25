// Owns MCP server draft editing and validation; standard UI owns modal lifecycle.

import { useEffect, useRef, useState } from "react";
import { Button, Checkbox, Dialog, Field, Input, Notice, Select, Textarea } from "@neuro/ui";

import type { LoomMcpServer } from "../../services/loomApi";
import { isValidMcpRemoteUrl, parseMcpKeyValueLines } from "../../services/mcpMarketplace";
import type { McpEditorState } from "./McpHubTypes";

const parseLines = (value: string) =>
  value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);

const environmentText = (environment: Record<string, string> = {}) =>
  Object.entries(environment)
    .map(([key, value]) => `${key}=${value}`)
    .join("\n");

const normalizedServerId = (value: string) =>
  value
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
  const nameInputRef = useRef<HTMLInputElement | null>(null);
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
    const selectedOption =
      editor.marketItem?.installOptions.find(
        (option) =>
          option.transport === editor.server.transport &&
          (option.transport === "stdio" ? option.command === editor.server.command : option.url === editor.server.url),
      ) ||
      editor.marketItem?.installOptions.find((option) => option.transport === editor.server.transport) ||
      editor.marketItem?.installOptions[0];
    setInstallOptionId(selectedOption?.id || "");
    setEnabled(editor.server.enabled !== false);
  }, [editor]);

  if (!editor) return null;

  const parsedEnvironment = parseMcpKeyValueLines(environment);
  const parsedHeaders = parseMcpKeyValueLines(headers);
  const invalidEnvironmentLines = transport === "stdio" ? parsedEnvironment.invalidLineNumbers : [];
  const invalidHeaderLines = transport === "streamable-http" ? parsedHeaders.invalidLineNumbers : [];
  const selectedInstallOption = editor.marketItem?.installOptions.find((option) => option.id === installOptionId);
  const missingEnvironmentKeys = (selectedInstallOption?.requiredEnvKeys || []).filter(
    (key) => !parsedEnvironment.values[key]?.trim(),
  );
  const missingHeaderKeys = (selectedInstallOption?.requiredHeaderKeys || []).filter(
    (key) => !parsedHeaders.values[key]?.trim(),
  );
  const unresolvedArguments =
    transport === "stdio" &&
    selectedInstallOption?.requiresManualConfiguration === true &&
    parseLines(args).some((argument) => /^<[^>]+>$/.test(argument));
  const unresolvedRemoteUrl = transport === "streamable-http" && /\{[^}]+\}/.test(url);
  const invalidRemoteUrl =
    transport === "streamable-http" && Boolean(url.trim()) && !unresolvedRemoteUrl && !isValidMcpRemoteUrl(url);
  const canSave =
    Boolean(id.trim() && name.trim() && (transport === "stdio" ? command.trim() : url.trim())) &&
    invalidEnvironmentLines.length === 0 &&
    invalidHeaderLines.length === 0 &&
    missingEnvironmentKeys.length === 0 &&
    missingHeaderKeys.length === 0 &&
    !unresolvedArguments &&
    !unresolvedRemoteUrl &&
    !invalidRemoteUrl;
  const title =
    editor.mode === "create"
      ? "添加 MCP"
      : editor.mode === "install"
        ? "安装 MCP"
        : editor.mode === "link"
          ? "链接添加"
          : "编辑 MCP";
  const validationDescription =
    [
      missingEnvironmentKeys.length > 0 ? "mcp-server-missing-environment" : null,
      missingHeaderKeys.length > 0 ? "mcp-server-missing-headers" : null,
      unresolvedArguments ? "mcp-server-unresolved-arguments" : null,
      unresolvedRemoteUrl ? "mcp-server-unresolved-url" : null,
      invalidRemoteUrl ? "mcp-server-invalid-url" : null,
      invalidEnvironmentLines.length > 0 ? "mcp-server-invalid-environment" : null,
      invalidHeaderLines.length > 0 ? "mcp-server-invalid-headers" : null,
    ]
      .filter(Boolean)
      .join(" ") || undefined;

  return (
    <Dialog
      open
      title={title}
      description={editor.marketItem?.installSource.packageName}
      className="loom-mcp-dialog"
      busy={busy}
      initialFocusRef={nameInputRef}
      aria-describedby={validationDescription}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <form
        className="mcp-server-dialog__form"
        onSubmit={(event) => {
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
        }}
      >
        <div className="mcp-server-dialog__scroll">
          <div className="mcp-server-dialog__grid">
            <Field label="名称">
              <Input ref={nameInputRef} value={name} onChange={(event) => setName(event.target.value)} />
            </Field>
            <Field label="ID">
              <Input value={id} disabled={editor.mode !== "create"} onChange={(event) => setId(event.target.value)} />
            </Field>
          </div>
          <Field label="描述">
            <Input value={description} onChange={(event) => setDescription(event.target.value)} />
          </Field>
          <Field label="连接方式">
            {editor.marketItem?.installOptions.length ? (
              <Select
                value={installOptionId}
                onChange={(event) => {
                  const option = editor.marketItem?.installOptions.find(
                    (candidate) => candidate.id === event.target.value,
                  );
                  if (!option) return;
                  setInstallOptionId(option.id);
                  setTransport(option.transport);
                  setCommand(option.command);
                  setArgs(option.args.join("\n"));
                  setEnvironment(environmentText(option.env));
                  setUrl(option.url);
                  setHeaders(environmentText(option.headers));
                }}
              >
                {editor.marketItem.installOptions.map((option) => (
                  <option key={option.id} value={option.id}>
                    {option.label}
                  </option>
                ))}
              </Select>
            ) : (
              <Select
                value={transport}
                onChange={(event) => setTransport(event.target.value as "stdio" | "streamable-http")}
              >
                <option value="stdio">本地 · stdio</option>
                <option value="streamable-http">远程 · Streamable HTTP</option>
              </Select>
            )}
          </Field>
          {transport === "stdio" ? (
            <>
              <Field label="启动命令">
                <Input
                  value={command}
                  onChange={(event) => setCommand(event.target.value)}
                  placeholder="npx、uvx、docker 或本地程序"
                />
              </Field>
              <div className="mcp-server-dialog__grid">
                <Field label="参数">
                  <Textarea
                    value={args}
                    aria-invalid={unresolvedArguments}
                    aria-describedby={unresolvedArguments ? "mcp-server-unresolved-arguments" : undefined}
                    onChange={(event) => setArgs(event.target.value)}
                    placeholder="每行一个参数"
                  />
                </Field>
                <Field label="环境变量">
                  <Textarea
                    value={environment}
                    aria-invalid={invalidEnvironmentLines.length > 0 || missingEnvironmentKeys.length > 0}
                    aria-describedby={
                      [
                        missingEnvironmentKeys.length > 0 ? "mcp-server-missing-environment" : null,
                        invalidEnvironmentLines.length > 0 ? "mcp-server-invalid-environment" : null,
                      ]
                        .filter(Boolean)
                        .join(" ") || undefined
                    }
                    onChange={(event) => setEnvironment(event.target.value)}
                    placeholder="KEY=value，每行一个"
                  />
                </Field>
              </div>
            </>
          ) : (
            <>
              <Field label="MCP 地址">
                <Input
                  value={url}
                  aria-invalid={unresolvedRemoteUrl || invalidRemoteUrl}
                  aria-describedby={
                    unresolvedRemoteUrl
                      ? "mcp-server-unresolved-url"
                      : invalidRemoteUrl
                        ? "mcp-server-invalid-url"
                        : undefined
                  }
                  onChange={(event) => setUrl(event.target.value)}
                  placeholder="https://example.com/mcp"
                />
              </Field>
              <Field label="请求头">
                <Textarea
                  value={headers}
                  aria-invalid={invalidHeaderLines.length > 0 || missingHeaderKeys.length > 0}
                  aria-describedby={
                    [
                      missingHeaderKeys.length > 0 ? "mcp-server-missing-headers" : null,
                      invalidHeaderLines.length > 0 ? "mcp-server-invalid-headers" : null,
                    ]
                      .filter(Boolean)
                      .join(" ") || undefined
                  }
                  onChange={(event) => setHeaders(event.target.value)}
                  placeholder="Authorization=Bearer ...，每行一个"
                />
              </Field>
            </>
          )}
          {missingEnvironmentKeys.length > 0 ? (
            <Notice id="mcp-server-missing-environment" tone="warning">
              需要填写：{missingEnvironmentKeys.join("、")}
            </Notice>
          ) : null}
          {unresolvedArguments ? (
            <Notice id="mcp-server-unresolved-arguments" tone="warning">
              请将参数中的占位符替换为实际值。
            </Notice>
          ) : null}
          {missingHeaderKeys.length > 0 ? (
            <Notice id="mcp-server-missing-headers" tone="warning">
              需要填写请求头：{missingHeaderKeys.join("、")}
            </Notice>
          ) : null}
          {unresolvedRemoteUrl ? (
            <Notice id="mcp-server-unresolved-url" tone="warning">
              请将地址中的变量替换为实际值。
            </Notice>
          ) : null}
          {invalidRemoteUrl ? (
            <Notice id="mcp-server-invalid-url" tone="warning">
              地址必须是有效的 HTTP 或 HTTPS 地址，且不能包含用户名或密码。
            </Notice>
          ) : null}
          {invalidEnvironmentLines.length > 0 ? (
            <Notice id="mcp-server-invalid-environment" tone="warning">
              环境变量第 {invalidEnvironmentLines.join("、")} 行格式错误，请使用 KEY=value。
            </Notice>
          ) : null}
          {invalidHeaderLines.length > 0 ? (
            <Notice id="mcp-server-invalid-headers" tone="warning">
              请求头第 {invalidHeaderLines.join("、")} 行格式错误，请使用 KEY=value。
            </Notice>
          ) : null}
          <Checkbox label="启用" checked={enabled} onChange={(event) => setEnabled(event.target.checked)} />
        </div>
        <footer className="mcp-server-dialog__actions">
          <Button variant="quiet" disabled={busy} onClick={onClose}>
            取消
          </Button>
          <Button variant="primary" type="submit" loading={busy} disabled={!canSave}>
            {editor.mode === "install" ? "安装" : "保存"}
          </Button>
        </footer>
      </form>
    </Dialog>
  );
}
