// Owns device polling, drafts, and daemon mutations; presentation stays in the panels.
import { useCallback, useEffect, useRef, useState } from "react";
import { uiFormat, uiText } from "../../i18n/index.ts";
import {
  approveManagedDevice,
  listManagedDevices,
  removeManagedDevice,
  updateManagedDevice,
  type LoomDeviceKind,
  type LoomManagedDevice,
} from "../../services/loomApi";
import { pushAppToast, requestAppConfirmation } from "../feedback/AppFeedback";
export const deviceKindLabels: Record<LoomDeviceKind, string> = {
  get computer() {
    return uiText("devices.kind.computer");
  },
  get tablet() {
    return uiText("devices.kind.tablet");
  },
  get phone() {
    return uiText("devices.kind.phone");
  },
  get other() {
    return uiText("devices.kind.other");
  },
};
export function useManagedDevicesController({ baseUrl, online }: { baseUrl: string; online: boolean }) {
  const [devices, setDevices] = useState<LoomManagedDevice[]>([]);

  const [pending, setPending] = useState<LoomManagedDevice[]>([]);

  const [loading, setLoading] = useState(false);

  const [busyId, setBusyId] = useState<string | null>(null);

  const [approvalDialogOpen, setApprovalDialogOpen] = useState(false);

  const [editingDevice, setEditingDevice] = useState<LoomManagedDevice | null>(null);

  const [selectedPendingIds, setSelectedPendingIds] = useState<string[]>([]);

  const generation = useRef(0);
  const refreshInFlight = useRef<number | null>(null);

  const applyResponse = useCallback((response: Awaited<ReturnType<typeof listManagedDevices>>) => {
    setDevices([...(response.devices ?? [])].sort((left, right) => Number(right.isLocal) - Number(left.isLocal)));
    setPending(response.pending ?? []);
  }, []);

  const refreshDevices = useCallback(
    async (quiet = false) => {
      const version = generation.current;
      if (!online) {
        setDevices([]);
        setPending([]);
        return;
      }
      if (refreshInFlight.current === version) return;
      refreshInFlight.current = version;
      if (!quiet) setLoading(true);
      try {
        const response = await listManagedDevices(baseUrl);
        if (version === generation.current) applyResponse(response);
      } catch (error) {
        if (!quiet && version === generation.current)
          pushAppToast({
            level: "error",
            text: error instanceof Error ? error.message : uiText("devices.managed.toast.loadFailed"),
          });
      } finally {
        if (refreshInFlight.current === version) refreshInFlight.current = null;
        if (!quiet && version === generation.current) setLoading(false);
      }
    },
    [applyResponse, baseUrl, online],
  );

  useEffect(() => {
    // Changing daemon identity invalidates UI receipts, not already-issued daemon mutations.
    generation.current += 1;
    setDevices([]);
    setPending([]);
    setLoading(false);
    setBusyId(null);
    setApprovalDialogOpen(false);
    setEditingDevice(null);
    setSelectedPendingIds([]);
    void refreshDevices();
    const timer = online ? window.setInterval(() => void refreshDevices(true), 4000) : undefined;
    return () => {
      generation.current += 1;
      window.clearInterval(timer);
    };
  }, [online, refreshDevices]);

  const approveSelectedDevices = async () => {
    if (selectedPendingIds.length === 0) return;
    const version = generation.current;
    setBusyId("approve-selected");
    try {
      let response: Awaited<ReturnType<typeof approveManagedDevice>> | null = null;
      for (const deviceId of selectedPendingIds) {
        response = await approveManagedDevice(baseUrl, deviceId);
        if (version !== generation.current) return;
      }
      if (response) applyResponse(response);
      setApprovalDialogOpen(false);
      setSelectedPendingIds([]);
      pushAppToast({
        level: "info",
        text: uiFormat("devices.managed.toast.approvedCount", { count: selectedPendingIds.length }),
      });
    } catch (error) {
      if (version !== generation.current) return;
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : uiText("devices.managed.toast.approveFailed"),
      });
    } finally {
      if (version === generation.current) setBusyId(null);
    }
  };

  const removeDevice = async (device: LoomManagedDevice) => {
    const version = generation.current;
    const accepted = await requestAppConfirmation({
      title: uiText("devices.managed.confirmRemove.title"),
      message: uiFormat("devices.managed.confirmRemove.message", { name: device.name }),
      confirmLabel: uiText("devices.managed.confirmRemove.confirm"),
    });
    if (!accepted || version !== generation.current) return;
    setBusyId(device.id);
    try {
      const response = await removeManagedDevice(baseUrl, device.id);
      if (version !== generation.current) return;
      applyResponse(response);
      setEditingDevice(null);
      pushAppToast({
        level: "info",
        text: uiFormat("devices.managed.toast.removed", { name: device.name }),
      });
    } catch (error) {
      if (version !== generation.current) return;
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : uiText("devices.managed.toast.removeFailed"),
      });
    } finally {
      if (version === generation.current) setBusyId(null);
    }
  };

  const saveEditedDevice = async () => {
    if (!editingDevice || editingDevice.isLocal) return;
    const version = generation.current;
    const name = editingDevice.name.trim();
    const address = editingDevice.address.trim();
    if (!name || !address) {
      pushAppToast({ level: "warning", text: uiText("devices.managed.toast.missingFields") });
      return;
    }
    setBusyId(editingDevice.id);
    try {
      const response = await updateManagedDevice(baseUrl, editingDevice.id, {
        name,
        address,
        kind: editingDevice.kind,
        enabled: editingDevice.enabled ?? true,
      });
      if (version !== generation.current) return;
      applyResponse(response);
      setEditingDevice(null);
      pushAppToast({ level: "info", text: uiText("devices.managed.toast.saved") });
    } catch (error) {
      if (version !== generation.current) return;
      pushAppToast({
        level: "error",
        text: error instanceof Error ? error.message : uiText("devices.managed.toast.saveFailed"),
      });
    } finally {
      if (version === generation.current) setBusyId(null);
    }
  };
  return {
    devices,
    pending,
    loading,
    busyId,
    approvalDialogOpen,
    setApprovalDialogOpen,
    editingDevice,
    setEditingDevice,
    selectedPendingIds,
    setSelectedPendingIds,
    approveSelectedDevices,
    removeDevice,
    saveEditedDevice,
  };
}
