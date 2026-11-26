// Art settings validation and serialization; presentation primitives do not own this policy.
import { uiFormat, uiText } from "../../i18n/index.ts";
import type {
  LoomArtManagement,
  LoomArtManagementParameter,
  LoomArtManagementSettingsInput,
  LoomCredentialSummary,
  LoomToolDefinition,
} from "../../services/loomApi.ts";

export interface ArtSecretDraft {
  useGlobal: boolean;
  credential: string;
  storedCredential: string;
  value: string;
}

export interface ArtEditDialogProps {
  tool: LoomToolDefinition | null;
  management: LoomArtManagement | null;
  loading: boolean;
  busyAction: "save" | "update" | null;
  error: string | null;
  onClose: () => void;
  onSave: (input: LoomArtManagementSettingsInput) => Promise<void>;
  onUpdate: (version: string, input: LoomArtManagementSettingsInput) => Promise<void>;
}

export interface ArtEditSettingsDraft {
  name: string;
  description: string;
  autoUpdate: boolean;
  parameterValues: Record<string, unknown>;
  valueBindings: Record<string, string>;
  useGlobalValues: Record<string, boolean>;
  secretDrafts: Record<string, ArtSecretDraft>;
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

const valuePresent = (value: unknown) =>
  value !== undefined && value !== null && (typeof value !== "string" || value.trim() !== "");

export function buildArtEditSettings(
  management: LoomArtManagement | null,
  globalCredentials: LoomCredentialSummary[],
  {
    name,
    description,
    autoUpdate,
    parameterValues,
    valueBindings,
    useGlobalValues,
    secretDrafts,
  }: ArtEditSettingsDraft,
): LoomArtManagementSettingsInput {
  if (!management) throw new Error(uiText("art.edit.error.notLoaded"));
  const defaults: Record<string, unknown> = {};
  const nextValueBindings: Record<string, string> = {};
  for (const parameter of management.parameters.filter((candidate) => !candidate.secret)) {
    if (useGlobalValues[parameter.id]) {
      const binding = valueBindings[parameter.id];
      if (!binding) throw new Error(uiFormat("art.edit.error.selectGlobalValue", { label: parameter.label }));
      const credential = globalCredentials.find((candidate) => candidate.name === binding);
      if (!credential || !credentialMatchesArtParameter(parameter, credential)) {
        throw new Error(uiFormat("art.edit.error.invalidGlobalValue", { label: parameter.label }));
      }
      nextValueBindings[parameter.id] = binding;
      continue;
    }
    const draft = parameterValues[parameter.id];
    if (parameter.required && !valuePresent(draft)) {
      throw new Error(uiFormat("art.edit.error.requiredParameter", { label: parameter.label }));
    }
    if (!valuePresent(draft)) continue;
    if (parameter.parameterType === "json" && typeof draft === "string") {
      try {
        defaults[parameter.id] = JSON.parse(draft);
      } catch {
        throw new Error(uiFormat("art.edit.error.invalidJson", { label: parameter.label }));
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
      if (!draft.credential) {
        throw new Error(uiFormat("art.edit.error.selectGlobalSecret", { label: parameter.label }));
      }
      const credential = globalCredentials.find((candidate) => candidate.name === draft.credential);
      if (!credential || !credentialMatchesArtParameter(parameter, credential)) {
        throw new Error(uiFormat("art.edit.error.invalidGlobalSecret", { label: parameter.label }));
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
    if (parameter.required) throw new Error(uiFormat("art.edit.error.requiredParameter", { label: parameter.label }));
  }
  return {
    ...(management.canEditIdentity ? { name: name.trim(), description } : {}),
    autoUpdate,
    defaults,
    valueBindings: nextValueBindings,
    credentialBindings: bindings,
    ...(Object.keys(secretValues).length ? { secretValues } : {}),
  };
}
