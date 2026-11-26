// Owns MCP server draft editing and validation; standard UI owns modal lifecycle.

import { useEffect, useRef, useState } from "react";
import { Button, Checkbox, Dialog, Field, Input, Notice, Select, Textarea } from "@neuro/ui";

import { useUiText } from "../../i18n/index.ts";
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
  const { t, tf } = useUiText();
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
      ? t("mcp.dialog.title.create")
      : editor.mode === "install"
        ? t("mcp.dialog.title.install")
        : editor.mode === "link"
          ? t("mcp.dialog.title.link")
          : t("mcp.dialog.title.edit");
  const separator = t("mcp.listSeparator");
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
            <Field label={t("mcp.dialog.field.name")}>
              <Input ref={nameInputRef} value={name} onChange={(event) => setName(event.target.value)} />
            </Field>
            <Field label={t("mcp.dialog.field.id")}>
              <Input value={id} disabled={editor.mode !== "create"} onChange={(event) => setId(event.target.value)} />
            </Field>
          </div>
          <Field label={t("mcp.dialog.field.description")}>
            <Input value={description} onChange={(event) => setDescription(event.target.value)} />
          </Field>
          <Field label={t("mcp.dialog.field.transport")}>
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
                <option value="stdio">{t("mcp.installOption.localStdio")}</option>
                <option value="streamable-http">{t("mcp.installOption.remoteHttp")}</option>
              </Select>
            )}
          </Field>
          {transport === "stdio" ? (
            <>
              <Field label={t("mcp.dialog.field.command")}>
                <Input
                  value={command}
                  onChange={(event) => setCommand(event.target.value)}
                  placeholder={t("mcp.dialog.placeholder.command")}
                />
              </Field>
              <div className="mcp-server-dialog__grid">
                <Field label={t("mcp.dialog.field.args")}>
                  <Textarea
                    value={args}
                    aria-invalid={unresolvedArguments}
                    aria-describedby={unresolvedArguments ? "mcp-server-unresolved-arguments" : undefined}
                    onChange={(event) => setArgs(event.target.value)}
                    placeholder={t("mcp.dialog.placeholder.args")}
                  />
                </Field>
                <Field label={t("mcp.dialog.field.environment")}>
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
                    placeholder={t("mcp.dialog.placeholder.environment")}
                  />
                </Field>
              </div>
            </>
          ) : (
            <>
              <Field label={t("mcp.dialog.field.url")}>
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
              <Field label={t("mcp.dialog.field.headers")}>
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
                  placeholder={t("mcp.dialog.placeholder.headers")}
                />
              </Field>
            </>
          )}
          {missingEnvironmentKeys.length > 0 ? (
            <Notice id="mcp-server-missing-environment" tone="warning">
              {tf("mcp.dialog.notice.missingEnvironment", { keys: missingEnvironmentKeys.join(separator) })}
            </Notice>
          ) : null}
          {unresolvedArguments ? (
            <Notice id="mcp-server-unresolved-arguments" tone="warning">
              {t("mcp.dialog.notice.unresolvedArguments")}
            </Notice>
          ) : null}
          {missingHeaderKeys.length > 0 ? (
            <Notice id="mcp-server-missing-headers" tone="warning">
              {tf("mcp.dialog.notice.missingHeaders", { keys: missingHeaderKeys.join(separator) })}
            </Notice>
          ) : null}
          {unresolvedRemoteUrl ? (
            <Notice id="mcp-server-unresolved-url" tone="warning">
              {t("mcp.dialog.notice.unresolvedUrl")}
            </Notice>
          ) : null}
          {invalidRemoteUrl ? (
            <Notice id="mcp-server-invalid-url" tone="warning">
              {t("mcp.dialog.notice.invalidUrl")}
            </Notice>
          ) : null}
          {invalidEnvironmentLines.length > 0 ? (
            <Notice id="mcp-server-invalid-environment" tone="warning">
              {tf("mcp.dialog.notice.invalidEnvironment", { lines: invalidEnvironmentLines.join(separator) })}
            </Notice>
          ) : null}
          {invalidHeaderLines.length > 0 ? (
            <Notice id="mcp-server-invalid-headers" tone="warning">
              {tf("mcp.dialog.notice.invalidHeaders", { lines: invalidHeaderLines.join(separator) })}
            </Notice>
          ) : null}
          <Checkbox
            label={t("mcp.action.enable")}
            checked={enabled}
            onChange={(event) => setEnabled(event.target.checked)}
          />
        </div>
        <footer className="mcp-server-dialog__actions">
          <Button variant="quiet" disabled={busy} onClick={onClose}>
            {t("mcp.action.cancel")}
          </Button>
          <Button variant="primary" type="submit" loading={busy} disabled={!canSave}>
            {editor.mode === "install" ? t("mcp.action.install") : t("mcp.action.save")}
          </Button>
        </footer>
      </form>
    </Dialog>
  );
}
