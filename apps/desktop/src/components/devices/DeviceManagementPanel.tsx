// Owns managed-device enrollment, approval, and editing flows.
import {
  addManagedDevice,
  approveManagedDevice,
  listManagedDevices,
  type LoomCredentialValueType,
  type LoomDeviceKind,
  type LoomManagedDevice,
  removeManagedDevice,
  updateManagedDevice,
} from "../../services/loomApi";
import { ArtIcon, ShellIcon } from "../app/appShell";
import {
  CredentialFieldDraft,
  credentialValueTypeLabels,
  pushAppToast,
  requestAppConfirmation,
} from "../feedback/AppFeedback";
import {
  type KeyboardEvent,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";

export const deviceKindLabels: Record<LoomDeviceKind, string> = {
  computer: "电脑",
  tablet: "平板",
  phone: "手机",
  other: "其他",
};

export function DeviceManagementPanel({
  baseUrl,
  online,
}: {
  baseUrl: string;
  online: boolean;
}) {
  const [devices, setDevices] = useState<LoomManagedDevice[]>([]);
  const [pending, setPending] = useState<LoomManagedDevice[]>([]);
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [approvalDialogOpen, setApprovalDialogOpen] = useState(false);
  const [editingDevice, setEditingDevice] = useState<LoomManagedDevice | null>(null);
  const [selectedPendingIds, setSelectedPendingIds] = useState<string[]>([]);
  const [draftName, setDraftName] = useState("");
  const [draftAddress, setDraftAddress] = useState("");
  const [draftKind, setDraftKind] = useState<LoomDeviceKind>("computer");
  const refreshInFlight = useRef(false);

  const applyResponse = useCallback((response: Awaited<ReturnType<typeof listManagedDevices>>) => {
    setDevices([...(response.devices ?? [])].sort((left, right) => Number(right.isLocal) - Number(left.isLocal)));
    setPending(response.pending ?? []);
  }, []);

  const refreshDevices = useCallback(async (quiet = false) => {
    if (refreshInFlight.current) return;
    if (!online) {
      setDevices([]);
      setPending([]);
      return;
    }
    refreshInFlight.current = true;
    if (!quiet) setLoading(true);
    try {
      applyResponse(await listManagedDevices(baseUrl));
    } catch (error) {
      if (!quiet) pushAppToast({ level: "error", text: error instanceof Error ? error.message : "设备列表加载失败" });
    } finally {
      refreshInFlight.current = false;
      if (!quiet) setLoading(false);
    }
  }, [applyResponse, baseUrl, online]);

  useEffect(() => {
    void refreshDevices();
    if (!online) return;
    const timer = window.setInterval(() => void refreshDevices(true), 4000);
    return () => window.clearInterval(timer);
  }, [online, refreshDevices]);

  useEffect(() => {
    if (!dialogOpen && !approvalDialogOpen && !editingDevice) return;
    const closeOnEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape" && !busyId) {
        setDialogOpen(false);
        setApprovalDialogOpen(false);
        setEditingDevice(null);
      }
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [approvalDialogOpen, busyId, dialogOpen, editingDevice]);

  const submitDevice = async () => {
    const name = draftName.trim();
    const address = draftAddress.trim();
    if (!name || !address) {
      pushAppToast({ level: "warning", text: "请填写设备名称和连接地址" });
      return;
    }
    setBusyId("create");
    try {
      applyResponse(await addManagedDevice(baseUrl, { name, address, kind: draftKind }));
      setDialogOpen(false);
      setDraftName("");
      setDraftAddress("");
      setDraftKind("computer");
      pushAppToast({ level: "info", text: "设备已添加" });
    } catch (error) {
      pushAppToast({ level: "error", text: error instanceof Error ? error.message : "设备添加失败" });
    } finally {
      setBusyId(null);
    }
  };

  const approveSelectedDevices = async () => {
    if (selectedPendingIds.length === 0) return;
    setBusyId("approve-selected");
    try {
      let response: Awaited<ReturnType<typeof approveManagedDevice>> | null = null;
      for (const deviceId of selectedPendingIds) {
        response = await approveManagedDevice(baseUrl, deviceId);
      }
      if (response) applyResponse(response);
      setApprovalDialogOpen(false);
      setSelectedPendingIds([]);
      pushAppToast({ level: "info", text: `已批准 ${selectedPendingIds.length} 台设备` });
    } catch (error) {
      pushAppToast({ level: "error", text: error instanceof Error ? error.message : "批准失败" });
    } finally {
      setBusyId(null);
    }
  };

  const removeDevice = async (device: LoomManagedDevice) => {
    setBusyId(device.id);
    try {
      applyResponse(await removeManagedDevice(baseUrl, device.id));
      setEditingDevice(null);
      pushAppToast({ level: "info", text: `已移除 ${device.name}` });
    } catch (error) {
      pushAppToast({ level: "error", text: error instanceof Error ? error.message : "移除失败" });
    } finally {
      setBusyId(null);
    }
  };

  const saveEditedDevice = async () => {
    if (!editingDevice || editingDevice.isLocal) return;
    const name = editingDevice.name.trim();
    const address = editingDevice.address.trim();
    if (!name || !address) {
      pushAppToast({ level: "warning", text: "请填写设备名称和连接地址" });
      return;
    }
    setBusyId(editingDevice.id);
    try {
      applyResponse(await updateManagedDevice(baseUrl, editingDevice.id, {
        name,
        address,
        kind: editingDevice.kind,
        enabled: editingDevice.enabled ?? true,
      }));
      setEditingDevice(null);
      pushAppToast({ level: "info", text: "设备设置已保存" });
    } catch (error) {
      pushAppToast({ level: "error", text: error instanceof Error ? error.message : "设备保存失败" });
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div className="device-management">
      <div className="device-management__toolbar">
        <div>
          <div className="device-management__title-row">
            <h2>所有设备</h2>
            <span>{devices.length} 个</span>
          </div>
        </div>
        <div className="device-management__actions">
          <button
            className="ghost-button device-approval-button"
            type="button"
            onClick={() => {
              setSelectedPendingIds([]);
              setApprovalDialogOpen(true);
            }}
          >
            批准设备
            {pending.length > 0 ? <span>{pending.length}</span> : null}
          </button>
          <button className="signal-button" type="button" onClick={() => setDialogOpen(true)} disabled={!online}>
            <span aria-hidden="true">＋</span> 添加设备
          </button>
        </div>
      </div>

      {loading ? <div className="device-empty">正在读取设备...</div> : devices.length === 0 ? (
        <div className="device-empty">
          <ShellIcon kind="device" />
          <strong>还没有已添加的设备</strong>
          <span>添加电脑、平板或其他客户端后会显示在这里。</span>
        </div>
      ) : (
        <div className="device-card-grid">
          {devices.map((device) => {
            const selected = editingDevice?.id === device.id;
            const enabled = device.enabled ?? true;
            return <button
              className={`device-card${device.isLocal ? " device-card--local" : ""}${selected ? " device-card--selected" : ""}${enabled ? "" : " device-card--disabled"}`}
              type="button"
              key={device.id}
              aria-pressed={selected}
              onClick={() => setEditingDevice({ ...device, enabled })}
            >
              <div className="device-card__icon"><ShellIcon kind="device" /></div>
              <div className="device-card__body">
                <div className="device-card__heading">
                  <strong>{device.name}</strong>
                  <span className="device-status device-status--approved">{device.isLocal ? "本机" : enabled ? "启用" : "禁用"}</span>
                </div>
                {!device.isLocal ? <span>{deviceKindLabels[device.kind]}</span> : null}
                <code>{device.address}</code>
              </div>
              <span className={enabled ? "device-card__online" : "device-card__offline"} title={enabled ? "设备已启用" : "设备已禁用"} />
            </button>;
          })}
        </div>
      )}

      {dialogOpen ? createPortal(
        <div
          className="framework-dialog-backdrop"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget && !busyId) setDialogOpen(false);
          }}
        >
          <section className="framework-dialog device-add-dialog" role="dialog" aria-modal="true" aria-labelledby="device-add-title">
            <header className="framework-dialog__header">
              <div>
                <h2 id="device-add-title">添加设备</h2>
                <p>保存客户端的连接地址。</p>
              </div>
              <button className="art-card-action" type="button" aria-label="关闭" onClick={() => setDialogOpen(false)} disabled={Boolean(busyId)}>
                <ShellIcon kind="close" />
              </button>
            </header>
            <div className="device-add-dialog__body">
              <label>
                <span>设备名称</span>
                <input className="studio-input" autoFocus value={draftName} onChange={(event) => setDraftName(event.target.value)} placeholder="例如：工作室电脑" />
              </label>
              <label>
                <span>设备类型</span>
                <select className="studio-input" value={draftKind} onChange={(event) => setDraftKind(event.target.value as LoomDeviceKind)}>
                  {Object.entries(deviceKindLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}
                </select>
              </label>
              <label className="device-add-dialog__wide">
                <span>连接地址</span>
                <input className="studio-input" value={draftAddress} onChange={(event) => setDraftAddress(event.target.value)} placeholder="192.168.1.28:19820" />
              </label>
            </div>
            <footer className="device-add-dialog__footer">
              <button className="ghost-button" type="button" onClick={() => setDialogOpen(false)} disabled={Boolean(busyId)}>取消</button>
              <button className="signal-button" type="button" onClick={() => void submitDevice()} disabled={Boolean(busyId)}>{busyId ? "添加中..." : "添加"}</button>
            </footer>
          </section>
        </div>,
        document.body,
      ) : null}
      {approvalDialogOpen ? createPortal(
        <div className="framework-dialog-backdrop" role="presentation" onMouseDown={(event) => {
          if (event.target === event.currentTarget && !busyId) setApprovalDialogOpen(false);
        }}>
          <section className="framework-dialog device-approval-dialog" role="dialog" aria-modal="true" aria-labelledby="device-approval-title">
            <header className="framework-dialog__header">
              <div><h2 id="device-approval-title">批准设备</h2><p>{pending.length} 台设备等待批准</p></div>
              <button className="art-card-action" type="button" aria-label="关闭" onClick={() => setApprovalDialogOpen(false)} disabled={Boolean(busyId)}><ShellIcon kind="close" /></button>
            </header>
            <div className="device-approval-dialog__list">
              {pending.length === 0 ? <div className="device-approval__empty">暂无待批准设备</div> : pending.map((device) => {
                const checked = selectedPendingIds.includes(device.id);
                return <label className={checked ? "device-request-row device-request-row--selected" : "device-request-row"} key={device.id}>
                  <input type="checkbox" checked={checked} onChange={() => setSelectedPendingIds((ids) => checked ? ids.filter((id) => id !== device.id) : [...ids, device.id])} />
                  <div className="device-card__icon"><ShellIcon kind="device" /></div>
                  <div className="device-request-row__identity"><strong>{device.name}</strong><span>{deviceKindLabels[device.kind]} · {device.address}</span></div>
                </label>;
              })}
            </div>
            <footer className="device-add-dialog__footer">
              <button className="ghost-button" type="button" onClick={() => setApprovalDialogOpen(false)} disabled={Boolean(busyId)}>取消</button>
              <button className="signal-button" type="button" onClick={() => void approveSelectedDevices()} disabled={Boolean(busyId) || selectedPendingIds.length === 0}>{busyId ? "批准中..." : `批准 ${selectedPendingIds.length || ""}`}</button>
            </footer>
          </section>
        </div>, document.body,
      ) : null}
      {editingDevice ? createPortal(
        <div className="framework-dialog-backdrop" role="presentation" onMouseDown={(event) => {
          if (event.target === event.currentTarget && !busyId) setEditingDevice(null);
        }}>
          <section className="framework-dialog device-edit-dialog" role="dialog" aria-modal="true" aria-labelledby="device-edit-title">
            <header className="framework-dialog__header">
              <div><h2 id="device-edit-title">编辑设备</h2><p>{editingDevice.id}</p></div>
              <button className="art-card-action" type="button" aria-label="关闭" onClick={() => setEditingDevice(null)} disabled={Boolean(busyId)}><ShellIcon kind="close" /></button>
            </header>
            <div className="device-edit-dialog__body">
              <label><span>设备名称</span><input className="studio-input" autoFocus={!editingDevice.isLocal} disabled={editingDevice.isLocal} value={editingDevice.name} onChange={(event) => setEditingDevice({ ...editingDevice, name: event.target.value })} /></label>
              <label><span>设备类型</span><select className="studio-input" disabled={editingDevice.isLocal} value={editingDevice.kind} onChange={(event) => setEditingDevice({ ...editingDevice, kind: event.target.value as LoomDeviceKind })}>{Object.entries(deviceKindLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
              <label className="device-edit-dialog__wide"><span>连接地址</span><input className="studio-input" disabled={editingDevice.isLocal} value={editingDevice.address} onChange={(event) => setEditingDevice({ ...editingDevice, address: event.target.value })} /></label>
              {editingDevice.isLocal ? <p className="device-edit-dialog__protected">Loom 启动主机始终启用，不能删除。</p> : <label className="device-edit-toggle"><input type="checkbox" checked={editingDevice.enabled ?? true} onChange={(event) => setEditingDevice({ ...editingDevice, enabled: event.target.checked })} /><span>启用设备</span></label>}
            </div>
            <footer className="device-edit-dialog__footer">
              {!editingDevice.isLocal ? <button className="danger-button" type="button" disabled={Boolean(busyId)} onClick={() => void (async () => {
                const accepted = await requestAppConfirmation({
                  title: "删除设备",
                  message: `删除 ${editingDevice.name} 后，需要重新添加或批准才能再次使用。`,
                  confirmLabel: "删除",
                });
                if (accepted) await removeDevice(editingDevice);
              })()}>删除</button> : <span />}
              <div>
                <button className="ghost-button" type="button" disabled={Boolean(busyId)} onClick={() => setEditingDevice(null)}>取消</button>
                {!editingDevice.isLocal ? <button className="signal-button" type="button" disabled={Boolean(busyId)} onClick={() => void saveEditedDevice()}>{busyId ? "保存中..." : "保存"}</button> : null}
              </div>
            </footer>
          </section>
        </div>, document.body,
      ) : null}
    </div>
  );
}

export function CredentialFieldDialog({
  draft,
  busy,
  onChange,
  onSave,
  onDelete,
  onClose,
}: {
  draft: CredentialFieldDraft | null;
  busy: boolean;
  onChange: (draft: CredentialFieldDraft) => void;
  onSave: () => void;
  onDelete: () => void;
  onClose: () => void;
}) {
  const dialogOpen = draft !== null;

  useEffect(() => {
    if (!dialogOpen) return;
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        if (!busy) onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [busy, dialogOpen, onClose]);

  if (!draft) return null;
  return createPortal(
    <div
      className="framework-dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (!busy && event.target === event.currentTarget) onClose();
      }}
    >
      <section className="framework-dialog credential-field-dialog" role="dialog" aria-modal="true" aria-labelledby="credential-field-dialog-title">
        <header className="framework-dialog__header">
          <h2 id="credential-field-dialog-title">{draft.original ? "编辑字段" : "添加字段"}</h2>
          <button className="icon-button" type="button" aria-label="关闭" disabled={busy} onClick={onClose}>
            <ArtIcon kind="close" />
          </button>
        </header>
        <div className="credential-field-dialog__form">
          <input autoFocus className="studio-input" aria-label="字段名字" placeholder="名字" value={draft.name} onChange={(event) => onChange({ ...draft, name: event.target.value })} />
          <select className="studio-input" aria-label="字段格式" value={draft.valueType} onChange={(event) => onChange({ ...draft, valueType: event.target.value as LoomCredentialValueType, value: "" })}>
            {(Object.entries(credentialValueTypeLabels) as Array<[LoomCredentialValueType, string]>).map(([valueType, label]) => (
              <option key={valueType} value={valueType}>{label}</option>
            ))}
          </select>
          {draft.valueType === "boolean" ? (
            <select className="studio-input" aria-label="字段值" value={draft.value} onChange={(event) => onChange({ ...draft, value: event.target.value })}>
              <option value="">选择</option>
              <option value="true">true</option>
              <option value="false">false</option>
            </select>
          ) : draft.valueType === "json" ? (
            <textarea className="studio-input credential-field-dialog__value" aria-label="字段值" placeholder="{}" value={draft.value} onChange={(event) => onChange({ ...draft, value: event.target.value })} />
          ) : (
            <input
              className="studio-input"
              aria-label="字段值"
              type={draft.valueType === "string" ? "text" : "number"}
              step={draft.valueType === "integer" ? 1 : "any"}
              placeholder="值"
              value={draft.value}
              onChange={(event) => onChange({ ...draft, value: event.target.value })}
            />
          )}
        </div>
        <footer className="credential-field-dialog__actions">
          {draft.original ? (
            <button className="danger-button" type="button" disabled={busy} onClick={onDelete}>删除</button>
          ) : <span />}
          <button className="signal-button" type="button" disabled={busy || !draft.name.trim() || !draft.value} onClick={onSave}>
            {busy ? "保存中" : "保存"}
          </button>
        </footer>
      </section>
    </div>,
    document.body,
  );
}
