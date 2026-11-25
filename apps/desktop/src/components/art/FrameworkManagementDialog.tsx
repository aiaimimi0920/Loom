// Owns framework package installation, removal confirmation, and upgrades.
import { frameworkFilterLabel, frameworkIdentity } from "../../services/artHubUi";
import type { LoomFramework } from "../../services/loomApi";
import type { StudioMessage } from "../app/appShell";
import { useEffect, useRef, useState } from "react";
import { Button, DataTable, Dialog, EmptyState, Notice } from "@neuro/ui";

export type FrameworkBusyAction = "toggle" | "upgrade" | null;

export function readFrameworkPackageBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error("无法读取框架更新包。"));
    reader.onload = () => {
      const result = typeof reader.result === "string" ? reader.result : "";
      const separator = result.indexOf(",");
      if (separator < 0 || !result.slice(separator + 1)) {
        reject(new Error("框架更新包内容为空。"));
        return;
      }
      resolve(result.slice(separator + 1));
    };
    reader.readAsDataURL(file);
  });
}

export function FrameworkManagementDialog({
  open,
  frameworks,
  loading,
  busyId,
  busyAction,
  error,
  message,
  onClose,
  onToggle,
  onUpgrade,
}: {
  open: boolean;
  frameworks: LoomFramework[];
  loading: boolean;
  busyId: string | null;
  busyAction: FrameworkBusyAction;
  error: string | null;
  message: StudioMessage | null;
  onClose: () => void;
  onToggle: (framework: LoomFramework) => Promise<void>;
  onUpgrade: (framework: LoomFramework, file: File) => Promise<void>;
}) {
  const fileInputRefs = useRef<Record<string, HTMLInputElement | null>>({});
  const [pendingUninstallId, setPendingUninstallId] = useState<string | null>(null);

  useEffect(() => {
    if (!open) setPendingUninstallId(null);
  }, [open]);

  if (!open) return null;

  const installCell = (framework: LoomFramework) => {
    const identity = frameworkIdentity(framework);
    const toggleBusy = busyId === identity && busyAction === "toggle";
    const confirmingUninstall = framework.installed && pendingUninstallId === identity;
    return (
      <Button
        variant={framework.installed ? "secondary" : "primary"}
        type="button"
        loading={toggleBusy}
        disabled={busyId !== null}
        onClick={() => {
          if (!framework.installed) {
            void onToggle(framework);
            return;
          }
          if (!confirmingUninstall) {
            setPendingUninstallId(identity);
            return;
          }
          setPendingUninstallId(null);
          void onToggle(framework);
        }}
      >
        {confirmingUninstall ? "确认卸载" : framework.installed ? "卸载" : "安装"}
      </Button>
    );
  };
  const upgradeCell = (framework: LoomFramework) => {
    const identity = frameworkIdentity(framework);
    const upgradeBusy = busyId === identity && busyAction === "upgrade";
    return (
      <>
        <input
          hidden
          ref={(element) => {
            fileInputRefs.current[identity] = element;
          }}
          type="file"
          accept=".zip,application/zip"
          onChange={(event) => {
            const file = event.currentTarget.files?.[0];
            event.currentTarget.value = "";
            if (file) void onUpgrade(framework, file);
          }}
        />
        <Button
          type="button"
          loading={upgradeBusy}
          disabled={!framework.installed || busyId !== null}
          onClick={() => fileInputRefs.current[identity]?.click()}
        >
          更新
        </Button>
      </>
    );
  };

  return (
    <Dialog
      open={open}
      title="管理框架"
      className="loom-framework-dialog"
      onOpenChange={(next) => {
        if (!next) onClose();
      }}
    >
      {error || message ? (
        <div className="n-stack loom-framework-dialog__feedback">
          {error ? <Notice tone="danger">{error}</Notice> : null}
          {message ? <Notice tone={message.kind === "error" ? "danger" : "success"}>{message.text}</Notice> : null}
        </div>
      ) : null}
      <div className="loom-framework-dialog__table" aria-busy={loading}>
        <DataTable
          label="框架列表"
          rows={frameworks}
          getRowKey={frameworkIdentity}
          columns={[
            { key: "identity", header: "框架", rowHeader: true, cell: frameworkFilterLabel },
            { key: "version", header: "版本", cell: (framework) => framework.version || "—" },
            { key: "install", header: "安装", cell: installCell },
            { key: "upgrade", header: "更新", cell: upgradeCell },
          ]}
          empty={
            error ? null : loading ? <Notice>加载中…</Notice> : <EmptyState role="status" title="暂无框架" />
          }
        />
      </div>
    </Dialog>
  );
}
