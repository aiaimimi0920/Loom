// Typed parameter and credential editors; values and bindings remain controlled by Loom.
import { Badge, Checkbox, Input, Panel, Select, Switch, Textarea } from "@neuro/ui";
import { useId } from "react";
import type { LoomArtManagementParameter } from "../../services/loomApi";
import { credentialMatchesArtParameter } from "./artEditModel";
import type { useArtEditDialogController } from "./useArtEditDialogController";

type EditorController = ReturnType<typeof useArtEditDialogController>;
type ParameterProps = { parameter: LoomArtManagementParameter; controller: EditorController };

function ArtEditLiteralInput({
  parameter,
  value,
  setValue,
  disabled,
  id,
}: {
  parameter: LoomArtManagementParameter;
  value: unknown;
  setValue: (value: unknown) => void;
  disabled: boolean;
  id: string;
}) {
  const options = Array.isArray(parameter.options) ? parameter.options : [];
  if (parameter.parameterType === "boolean") {
    if (!parameter.required) {
      return (
        <Select
          id={id}
          aria-label={parameter.label}
          value={value === "" ? "" : Boolean(value) ? "true" : "false"}
          onChange={(event) => setValue(event.target.value === "" ? "" : event.target.value === "true")}
          disabled={disabled}
        >
          <option value="">使用默认值</option>
          <option value="true">启用</option>
          <option value="false">禁用</option>
        </Select>
      );
    }
    return (
      <Switch
        id={id}
        aria-label={parameter.label}
        checked={Boolean(value)}
        onChange={(event) => setValue(event.target.checked)}
        disabled={disabled}
        label="启用"
      />
    );
  }
  if (parameter.parameterType === "enum" || options.length) {
    return (
      <Select
        id={id}
        aria-label={parameter.label}
        value={String(value ?? "")}
        onChange={(event) => setValue(event.target.value)}
        disabled={disabled}
        required={parameter.required}
      >
        {!parameter.required ? <option value="">使用默认值</option> : null}
        {options.map((option, index) => {
          const optionValue =
            typeof option === "object" && option !== null && "value" in option
              ? String((option as { value: unknown }).value)
              : String(option);
          const optionLabel =
            typeof option === "object" && option !== null && "label" in option
              ? String((option as { label: unknown }).label)
              : optionValue;
          return (
            <option key={`${parameter.id}-${index}-${optionValue}`} value={optionValue}>
              {optionLabel}
            </option>
          );
        })}
      </Select>
    );
  }
  if (parameter.parameterType === "json") {
    return (
      <Textarea
        id={id}
        aria-label={parameter.label}
        className="art-edit-dialog__json"
        value={String(value ?? "")}
        onChange={(event) => setValue(event.target.value)}
        disabled={disabled}
        required={parameter.required}
      />
    );
  }
  const numericInput = parameter.parameterType === "number" || parameter.parameterType === "integer";
  return (
    <Input
      id={id}
      aria-label={parameter.label}
      className={numericInput ? "art-edit-dialog__numeric" : undefined}
      type={numericInput ? "number" : "text"}
      inputMode={numericInput ? (parameter.parameterType === "integer" ? "numeric" : "decimal") : undefined}
      value={typeof value === "string" || typeof value === "number" ? value : ""}
      min={typeof parameter.minimum === "number" ? parameter.minimum : undefined}
      max={typeof parameter.maximum === "number" ? parameter.maximum : undefined}
      step={typeof parameter.step === "number" ? parameter.step : parameter.parameterType === "integer" ? 1 : undefined}
      placeholder={
        !parameter.required && parameter.default !== undefined ? `默认：${String(parameter.default)}` : undefined
      }
      onChange={(event) =>
        setValue(numericInput ? (event.target.value === "" ? "" : Number(event.target.value)) : event.target.value)
      }
      disabled={disabled}
      required={parameter.required}
    />
  );
}

