// Owns credential mutation inputs without exposing persisted secret values.

import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

import type { LoomMcpServer } from "../../services/loomApi";
import { McpIcon } from "./McpIcon";

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
  const dialogRef = useRef<HTMLElement | null>(null);
  const firstInputRef = useRef<HTMLInputElement | null>(null);
  const busyRef = useRef(busy);
  const [values, setValues] = useState<Record<string, string>>({});
  const [clear, setClear] = useState<Set<string>>(new Set());
  busyRef.current = busy;

  useEffect(() => {
    if (!server) return;
    setValues({});
    setClear(new Set());
    const previousOverflow = document.body.style.overflow;
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    document.body.style.overflow = "hidden";
    const timer = window.setTimeout(() => firstInputRef.current?.focus(), 0);
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape" && !busyRef.current) {
        event.preventDefault();
        onClose();
      }
      if (event.key !== "Tab") return;
      const focusable = Array.from(dialogRef.current?.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ) ?? []);
      if (!focusable.length) return;
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
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("keydown", handleKeyDown);
      document.body.style.overflow = previousOverflow;
      previousFocus?.focus();
    };
  }, [onClose, server]);

  if (!server) return null;
  const requirements = server.credentialRequirements || [];
  const enteredValues = Object.fromEntries(Object.entries(values).filter(([, value]) => value.length > 0));
  const canSave = Object.keys(enteredValues).length > 0 || clear.size > 0;
  return createPortal(
    <div className="framework-dialog-backdrop" role="presentation" onMouseDown={(event) => {
      if (event.target === event.currentTarget && !busy) onClose();
    }}>
      <section ref={dialogRef} className="framework-dialog mcp-server-dialog" role="dialog" aria-modal="true" aria-labelledby="mcp-credential-dialog-title" aria-busy={busy}>
        <header className="framework-dialog__header">
          <div>
            <h2 id="mcp-credential-dialog-title">配置 MCP 凭据</h2>
            <p>{server.name} · {server.package?.qualifiedId || server.id}</p>
          </div>
          <button className="icon-button" type="button" aria-label="关闭" title="关闭" disabled={busy} onClick={onClose}><McpIcon kind="close" /></button>
        </header>
        <form className="mcp-server-dialog__form" onSubmit={(event) => {
          event.preventDefault();
          if (!canSave || busy) return;
          void onSave(enteredValues, [...clear]);
        }}>
          <div className="mcp-server-dialog__scroll">
            <p className="mcp-server-dialog__credential-note">凭据由 Loom CredentialStore 加密保存；此页面不会回显已保存的值。</p>
            {requirements.length ? requirements.map((requirement, index) => (
              <div className="mcp-server-dialog__credential" key={requirement.id}>
                <label>
                  <span>{requirement.label}{requirement.required ? " · 必填" : " · 可选"}</span>
                  <input
                    ref={index === 0 ? firstInputRef : undefined}
                    className="studio-input"
                    type="password"
                    autoComplete="off"
                    value={values[requirement.id] || ""}
                    placeholder={server.credentialBound ? "已保存；留空保持不变" : "输入凭据"}
                    disabled={clear.has(requirement.id)}
                    onChange={(event) => setValues((current) => ({ ...current, [requirement.id]: event.target.value }))}
                  />
                </label>
                <label className="mcp-server-dialog__enabled">
                  <input
                    type="checkbox"
                    checked={clear.has(requirement.id)}
                    onChange={(event) => setClear((current) => {
                      const next = new Set(current);
                      if (event.target.checked) next.add(requirement.id); else next.delete(requirement.id);
                      return next;
                    })}
                  />
                  <span>清除此凭据</span>
                </label>
              </div>
            )) : <p className="muted-line">此 MCP 服务没有声明凭据槽位。</p>}
          </div>
          <footer className="mcp-server-dialog__actions">
            <button className="ghost-button" type="button" disabled={busy} onClick={onClose}>取消</button>
            <button className="signal-button" type="submit" disabled={busy || !canSave}>{busy ? "保存中" : "保存凭据"}</button>
          </footer>
        </form>
      </section>
    </div>,
    document.body,
  );
}
