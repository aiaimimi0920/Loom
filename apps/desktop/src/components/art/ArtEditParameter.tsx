// Typed parameter and credential editors; values and bindings remain controlled by Loom.
import { Badge, Checkbox, Input, Panel, Select, Switch, Textarea } from "@neuro/ui";
import { useId } from "react";
import { useUiText } from "../../i18n/index.ts";
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
  const { t, tf } = useUiText();
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
          <option value="">{t("art.edit.param.useDefault")}</option>
          <option value="true">{t("art.action.enable")}</option>
          <option value="false">{t("art.action.disable")}</option>
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
        label={t("art.action.enable")}
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
        {!parameter.required ? <option value="">{t("art.edit.param.useDefault")}</option> : null}
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
        !parameter.required && parameter.default !== undefined
          ? tf("art.edit.param.defaultPlaceholder", { value: String(parameter.default) })
          : undefined
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
  const { t, tf } = useUiText();
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
          label={t("art.edit.param.reference")}
          aria-label={tf("art.edit.param.referenceGlobalValueAria", { label: parameter.label })}
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
            aria-label={tf("art.edit.param.selectedGlobalValueAria", { label: parameter.label })}
            value={selectedBindingAvailable ? selectedBinding : ""}
            onChange={(event) => setValueBindings((current) => ({ ...current, [parameter.id]: event.target.value }))}
            disabled={busy || !usingGlobal}
          >
            <option value="">
              {compatibleCredentials.length
                ? t("art.edit.param.selectGlobalValue")
                : t("art.edit.param.noMatchingGlobalValue")}
            </option>
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
  const { t, tf } = useUiText();
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
          label={t("art.edit.param.reference")}
          aria-label={tf("art.edit.param.referenceGlobalSecretAria", { label: parameter.label })}
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
            aria-label={tf("art.edit.param.defaultSecretAria", { label: parameter.label })}
            placeholder={
              draft.storedCredential
                ? t("art.edit.param.secretSavedPlaceholder")
                : t("art.edit.param.secretEnterPlaceholder")
            }
            value={draft.value}
            onChange={(event) => setDraft({ value: event.target.value })}
            disabled={busy || draft.useGlobal}
          />
        </div>
        <div data-active={draft.useGlobal} aria-hidden={!draft.useGlobal} inert={!draft.useGlobal}>
          <Select
            id={globalId}
            aria-label={tf("art.edit.param.selectedGlobalSecretAria", { label: parameter.label })}
            value={globalCredentialExists ? draft.credential : ""}
            onChange={(event) => setDraft({ credential: event.target.value })}
            disabled={busy || !draft.useGlobal}
          >
            <option value="">
              {compatibleCredentials.length
                ? t("art.edit.param.selectGlobalSecret")
                : t("art.edit.param.noMatchingGlobalSecret")}
            </option>
            {compatibleCredentials.map((credential) => (
              <option key={credential.name} value={credential.name}>
                {credential.name}
              </option>
            ))}
          </Select>
        </div>
      </div>
      <div className="art-edit-dialog__secret-status" data-active={showStoredStatus} aria-hidden={!showStoredStatus}>
        <Badge tone="success">{t("art.edit.param.savedToArt")}</Badge>
      </div>
    </Panel>
  );
}
