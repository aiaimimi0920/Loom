// Owns editor hydration, per-parameter drafts and atomic save/update submission.
import { useEffect, useState } from "react";
import { artDisplayIdentity } from "../../services/artHubUi";
import { buildArtEditSettings, type ArtEditDialogProps, type ArtSecretDraft } from "./artEditModel";

export function useArtEditDialogController({
  tool,
  management,
  busyAction,
  error,
  onSave,
  onUpdate,
}: ArtEditDialogProps) {
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
          typeof document === "undefined" ? "zh-CN" : document.documentElement.lang || window.navigator.language,
        )
      : null;
    setName(
      management.canEditIdentity
        ? management.name || tool?.name || ""
        : identity?.localizedName || management.name || tool?.name || "",
    );
    setDescription(
      management.canEditIdentity
        ? management.description || tool?.description || ""
        : identity?.localizedDescription || management.description || tool?.description || "",
    );
    setAutoUpdate(management.autoUpdate !== false);
    setTargetVersion(management.currentVersion);
    setParameterValues(
      Object.fromEntries(
        management.parameters
          .filter((parameter) => !parameter.secret)
          .map((parameter) => {
            const hasSavedValue = Object.prototype.hasOwnProperty.call(management.defaults, parameter.id);
            const value = hasSavedValue
              ? management.defaults[parameter.id]
              : parameter.required
                ? parameter.default
                : "";
            return [
              parameter.id,
              parameter.parameterType === "json" && value !== undefined && value !== ""
                ? JSON.stringify(value, null, 2)
                : (value ?? (parameter.required && parameter.parameterType === "boolean" ? false : "")),
            ];
          }),
      ),
    );
    setValueBindings({ ...management.valueBindings });
    setUseGlobalValues(
      Object.fromEntries(
        management.parameters
          .filter((parameter) => !parameter.secret)
          .map((parameter) => [parameter.id, Boolean(management.valueBindings[parameter.id])]),
      ),
    );
    const credentialsByName = new Map(
      management.availableCredentials.map((credential) => [credential.name, credential]),
    );
    setSecretDrafts(
      Object.fromEntries(
        management.parameters
          .filter((parameter) => parameter.secret)
          .map((parameter) => {
            const binding = management.credentialBindings[parameter.id] || "";
            const summary = credentialsByName.get(binding);
            const storedCredential =
              summary?.scope.artId === management.artId || binding.startsWith("loom-art-secret-") ? binding : "";
            return [
              parameter.id,
              {
                useGlobal: Boolean(binding && !storedCredential),
                credential: binding && !storedCredential ? binding : "",
                storedCredential,
                value: "",
              } satisfies ArtSecretDraft,
            ];
          }),
      ),
    );
    setFormError(null);
  }, [management, tool]);
  const requiredParameters =
    management?.parameters.filter((parameter) => parameter.required && !parameter.secret) ?? [];
  const optionalParameters =
    management?.parameters.filter((parameter) => !parameter.required && !parameter.secret) ?? [];
  const secretParameters = management?.parameters.filter((parameter) => parameter.secret) ?? [];
  const hasSplitWorkspace = requiredParameters.length > 0 && secretParameters.length > 0;
  const globalCredentials =
    management?.availableCredentials.filter((credential) => !credential.scope.frameworkId && !credential.scope.artId) ??
    [];
  const visibleError = formError || error;
  const submit = async (action: "save" | "update") => {
    setFormError(null);
    try {
      const input = buildArtEditSettings(management, globalCredentials, {
        name,
        description,
        autoUpdate,
        parameterValues,
        valueBindings,
        useGlobalValues,
        secretDrafts,
      });
      if (action === "update") {
        await onUpdate(targetVersion, input);
      } else {
        await onSave(input);
      }
    } catch (submitError) {
      setFormError(submitError instanceof Error ? submitError.message : "无法保存 Art 设置。");
    }
  };
  return {
    name,
    description,
    autoUpdate,
    targetVersion,
    parameterValues,
    valueBindings,
    useGlobalValues,
    secretDrafts,
    setName,
    setDescription,
    setAutoUpdate,
    setTargetVersion,
    setParameterValues,
    setValueBindings,
    setUseGlobalValues,
    setSecretDrafts,
    busy,
    requiredParameters,
    optionalParameters,
    secretParameters,
    hasSplitWorkspace,
    globalCredentials,
    visibleError,
    submit,
  };
}
