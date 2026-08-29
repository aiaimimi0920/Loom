// Renders bounded, manifest-declared fields without loading plugin UI code.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  getCapabilitySettings,
  saveCapabilitySettings,
  type CapabilitySettingField,
  type CapabilitySettingsSnapshot,
} from "../../../services/loomApi";
import { pushAppToast } from "../../feedback/AppFeedback";

const fieldDefault = (field: CapabilitySettingField): unknown => {
  if (field.payload.default !== undefined) return field.payload.default;
  if (field.payload.type === "boolean") return false;
  if (field.payload.type === "number") return 0;
  if (field.payload.type === "enum") return field.payload.options?.[0] ?? "";
  return "";
};

function CapabilitySettingControl({
  field,
  value,
  disabled,
  onChange,
}: {
  field: CapabilitySettingField;
  value: unknown;
  disabled: boolean;
  onChange: (value: unknown) => void;
}) {
  const inputId = `capability-setting-${field.id.replace(/[^A-Za-z0-9_-]/g, "-")}`;
  if (field.payload.type === "boolean") {
    return (
      <label className="capability-setting capability-setting--toggle" htmlFor={inputId}>
        <input id={inputId} type="checkbox" checked={value === true} disabled={disabled} onChange={(event) => onChange(event.currentTarget.checked)} />
        <span><strong>{field.title ?? field.id}</strong><small>{field.payload.description}</small></span>
      </label>
    );
  }
  return (
    <label className="capability-setting" htmlFor={inputId}>
      <span><strong>{field.title ?? field.id}</strong><small>{field.payload.description}</small></span>
      {field.payload.type === "enum" ? (
        <select id={inputId} value={typeof value === "string" ? value : ""} disabled={disabled} onChange={(event) => onChange(event.currentTarget.value)}>
          {(field.payload.options ?? []).map((option) => <option key={option} value={option}>{option}</option>)}
        </select>
      ) : (
        <input
          id={inputId}
          type={field.payload.type === "number" ? "number" : "text"}
          value={typeof value === "string" || typeof value === "number" ? value : ""}
          disabled={disabled}
          onChange={(event) => onChange(field.payload.type === "number"
            ? (Number.isFinite(event.currentTarget.valueAsNumber) ? event.currentTarget.valueAsNumber : undefined)
            : event.currentTarget.value)}
        />
      )}
    </label>
  );
}

export function CapabilitySettingsEditor({
  baseUrl,
  qualifiedId,
  disabled,
}: {
  baseUrl: string;
  qualifiedId: string;
  disabled: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [snapshot, setSnapshot] = useState<CapabilitySettingsSnapshot | null>(null);
  const [draft, setDraft] = useState<Record<string, unknown>>({});
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const generationRef = useRef(0);

  useEffect(() => {
    if (!open) return;
    const generation = ++generationRef.current;
    setLoading(true);
    void getCapabilitySettings(baseUrl, qualifiedId).then((next) => {
      if (generation !== generationRef.current) return;
      setSnapshot(next);
      setDraft(next.settings.values);
      setError(null);
    }).catch((reason: unknown) => {
      if (generation === generationRef.current) {
        setError(reason instanceof Error ? reason.message : "无法读取能力扩展设置。");
      }
    }).finally(() => {
      if (generation === generationRef.current) setLoading(false);
    });
    return () => { generationRef.current += 1; };
  }, [baseUrl, open, qualifiedId]);

  const editableFields = useMemo(() => snapshot?.fields.filter((field) =>
    ["string", "number", "boolean", "enum"].includes(field.payload.type)) ?? [], [snapshot]);
  const save = useCallback(async () => {
    if (!snapshot || saving) return;
    setSaving(true);
    try {
      const editableIds = new Set(editableFields.map((field) => field.id));
      const values = Object.fromEntries(snapshot.fields.flatMap((field) => {
        if (editableIds.has(field.id)) {
          return [[field.id, draft[field.id] ?? fieldDefault(field)]];
        }
        return Object.prototype.hasOwnProperty.call(snapshot.settings.values, field.id)
          ? [[field.id, snapshot.settings.values[field.id]]]
          : [];
      }));
      const settings = await saveCapabilitySettings(
        baseUrl,
        qualifiedId,
        snapshot.settings.revision,
        snapshot.packageDigest,
        values,
      );
      setSnapshot({ ...snapshot, settings });
      setDraft(settings.values);
      pushAppToast({ level: "info", text: "能力扩展设置已保存。" });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "无法保存能力扩展设置。");
    } finally {
      setSaving(false);
    }
  }, [baseUrl, draft, editableFields, qualifiedId, saving, snapshot]);

  return (
    <section className="capability-plugin-settings">
      <button className="capability-plugin-settings__toggle" type="button" aria-expanded={open} onClick={() => setOpen((value) => !value)}>
        插件设置 <span aria-hidden="true">{open ? "−" : "+"}</span>
      </button>
      {open ? <div className="capability-plugin-settings__body">
        {loading ? <p role="status">正在验证并读取设置…</p> : null}
        {error ? <p className="capability-diagnostic" role="alert">{error}</p> : null}
        {!loading && snapshot && editableFields.length === 0 ? <p>此扩展未声明可编辑设置。</p> : null}
        {editableFields.map((field) => <CapabilitySettingControl
          key={field.id}
          field={field}
          value={draft[field.id] ?? fieldDefault(field)}
          disabled={disabled || saving}
          onChange={(value) => setDraft((current) => ({ ...current, [field.id]: value }))}
        />)}
        {editableFields.length ? <button className="signal-button" type="button" disabled={disabled || loading || saving} onClick={() => void save()}>
          {saving ? "保存中…" : "保存插件设置"}
        </button> : null}
      </div> : null}
    </section>
  );
}
