// Owns credential mutation inputs without exposing persisted secret values.

import { Button, Checkbox, Dialog, EmptyState, Field, Input, Notice } from "@neuro/ui";
import { useEffect, useRef, useState } from "react";

import type { LoomMcpServer } from "../../services/loomApi";

export function McpCredentialDialog({
  server,
  busy,
  onClose,
  onSave,
}: {
  server: LoomMcpServer | null;
  busy: boolean;
  onClose: () => void;
  onSave: (values: Record<string, string>, clear: string[]) => Promise<void>;
}) {
  const firstInputRef = useRef<HTMLInputElement | null>(null);
  const [values, setValues] = useState<Record<string, string>>({});
  const [clear, setClear] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (!server) return;
    setValues({});
    setClear(new Set());
  }, [server]);

  if (!server) return null;
  const requirements = server.credentialRequirements || [];
  const enteredValues = Object.fromEntries(Object.entries(values).filter(([, value]) => value.length > 0));
  const canSave = Object.keys(enteredValues).length > 0 || clear.size > 0;
  return (
    <Dialog
      open
      title="配置 MCP 凭据"
      description={`${server.name} · ${server.package?.qualifiedId || server.id}`}
      className="loom-mcp-dialog"
      busy={busy}
      initialFocusRef={firstInputRef}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <form
        className="mcp-server-dialog__form"
        onSubmit={(event) => {
          event.preventDefault();
          if (!canSave || busy) return;
          void onSave(enteredValues, [...clear]);
        }}
      >
        <div className="mcp-server-dialog__scroll">
          <Notice tone="info">凭据由 Loom CredentialStore 加密保存；此页面不会回显已保存的值。</Notice>
          {requirements.length ? (
            requirements.map((requirement, index) => (
              <div className="mcp-server-dialog__credential" key={requirement.id}>
                <Field label={`${requirement.label}${requirement.required ? " · 必填" : " · 可选"}`}>
                  <Input
                    ref={index === 0 ? firstInputRef : undefined}
                    type="password"
                    autoComplete="off"
                    value={values[requirement.id] || ""}
                    placeholder={server.credentialBound ? "已保存；留空保持不变" : "输入凭据"}
                    disabled={clear.has(requirement.id)}
                    onChange={(event) => setValues((current) => ({ ...current, [requirement.id]: event.target.value }))}
                  />
                </Field>
                <Checkbox
                  label="清除此凭据"
                  checked={clear.has(requirement.id)}
                  onChange={(event) =>
                    setClear((current) => {
                      const next = new Set(current);
                      if (event.target.checked) next.add(requirement.id);
                      else next.delete(requirement.id);
                      return next;
                    })
                  }
                />
              </div>
            ))
          ) : (
            <EmptyState title="此 MCP 服务没有声明凭据槽位。" />
          )}
        </div>
        <footer className="mcp-server-dialog__actions">
          <Button variant="quiet" disabled={busy} onClick={onClose}>
            取消
          </Button>
          <Button variant="primary" type="submit" loading={busy} disabled={!canSave}>
            保存凭据
          </Button>
        </footer>
      </form>
    </Dialog>
  );
}