export function ArtEditParameter({ parameter, controller }: ParameterProps) {
  const {
    parameterValues,
    setParameterValues,
    globalCredentials,
    useGlobalValues,
    setUseGlobalValues,
    valueBindings,
    setValueBindings,
    busy,
  } = controller;
  const id = useId(),
    globalId = `${id}-global`;
  const value = parameterValues[parameter.id];
  const setValue = (next: unknown) => setParameterValues((current) => ({ ...current, [parameter.id]: next }));
  const compatibleCredentials = globalCredentials.filter((credential) =>
    credentialMatchesArtParameter(parameter, credential),
  );
  const usingGlobal = Boolean(useGlobalValues[parameter.id]);
  const selectedBinding = valueBindings[parameter.id] || "";
  const selectedBindingAvailable = compatibleCredentials.some((credential) => credential.name === selectedBinding);
  return (
    <div className="art-edit-dialog__parameter" data-parameter-id={parameter.id}>
      <div className="art-edit-dialog__parameter-head">
        <label htmlFor={usingGlobal ? globalId : id}>
          {parameter.label}
          {parameter.required ? " *" : ""}
        </label>
        <Checkbox
          checked={usingGlobal}
          disabled={busy}
          label="引用"
          aria-label={`${parameter.label} 引用全局值`}
          onChange={(event) => {
            const checked = event.target.checked;
            setUseGlobalValues((current) => ({ ...current, [parameter.id]: checked }));
            if (checked && !selectedBindingAvailable)
              setValueBindings((current) => ({ ...current, [parameter.id]: compatibleCredentials[0]?.name ?? "" }));
          }}
        />
      </div>
      <div className="art-edit-dialog__value-slot">
        <div data-active={!usingGlobal} aria-hidden={usingGlobal} inert={usingGlobal}>
          <ArtEditLiteralInput
            id={id}
            parameter={parameter}
            value={value}
            setValue={setValue}
            disabled={busy || usingGlobal}
          />
        </div>
        <div data-active={usingGlobal} aria-hidden={!usingGlobal} inert={!usingGlobal}>
          <Select
            id={globalId}
            aria-label={`${parameter.label} 使用的全局值`}
            value={selectedBindingAvailable ? selectedBinding : ""}
            onChange={(event) => setValueBindings((current) => ({ ...current, [parameter.id]: event.target.value }))}
            disabled={busy || !usingGlobal}
          >
            <option value="">{compatibleCredentials.length ? "选择全局值" : "暂无匹配的全局值"}</option>
            {compatibleCredentials.map((credential) => (
              <option key={credential.name} value={credential.name}>
                {credential.name}
              </option>
            ))}
          </Select>
        </div>
      </div>
    </div>
  );
}

export function ArtEditSecretParameter({ parameter, controller }: ParameterProps) {
  const { secretDrafts, setSecretDrafts, globalCredentials, busy } = controller;
  const id = useId(),
    globalId = `${id}-global`;
  const draft = secretDrafts[parameter.id] ?? { useGlobal: false, credential: "", storedCredential: "", value: "" };
  const compatibleCredentials = globalCredentials.filter((credential) =>
    credentialMatchesArtParameter(parameter, credential),
  );
  const globalCredentialExists = compatibleCredentials.some((credential) => credential.name === draft.credential);
  const showStoredStatus = !draft.useGlobal && Boolean(draft.storedCredential) && !draft.value;
  const setDraft = (next: Partial<typeof draft>) =>
    setSecretDrafts((current) => ({ ...current, [parameter.id]: { ...(current[parameter.id] ?? draft), ...next } }));
  return (
    <Panel className="art-edit-dialog__secret" data-parameter-id={parameter.id} aria-label={parameter.label}>
      <div className="art-edit-dialog__parameter-head">
        <label htmlFor={draft.useGlobal ? globalId : id}>
          {parameter.label}
          {parameter.required ? " *" : ""}
        </label>
        <Checkbox
          checked={draft.useGlobal}
          disabled={busy}
          label="引用"
          aria-label={`${parameter.label} 引用全局机密`}
          onChange={(event) =>
            setDraft({
              useGlobal: event.target.checked,
              credential: event.target.checked
                ? globalCredentialExists
                  ? draft.credential
                  : (compatibleCredentials[0]?.name ?? "")
                : draft.credential,
            })
          }
        />
      </div>
      <div className="art-edit-dialog__value-slot">
        <div data-active={!draft.useGlobal} aria-hidden={draft.useGlobal} inert={draft.useGlobal}>
          <Input
            id={id}
            type="password"
            autoComplete="new-password"
            aria-label={`${parameter.label} 的默认机密值`}
            placeholder={draft.storedCredential ? "已保存；输入新值可替换" : "输入机密值"}
            value={draft.value}
            onChange={(event) => setDraft({ value: event.target.value })}
            disabled={busy || draft.useGlobal}
          />
        </div>
        <div data-active={draft.useGlobal} aria-hidden={!draft.useGlobal} inert={!draft.useGlobal}>
          <Select
            id={globalId}
            aria-label={`${parameter.label} 使用的全局机密`}
            value={globalCredentialExists ? draft.credential : ""}
            onChange={(event) => setDraft({ credential: event.target.value })}
            disabled={busy || !draft.useGlobal}
          >
            <option value="">{compatibleCredentials.length ? "选择全局机密" : "暂无匹配的全局机密"}</option>
            {compatibleCredentials.map((credential) => (
              <option key={credential.name} value={credential.name}>
                {credential.name}
              </option>
            ))}
          </Select>
        </div>
      </div>
      <div className="art-edit-dialog__secret-status" data-active={showStoredStatus} aria-hidden={!showStoredStatus}>
        <Badge tone="success">已安全保存到当前 Art</Badge>
      </div>
    </Panel>
  );
}
