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
import { useUiText } from "../../i18n/index.ts";
import type { LoomDeviceKind } from "../../services/loomApi";
import { deviceKindLabels, type useManagedDevicesController } from "./useManagedDevicesController";

export function ManagedDeviceDialogs({
  controller,
  resolveReturnFocus,
}: {
  controller: ReturnType<typeof useManagedDevicesController>;
  resolveReturnFocus: (target: HTMLElement | null) => HTMLElement | null;
}) {
  const { t, tf } = useUiText();
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
          title={t("devices.managed.approveDevices")}
          className="loom-device-dialog"
          busy={busy}
          resolveReturnFocus={resolveReturnFocus}
          onOpenChange={(open) => {
            if (!open) setApprovalDialogOpen(false);
          }}
        >
          <div className="loom-device-dialog__pending">
            <Badge>{tf("devices.managed.dialog.pendingBadge", { count: pending.length })}</Badge>
            {pending.length === 0 ? (
              <EmptyState title={t("devices.managed.dialog.noPending")} />
            ) : pending.length > 6 ? (
              <MultiSelect
                label={t("devices.managed.dialog.pendingLabel")}
                values={selectedPendingIds}
                onValuesChange={setSelectedPendingIds}
                disabled={busy}
                search={{
                  label: t("devices.managed.dialog.searchPendingLabel"),
                  placeholder: t("devices.managed.dialog.searchPendingPlaceholder"),
                  emptyMessage: t("devices.common.noMatchingDevices"),
                }}
                options={pending.map((device) => ({
                  value: device.id,
                  label: device.name,
                  description: `${deviceKindLabels[device.kind]} · ${device.address}`,
                }))}
              />
            ) : (
              <div className="loom-device-dialog__choices" role="group" aria-label={t("devices.managed.dialog.pendingLabel")}>
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
              {t("devices.common.cancel")}
            </Button>
            <Button
              variant="primary"
              onClick={() => void approveSelectedDevices()}
              loading={busyId === "approve-selected"}
              disabled={busy || selectedPendingIds.length === 0}
            >
              {t("devices.managed.dialog.approveAction")}
            </Button>
          </div>
        </Dialog>
      ) : null}
      {editingDevice ? (
        <Dialog
          open
          title={t("devices.managed.editDialog.title")}
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
            <Field label={t("devices.managed.editDialog.nameLabel")}>
              <Input
                ref={editName}
                disabled={editingDevice.isLocal}
                value={editingDevice.name}
                onChange={(event) => setEditingDevice({ ...editingDevice, name: event.target.value })}
              />
            </Field>
            <Field label={t("devices.managed.editDialog.kindLabel")}>
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
              <Field label={t("devices.managed.editDialog.addressLabel")}>
                <Input
                  disabled={editingDevice.isLocal}
                  value={editingDevice.address}
                  onChange={(event) => setEditingDevice({ ...editingDevice, address: event.target.value })}
                />
              </Field>
            </div>
            {editingDevice.isLocal ? (
              <Notice className="loom-device-dialog__wide" tone="info">
                {t("devices.managed.editDialog.localHostNotice")}
              </Notice>
            ) : (
              <Switch
                className="loom-device-dialog__wide"
                label={t("devices.managed.editDialog.enableSwitch")}
                checked={editingDevice.enabled ?? true}
                onChange={(event) => setEditingDevice({ ...editingDevice, enabled: event.target.checked })}
              />
            )}
          </div>
          <div className="loom-device-dialog__actions loom-device-dialog__actions--split">
            {!editingDevice.isLocal ? (
              <Button variant="danger" disabled={busy} onClick={() => void removeDevice(editingDevice)}>
                {t("devices.common.delete")}
              </Button>
            ) : (
              <span />
            )}
            <div>
              <Button variant="quiet" disabled={busy} onClick={() => setEditingDevice(null)}>
                {t("devices.common.cancel")}
              </Button>
              {!editingDevice.isLocal ? (
                <Button variant="primary" disabled={busy} loading={busy} onClick={() => void saveEditedDevice()}>
                  {t("devices.common.save")}
                </Button>
              ) : null}
            </div>
          </div>
        </Dialog>
      ) : null}
    </>
  );
}
