// Owns credential mutation inputs without exposing persisted secret values.

import { Button, Checkbox, Dialog, EmptyState, Field, Input, Notice } from "@neuro/ui";
import { useEffect, useRef, useState } from "react";

import { useUiText } from "../../i18n/index.ts";
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
  const { t, tf } = useUiText();
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
      title={t("mcp.credentials.title")}
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
          <Notice tone="info">{t("mcp.credentials.notice")}</Notice>
          {requirements.length ? (
            requirements.map((requirement, index) => (
              <div className="mcp-server-dialog__credential" key={requirement.id}>
                <Field
                  label={tf(
                    requirement.required ? "mcp.credentials.fieldRequired" : "mcp.credentials.fieldOptional",
                    { label: requirement.label },
                  )}
                >
                  <Input
                    ref={index === 0 ? firstInputRef : undefined}
                    type="password"
                    autoComplete="off"
                    value={values[requirement.id] || ""}
                    placeholder={
                      server.credentialBound
                        ? t("mcp.credentials.placeholderSaved")
                        : t("mcp.credentials.placeholderEnter")
                    }
                    disabled={clear.has(requirement.id)}
                    onChange={(event) => setValues((current) => ({ ...current, [requirement.id]: event.target.value }))}
                  />
                </Field>
                <Checkbox
                  label={t("mcp.credentials.clear")}
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
            <EmptyState title={t("mcp.credentials.empty")} />
          )}
        </div>
        <footer className="mcp-server-dialog__actions">
          <Button variant="quiet" disabled={busy} onClick={onClose}>
            {t("mcp.action.cancel")}
          </Button>
          <Button variant="primary" type="submit" loading={busy} disabled={!canSave}>
            {t("mcp.credentials.save")}
          </Button>
        </footer>
      </form>
    </Dialog>
  );
}
