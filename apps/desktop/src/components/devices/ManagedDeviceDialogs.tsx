// Standard modal composition; the controller retains device enrollment and mutation ownership.
import {
  Badge,
  Button,
  Checkbox,
  Dialog,
  EmptyState,
  Field,
  Input,
  MultiSelect,
  Notice,
  Select,
  Switch,
} from "@neuro/ui";
import { useRef } from "react";
import type { LoomDeviceKind } from "../../services/loomApi";
import { deviceKindLabels, type useManagedDevicesController } from "./useManagedDevicesController";

export function ManagedDeviceDialogs({
  controller,
  resolveReturnFocus,
}: {
  controller: ReturnType<typeof useManagedDevicesController>;
  resolveReturnFocus: (target: HTMLElement | null) => HTMLElement | null;
}) {
  const {
    busyId,
    approvalDialogOpen,
    setApprovalDialogOpen,
    editingDevice,
    setEditingDevice,
    pending,
    selectedPendingIds,
    setSelectedPendingIds,
    approveSelectedDevices,
    removeDevice,
    saveEditedDevice,
  } = controller;
  const editName = useRef<HTMLInputElement>(null);
  const busy = Boolean(busyId);
  return (
    <>
      {approvalDialogOpen ? (
        <Dialog
          open
          title="批准设备"
          className="loom-device-dialog"
          busy={busy}
          resolveReturnFocus={resolveReturnFocus}
          onOpenChange={(open) => {
            if (!open) setApprovalDialogOpen(false);
          }}
        >
          <div className="loom-device-dialog__pending">
            <Badge>{pending.length} 台待批准</Badge>
            {pending.length === 0 ? (
              <EmptyState title="暂无待批准设备" />
            ) : pending.length > 6 ? (
              <MultiSelect
                label="待批准设备"
                values={selectedPendingIds}
                onValuesChange={setSelectedPendingIds}
                disabled={busy}
                search={{ label: "搜索待批准设备", placeholder: "搜索设备", emptyMessage: "没有匹配设备" }}
                options={pending.map((device) => ({
                  value: device.id,
                  label: device.name,
                  description: `${deviceKindLabels[device.kind]} · ${device.address}`,
                }))}
              />
            ) : (
              <div className="loom-device-dialog__choices" role="group" aria-label="待批准设备">
                {pending.map((device) => {
                  const checked = selectedPendingIds.includes(device.id);
                  return (
                    <Checkbox
                      key={device.id}
                      aria-label={device.name}
                      checked={checked}
                      disabled={busy}
                      onChange={() =>
                        setSelectedPendingIds((ids) =>
                          checked ? ids.filter((id) => id !== device.id) : [...ids, device.id],
                        )
                      }
                      label={
                        <span className="loom-device-choice">
                          <strong>{device.name}</strong>
                          <span>
                            {deviceKindLabels[device.kind]} · {device.address}
                          </span>
                        </span>
                      }
                    />
                  );
                })}
              </div>
            )}
          </div>
          <div className="loom-device-dialog__actions">
            <Button variant="quiet" onClick={() => setApprovalDialogOpen(false)} disabled={busy}>
              取消
            </Button>
            <Button
              variant="primary"
              onClick={() => void approveSelectedDevices()}
              loading={busyId === "approve-selected"}
              disabled={busy || selectedPendingIds.length === 0}
            >
              批准
            </Button>
          </div>
        </Dialog>
      ) : null}
      {editingDevice ? (
        <Dialog
          open
          title="编辑设备"
          description={editingDevice.id}
          className="loom-device-dialog"
          busy={busy}
          initialFocusRef={editName}
          resolveReturnFocus={resolveReturnFocus}
          onOpenChange={(open) => {
            if (!open) setEditingDevice(null);
          }}
        >
          <div className="loom-device-dialog__fields">
            <Field label="设备名称">
              <Input
                ref={editName}
                disabled={editingDevice.isLocal}
                value={editingDevice.name}
                onChange={(event) => setEditingDevice({ ...editingDevice, name: event.target.value })}
              />
            </Field>
            <Field label="设备类型">
              <Select
                disabled={editingDevice.isLocal}
                value={editingDevice.kind}
                onChange={(event) => setEditingDevice({ ...editingDevice, kind: event.target.value as LoomDeviceKind })}
              >
                {Object.entries(deviceKindLabels).map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </Select>
            </Field>
            <div className="loom-device-dialog__wide">
              <Field label="连接地址">
                <Input
                  disabled={editingDevice.isLocal}
                  value={editingDevice.address}
                  onChange={(event) => setEditingDevice({ ...editingDevice, address: event.target.value })}
                />
              </Field>
            </div>
            {editingDevice.isLocal ? (
              <Notice className="loom-device-dialog__wide" tone="info">
                Loom 启动主机始终启用，不能删除。
              </Notice>
            ) : (
              <Switch
                className="loom-device-dialog__wide"
                label="启用设备"
                checked={editingDevice.enabled ?? true}
                onChange={(event) => setEditingDevice({ ...editingDevice, enabled: event.target.checked })}
              />
            )}
          </div>
          <div className="loom-device-dialog__actions loom-device-dialog__actions--split">
            {!editingDevice.isLocal ? (
              <Button variant="danger" disabled={busy} onClick={() => void removeDevice(editingDevice)}>
                删除
              </Button>
            ) : (
              <span />
            )}
            <div>
              <Button variant="quiet" disabled={busy} onClick={() => setEditingDevice(null)}>
                取消
              </Button>
              {!editingDevice.isLocal ? (
                <Button variant="primary" disabled={busy} loading={busy} onClick={() => void saveEditedDevice()}>
                  保存
                </Button>
              ) : null}
            </div>
          </div>
        </Dialog>
      ) : null}
    </>
  );
}
