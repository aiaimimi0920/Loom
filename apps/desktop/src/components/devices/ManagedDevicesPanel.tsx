// Composes managed-device cards and dialogs around the application-owned controller.
import { Badge, Button, EmptyState } from "@neuro/ui";
import { useRef } from "react";
import { ShellIcon } from "../app/appShell";
import { ManagedDeviceDialogs } from "./ManagedDeviceDialogs";
import { deviceKindLabels, useManagedDevicesController } from "./useManagedDevicesController";

export function ManagedDevicesPanel(props: { baseUrl: string; online: boolean }) {
  const controller = useManagedDevicesController(props);
  const {
    devices,
    pending,
    loading,
    editingDevice,
    setEditingDevice,
    setSelectedPendingIds,
    setApprovalDialogOpen,
  } = controller;
  const createButton = useRef<HTMLButtonElement>(null);
  const visibleDevices = devices.filter((device) => !device.isLocal)
    .sort((a, b) => Number(b.online) - Number(a.online) || a.name.localeCompare(b.name));
  return (
    <div className="loom-managed-devices">
      <div className="loom-managed-devices__toolbar">
        <div className="loom-managed-devices__title">
          <h2>设备</h2>
          <Badge>{visibleDevices.length} 个</Badge>
        </div>
        <div className="loom-managed-devices__actions">
          <Button
            ref={createButton}
            variant="quiet"
            onClick={() => {
              setSelectedPendingIds([]);
              setApprovalDialogOpen(true);
            }}
          >
            批准设备 {pending.length > 0 ? <Badge tone="warning">{pending.length}</Badge> : null}
          </Button>
        </div>
      </div>
      {loading ? (
        <div role="status">
          <EmptyState title="正在读取设备" />
        </div>
      ) : visibleDevices.length === 0 ? (
        <EmptyState title="暂无设备" />
      ) : (
        <div className="loom-managed-devices__grid">
          {visibleDevices.map((device) => {
            const selected = editingDevice?.id === device.id;
            const enabled = device.enabled ?? true;
            return (
              <Button
                key={device.id}
                className="loom-managed-device"
                aria-pressed={selected}
                onClick={() => setEditingDevice({ ...device, enabled })}
              >
                <span className="loom-managed-device__icon" aria-hidden="true">
                  <ShellIcon kind="device" />
                </span>
                <span className="loom-managed-device__body">
                  <span className="loom-managed-device__heading">
                    <strong title={device.name}>{device.name}</strong>
                    <Badge tone={device.online ? "success" : "neutral"}>
                      {!enabled ? "禁用" : device.online ? "在线" : "离线"}
                    </Badge>
                  </span>
                  {!device.isLocal ? <span>{deviceKindLabels[device.kind]}</span> : null}
                  <code title={device.address}>{device.address}</code>
                </span>
              </Button>
            );
          })}
        </div>
      )}
      <ManagedDeviceDialogs
        controller={controller}
        resolveReturnFocus={(target) =>
          target?.isConnected && !target.closest("[hidden], [inert]") ? target : createButton.current
        }
      />
    </div>
  );
}
