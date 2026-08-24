// Owns Art runtime configuration editing and credential selection.
import { artDisplayIdentity } from "../../services/artHubUi";
import {
  type LoomArtManagement,
  type LoomArtManagementParameter,
  type LoomArtManagementSettingsInput,
  type LoomCredentialSummary,
  LoomToolDefinition,
} from "../../services/loomApi";
import { ArtIcon, ArtPublisherIcon } from "../app/appShell";
import {
  type KeyboardEvent,
  useEffect,
  useRef,
  useState,
} from "react";

export interface ArtSecretDraft {
  useGlobal: boolean;
  credential: string;
  storedCredential: string;
  value: string;
}

export function credentialMatchesArtParameter(
  parameter: LoomArtManagementParameter,
  credential: LoomCredentialSummary,
): boolean {
  const valueType = credential.valueType ?? "string";
  if (parameter.secret) return valueType === "string";
  switch (parameter.parameterType) {
    case "number":
      return valueType === "number" || valueType === "integer";
    case "integer":
      return valueType === "integer";
    case "boolean":
      return valueType === "boolean";
    case "json":
      return valueType === "json";
    default:
      return valueType === "string";
  }
}

export function ArtEditDialog({
  tool,
  management,
  loading,
  busyAction,
  error,
  onClose,
  onSave,
  onUpdate,
}: {
  tool: LoomToolDefinition | null;
  management: LoomArtManagement | null;
  loading: boolean;
  busyAction: "save" | "update" | null;
  error: string | null;
  onClose: () => void;
  onSave: (input: LoomArtManagementSettingsInput) => Promise<void>;
  onUpdate: (version: string, input: LoomArtManagementSettingsInput) => Promise<void>;
}) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const nameInputRef = useRef<HTMLInputElement | null>(null);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [autoUpdate, setAutoUpdate] = useState(true);
  const [targetVersion, setTargetVersion] = useState("");
  const [parameterValues, setParameterValues] = useState<Record<string, unknown>>({});
  const [valueBindings, setValueBindings] = useState<Record<string, string>>({});
  const [useGlobalValues, setUseGlobalValues] = useState<Record<string, boolean>>({});
  const [secretDrafts, setSecretDrafts] = useState<Record<string, ArtSecretDraft>>({});
  const [formError, setFormError] = useState<string | null>(null);
  const busy = busyAction !== null;

  useEffect(() => {
    if (!management) return;
    const identity = tool
      ? artDisplayIdentity(
          tool,
          management.artId,
          typeof document === "undefined"
            ? "zh-CN"
            : document.documentElement.lang || window.navigator.language,
        )
      : null;
    setName(management.canEditIdentity
      ? management.name || tool?.name || ""
      : identity?.localizedName || management.name || tool?.name || "");
    setDescription(management.canEditIdentity
      ? management.description || tool?.description || ""
      : identity?.localizedDescription || management.description || tool?.description || "");
    setAutoUpdate(management.autoUpdate !== false);
    setTargetVersion(management.currentVersion);
    setParameterValues(Object.fromEntries(
      management.parameters
        .filter((parameter) => !parameter.secret)
        .map((parameter) => {
          const hasSavedValue = Object.prototype.hasOwnProperty.call(management.defaults, parameter.id);
          const value = hasSavedValue
            ? management.defaults[parameter.id]
            : parameter.required
              ? parameter.default
              : "";
          return [parameter.id, parameter.parameterType === "json" && value !== undefined && value !== ""
            ? JSON.stringify(value, null, 2)
            : value ?? (parameter.required && parameter.parameterType === "boolean" ? false : "")];
        }),
    ));
    setValueBindings({ ...management.valueBindings });
    setUseGlobalValues(Object.fromEntries(
      management.parameters
        .filter((parameter) => !parameter.secret)
        .map((parameter) => [parameter.id, Boolean(management.valueBindings[parameter.id])]),
    ));
    const credentialsByName = new Map(
      management.availableCredentials.map((credential) => [credential.name, credential]),
    );
    setSecretDrafts(Object.fromEntries(
      management.parameters
        .filter((parameter) => parameter.secret)
        .map((parameter) => {
          const binding = management.credentialBindings[parameter.id] || "";
          const summary = credentialsByName.get(binding);
          const storedCredential = summary?.scope.artId === management.artId
            || binding.startsWith("loom-art-secret-")
            ? binding
            : "";
          return [parameter.id, {
            useGlobal: Boolean(binding && !storedCredential),
            credential: binding && !storedCredential ? binding : "",
            storedCredential,
            value: "",
          } satisfies ArtSecretDraft];
        }),
    ));
    setFormError(null);
  }, [management, tool]);

  useEffect(() => {
    if (!tool) return;
    if (management && !loading) nameInputRef.current?.focus();
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        if (!busy) onClose();
        return;
      }
      if (event.key !== "Tab" || !dialogRef.current) return;
      const focusable = [...dialogRef.current.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      )];
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [busy, loading, management, onClose, tool]);

  if (!tool) return null;

  const displayIdentity = artDisplayIdentity(
    tool,
    management?.artId,
    typeof document === "undefined"
      ? "zh-CN"
      : document.documentElement.lang || window.navigator.language,
  );
  const requiredParameters = management?.parameters.filter((parameter) => parameter.required && !parameter.secret) ?? [];
  const optionalParameters = management?.parameters.filter((parameter) => !parameter.required && !parameter.secret) ?? [];
  const secretParameters = management?.parameters.filter((parameter) => parameter.secret) ?? [];
  const hasSplitWorkspace = requiredParameters.length > 0 && secretParameters.length > 0;
  const globalCredentials = management?.availableCredentials.filter((credential) => (
    !credential.scope.frameworkId && !credential.scope.artId
  )) ?? [];
  const visibleError = formError || error;

  const valuePresent = (value: unknown) => (
    value !== undefined && value !== null && (typeof value !== "string" || value.trim() !== "")
  );

  const buildSettingsInput = (): LoomArtManagementSettingsInput => {
    if (!management) throw new Error("Art 管理信息尚未加载。");
    const defaults: Record<string, unknown> = {};
    const nextValueBindings: Record<string, string> = {};
    for (const parameter of management.parameters.filter((candidate) => !candidate.secret)) {
      if (useGlobalValues[parameter.id]) {
        const binding = valueBindings[parameter.id];
        if (!binding) throw new Error(`必须为 ${parameter.label} 选择全局值。`);
        const credential = globalCredentials.find((candidate) => candidate.name === binding);
        if (!credential || !credentialMatchesArtParameter(parameter, credential)) {
          throw new Error(`${parameter.label} 的全局值不存在或类型不匹配。`);
        }
        nextValueBindings[parameter.id] = binding;
        continue;
      }
      const draft = parameterValues[parameter.id];
      if (parameter.required && !valuePresent(draft)) {
        throw new Error(`必须填写 ${parameter.label}。`);
      }
      if (!valuePresent(draft)) continue;
      if (parameter.parameterType === "json" && typeof draft === "string") {
        try {
          defaults[parameter.id] = JSON.parse(draft);
        } catch {
          throw new Error(`${parameter.label} 不是有效的 JSON。`);
        }
      } else {
        defaults[parameter.id] = draft;
      }
    }
    const bindings: Record<string, string> = {};
    const secretValues: Record<string, string> = {};
    for (const parameter of management.parameters.filter((candidate) => candidate.secret)) {
      const draft = secretDrafts[parameter.id] ?? {
        useGlobal: false,
        credential: "",
        storedCredential: "",
        value: "",
      };
      if (draft.useGlobal) {
        if (!draft.credential) throw new Error(`必须为 ${parameter.label} 选择全局机密。`);
        const credential = globalCredentials.find((candidate) => candidate.name === draft.credential);
        if (!credential || !credentialMatchesArtParameter(parameter, credential)) {
          throw new Error(`${parameter.label} 的全局机密不存在或不是文本类型。`);
        }
        bindings[parameter.id] = draft.credential;
        continue;
      }
      if (draft.value.trim()) {
        secretValues[parameter.id] = draft.value;
        if (draft.storedCredential) bindings[parameter.id] = draft.storedCredential;
        continue;
      }
      if (draft.storedCredential) {
        bindings[parameter.id] = draft.storedCredential;
        continue;
      }
      if (parameter.required) throw new Error(`必须填写 ${parameter.label}。`);
    }
    return {
      ...(management.canEditIdentity ? { name: name.trim(), description } : {}),
      autoUpdate,
      defaults,
      valueBindings: nextValueBindings,
      credentialBindings: bindings,
      ...(Object.keys(secretValues).length ? { secretValues } : {}),
    };
  };

  const submit = async (action: "save" | "update") => {
    setFormError(null);
    try {
      const input = buildSettingsInput();
      if (action === "update") {
        await onUpdate(targetVersion, input);
      } else {
        await onSave(input);
      }
    } catch (submitError) {
      setFormError(submitError instanceof Error ? submitError.message : "无法保存 Art 设置。");
    }
  };

  const renderParameter = (parameter: LoomArtManagementParameter) => {
    const value = parameterValues[parameter.id];
    const setValue = (next: unknown) => setParameterValues((current) => ({ ...current, [parameter.id]: next }));
    const options = Array.isArray(parameter.options) ? parameter.options : [];
    const compatibleCredentials = globalCredentials.filter((credential) => (
      credentialMatchesArtParameter(parameter, credential)
    ));
    const usingGlobal = Boolean(useGlobalValues[parameter.id]);
    const selectedBinding = valueBindings[parameter.id] || "";
    const selectedBindingAvailable = compatibleCredentials.some((credential) => (
      credential.name === selectedBinding
    ));
    const renderLiteralEditor = () => {
      if (parameter.parameterType === "boolean") {
        if (!parameter.required) {
          return (
            <select aria-label={parameter.label} className="studio-input" value={value === "" ? "" : Boolean(value) ? "true" : "false"} onChange={(event) => setValue(event.target.value === "" ? "" : event.target.value === "true")} disabled={busy}>
              <option value="">使用默认值</option>
              <option value="true">启用</option>
              <option value="false">禁用</option>
            </select>
          );
        }
        return (
          <label className="art-edit-dialog__boolean">
            <input aria-label={parameter.label} type="checkbox" checked={Boolean(value)} onChange={(event) => setValue(event.target.checked)} disabled={busy} />
            <span>{Boolean(value) ? "启用" : "禁用"}</span>
          </label>
        );
      }
      if (parameter.parameterType === "enum" || options.length) {
        return (
          <select aria-label={parameter.label} className="studio-input" value={String(value ?? "")} onChange={(event) => setValue(event.target.value)} disabled={busy} required={parameter.required}>
            {!parameter.required ? <option value="">使用默认值</option> : null}
            {options.map((option, index) => {
              const optionValue = typeof option === "object" && option !== null && "value" in option
                ? String((option as { value: unknown }).value)
                : String(option);
              const optionLabel = typeof option === "object" && option !== null && "label" in option
                ? String((option as { label: unknown }).label)
                : optionValue;
              return <option key={`${parameter.id}-${index}-${optionValue}`} value={optionValue}>{optionLabel}</option>;
            })}
          </select>
        );
      }
      if (parameter.parameterType === "json") {
        return <textarea aria-label={parameter.label} className="studio-input art-edit-dialog__json" value={String(value ?? "")} onChange={(event) => setValue(event.target.value)} disabled={busy} required={parameter.required} />;
      }
      const numericInput = parameter.parameterType === "number" || parameter.parameterType === "integer";
      return (
        <input
          aria-label={parameter.label}
          className={`studio-input art-edit-dialog__value-input${numericInput ? " art-edit-dialog__value-input--number" : ""}`}
          type={numericInput ? "number" : "text"}
          inputMode={numericInput ? parameter.parameterType === "integer" ? "numeric" : "decimal" : undefined}
          value={typeof value === "string" || typeof value === "number" ? value : ""}
          min={typeof parameter.minimum === "number" ? parameter.minimum : undefined}
          max={typeof parameter.maximum === "number" ? parameter.maximum : undefined}
          step={typeof parameter.step === "number" ? parameter.step : parameter.parameterType === "integer" ? 1 : undefined}
          placeholder={!parameter.required && parameter.default !== undefined ? `默认：${String(parameter.default)}` : undefined}
          onChange={(event) => setValue(
            numericInput
              ? (event.target.value === "" ? "" : Number(event.target.value))
              : event.target.value,
          )}
          disabled={busy}
          required={parameter.required}
        />
      );
    };
    return (
      <div className="art-edit-dialog__parameter" key={parameter.id}>
        <div className="art-edit-dialog__parameter-head">
          <span>{parameter.label}{parameter.required ? " *" : ""}</span>
          <label className="art-edit-dialog__binding-toggle">
            <input
              type="checkbox"
              checked={usingGlobal}
              onChange={(event) => {
                const checked = event.target.checked;
                setUseGlobalValues((current) => ({ ...current, [parameter.id]: checked }));
                if (checked && !selectedBindingAvailable) {
                  setValueBindings((current) => ({
                    ...current,
                    [parameter.id]: compatibleCredentials[0]?.name ?? "",
                  }));
                }
              }}
              disabled={busy}
              aria-label={`${parameter.label} 引用全局值`}
            />
            <span>引用</span>
          </label>
        </div>
        {usingGlobal ? (
          <select
            className="studio-input"
            aria-label={`${parameter.label} 使用的全局值`}
            value={selectedBindingAvailable ? selectedBinding : ""}
            onChange={(event) => setValueBindings((current) => ({
              ...current,
              [parameter.id]: event.target.value,
            }))}
            disabled={busy}
          >
            <option value="">{compatibleCredentials.length ? "选择全局值" : "暂无匹配的全局值"}</option>
            {compatibleCredentials.map((credential) => (
              <option key={credential.name} value={credential.name}>{credential.name}</option>
            ))}
          </select>
        ) : renderLiteralEditor()}
      </div>
    );
  };

  const renderSecretParameter = (parameter: LoomArtManagementParameter) => {
    const draft = secretDrafts[parameter.id] ?? {
      useGlobal: false,
      credential: "",
      storedCredential: "",
      value: "",
    };
    const compatibleCredentials = globalCredentials.filter((credential) => (
      credentialMatchesArtParameter(parameter, credential)
    ));
    const globalCredentialExists = compatibleCredentials.some((credential) => (
      credential.name === draft.credential
    ));
    const setDraft = (next: Partial<ArtSecretDraft>) => setSecretDrafts((current) => ({
      ...current,
      [parameter.id]: { ...(current[parameter.id] ?? draft), ...next },
    }));
    return (
      <div className="art-edit-dialog__secret" key={parameter.id}>
        <div className="art-edit-dialog__secret-label">
          <span>{parameter.label}{parameter.required ? " *" : ""}</span>
          <label className="art-edit-dialog__binding-toggle">
            <input
              type="checkbox"
              checked={draft.useGlobal}
              onChange={(event) => setDraft({
                useGlobal: event.target.checked,
                credential: event.target.checked
                  ? globalCredentialExists ? draft.credential : compatibleCredentials[0]?.name ?? ""
                  : draft.credential,
              })}
              disabled={busy}
              aria-label={`${parameter.label} 引用全局机密`}
            />
            <span>引用</span>
          </label>
        </div>
        <div className="art-edit-dialog__secret-controls">
          {draft.useGlobal ? (
            <select
              className="studio-input"
              aria-label={`${parameter.label} 使用的全局机密`}
              value={globalCredentialExists ? draft.credential : ""}
              onChange={(event) => setDraft({ credential: event.target.value })}
              disabled={busy}
            >
              <option value="">{compatibleCredentials.length ? "选择全局机密" : "暂无匹配的全局机密"}</option>
              {compatibleCredentials.map((credential) => (
                <option key={credential.name} value={credential.name}>{credential.name}</option>
              ))}
            </select>
          ) : (
            <input
              className="studio-input"
              type="password"
              autoComplete="new-password"
              aria-label={`${parameter.label} 的默认机密值`}
              placeholder={draft.storedCredential ? "已保存；输入新值可替换" : "输入机密值"}
              value={draft.value}
              onChange={(event) => setDraft({ value: event.target.value })}
              disabled={busy}
            />
          )}
        </div>
        {!draft.useGlobal && draft.storedCredential && !draft.value ? (
          <span className="art-edit-dialog__secret-status">已安全保存到当前 Art</span>
        ) : null}
      </div>
    );
  };

  return (
    <div
      className="framework-dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (!busy && event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="framework-dialog art-edit-dialog"
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="art-edit-dialog-title"
      >
        <header className="framework-dialog__header">
          <h2 id="art-edit-dialog-title">编辑</h2>
          <button
            className="art-card-action"
            type="button"
            aria-label="关闭编辑"
            title="关闭"
            onClick={onClose}
            disabled={busy}
          >
            <ArtIcon kind="close" />
          </button>
        </header>
        {visibleError ? <p className="error-text" role="alert">{visibleError}</p> : null}
        {loading || !management ? <div className="art-edit-dialog__loading">加载中</div> : null}
        <form
          className="art-edit-dialog__form"
          onSubmit={(event) => {
            event.preventDefault();
            void submit("save");
          }}
        >
          {management ? (
            <div className="art-edit-dialog__scroll">
              <div className="art-edit-dialog__overview">
                <section className="art-edit-dialog__section art-edit-dialog__identity" aria-label="Art 信息">
                  <div className="art-edit-dialog__identity-meta">
                    <span
                      className="art-edit-dialog__publisher"
                      title={displayIdentity.publisher.id}
                    >
                      <ArtPublisherIcon publisher={displayIdentity.publisher} />
                      <strong>{displayIdentity.publisher.name}</strong>
                    </span>
                    <strong
                      className="art-edit-dialog__english-name"
                      title={displayIdentity.englishName}
                    >
                      {displayIdentity.englishName}
                    </strong>
                    {displayIdentity.globalId ? (
                      <code
                        className="art-edit-dialog__id"
                        aria-label={`Art 编号 ${displayIdentity.globalId}`}
                        title={displayIdentity.globalId}
                      >
                        {displayIdentity.globalId}
                      </code>
                    ) : null}
                  </div>
                  <input
                    className="studio-input art-edit-dialog__name"
                    ref={nameInputRef}
                    aria-label="名称"
                    placeholder="Art 名称"
                    value={name}
                    onChange={(event) => setName(event.target.value)}
                    disabled={busy || !management.canEditIdentity}
                    required
                  />
                  <textarea
                    className="studio-input art-edit-dialog__description"
                    aria-label="描述"
                    placeholder="描述"
                    value={description}
                    onChange={(event) => setDescription(event.target.value)}
                    disabled={busy || !management.canEditIdentity}
                  />
                </section>

                <section className="art-edit-dialog__section art-edit-dialog__version" aria-label="版本">
                  <div className="art-edit-dialog__version-primary">
                    <strong
                      className="art-edit-dialog__current-version"
                      aria-label={`当前版本 ${management.currentVersion}`}
                    >
                      {management.currentVersion}
                    </strong>
                    <label className="art-edit-dialog__toggle">
                      <input
                        type="checkbox"
                        aria-label="自动更新"
                        checked={autoUpdate}
                        onChange={(event) => setAutoUpdate(event.target.checked)}
                        disabled={busy}
                      />
                      <span>自动更新</span>
                    </label>
                  </div>
                  <div className="art-edit-dialog__version-action">
                    <div className="art-edit-dialog__version-select">
                      {management.updateAvailable ? (
                        <span className="art-edit-dialog__version-new" aria-label="有新版本">new</span>
                      ) : null}
                      <select
                        className="studio-input"
                        aria-label="目标版本"
                        value={targetVersion}
                        onChange={(event) => setTargetVersion(event.target.value)}
                        disabled={busy || autoUpdate}
                      >
                        {management.availableVersions.map((version) => <option key={version} value={version}>{version}</option>)}
                      </select>
                    </div>
                    <button
                      className="ghost-button art-edit-dialog__version-update"
                      type="button"
                      disabled={busy || autoUpdate || !targetVersion || targetVersion === management.currentVersion}
                      onClick={() => void submit("update")}
                    >
                      {busyAction === "update" ? "更新中" : "更新"}
                    </button>
                  </div>
                </section>
              </div>

              <div className={`art-edit-dialog__workspace${hasSplitWorkspace ? "" : " art-edit-dialog__workspace--single"}`}>
                {requiredParameters.length ? (
                  <section className="art-edit-dialog__section art-edit-dialog__parameters" aria-label="参数">
                    <div className="art-edit-dialog__fields">{requiredParameters.map(renderParameter)}</div>
                  </section>
                ) : null}

                {secretParameters.length ? (
                  <section className="art-edit-dialog__section art-edit-dialog__secrets" aria-label="机密参数">
                    <div className="art-edit-dialog__secret-list">
                      {secretParameters.map(renderSecretParameter)}
                    </div>
                  </section>
                ) : null}

                {optionalParameters.length ? (
                  <details className="art-edit-dialog__section art-edit-dialog__optional">
                    <summary aria-label="可选参数">更多</summary>
                    <div className="art-edit-dialog__fields art-edit-dialog__fields--optional">
                      {optionalParameters.map(renderParameter)}
                    </div>
                  </details>
                ) : null}
              </div>
            </div>
          ) : null}
          <div className="art-edit-dialog__actions">
            <button className="ghost-button" type="button" onClick={onClose} disabled={busy}>取消</button>
            <button className="signal-button" type="submit" disabled={loading || !management || busy || (management.canEditIdentity && !name.trim())}>
              {busyAction === "save" ? "保存中" : "保存"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
