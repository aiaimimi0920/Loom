import type {
  LoomArtRuntimeManifest,
  LoomFramework,
  LoomFrameworkAuthoringField,
  LoomFrameworkAuthoringPort,
  LoomToolDefinition,
  LoomToolExecution,
} from "./loomApi";
import { frameworkFilterLabel } from "./artHubUi.ts";

export interface ArtAuthoringDraft {
  id: string;
  name: string;
  description: string;
  values: Record<string, unknown>;
  inputs?: LoomFrameworkAuthoringPort[];
  outputs?: LoomFrameworkAuthoringPort[];
}

export interface AuthoredArtPackage {
  tool: LoomToolDefinition;
  runtime?: LoomArtRuntimeManifest;
}

const asString = (value: unknown) => typeof value === "string" ? value.trim() : "";
const executionFieldIds = new Set([
  "endpoint",
  "method",
  "headers",
  "body",
  "serverId",
  "toolName",
  "arguments",
  "workflowId",
  "runtimeCommand",
  "runtimeArgs",
]);

const splitArguments = (value: unknown): string[] => {
  const text = asString(value);
  if (!text) return [];
  if (text.includes("\n")) {
    return text.split(/\r?\n/).map((item) => item.trim()).filter(Boolean);
  }
  const result: string[] = [];
  let current = "";
  let quote: "'" | '"' | null = null;
  let escaping = false;
  for (const character of text) {
    if (escaping) {
      current += character;
      escaping = false;
      continue;
    }
    if (character === "\\" && quote !== "'") {
      escaping = true;
      continue;
    }
    if (character === "'" || character === '"') {
      if (quote === character) quote = null;
      else if (quote === null) quote = character;
      else current += character;
      continue;
    }
    if (/\s/.test(character) && quote === null) {
      if (current) result.push(current);
      current = "";
      continue;
    }
    current += character;
  }
  if (escaping) current += "\\";
  if (current) result.push(current);
  return result;
};

const frameworkVersionRequirement = (version?: string | null) => {
  const match = version?.match(/^(\d+)\.(\d+)/);
  return match ? `^${match[1]}.${match[2]}` : undefined;
};

const portDefinition = (port: LoomFrameworkAuthoringPort) => ({
  name: port.name,
  label: port.label || port.name,
  type: port.type || "string",
  executionType: port.executionType || "string",
  ...(port.required ? { required: true } : {}),
  ...(port.exposePort ? { exposePort: true } : {}),
});

const requiredValue = (
  values: Record<string, unknown>,
  field: string,
  framework: LoomFramework,
) => {
  const value = asString(values[field]);
  if (!value) throw new Error(`${frameworkFilterLabel(framework)} 需要字段 ${field}。`);
  return value;
};

const executionForFramework = (
  framework: LoomFramework,
  values: Record<string, unknown>,
): { execution: LoomToolExecution; runtime?: LoomArtRuntimeManifest } => {
  switch (framework.id) {
    case "cloud_api":
      return {
        execution: {
          type: "cloud_api",
          endpoint: requiredValue(values, "endpoint", framework),
          method: asString(values.method).toUpperCase() || "POST",
          headers: typeof values.headers === "string"
            ? values.headers
            : JSON.stringify(values.headers ?? {}),
          body: typeof values.body === "string" ? values.body : JSON.stringify(values.body ?? {}),
        },
      };
    case "mcp":
      return {
        execution: {
          type: "mcp",
          serverId: requiredValue(values, "serverId", framework),
          toolName: requiredValue(values, "toolName", framework),
        },
      };
    case "workflow":
      return {
        execution: {
          type: "workflow",
          workflowId: requiredValue(values, "workflowId", framework),
        },
      };
    default: {
      const command = requiredValue(values, "runtimeCommand", framework);
      return {
        execution: {
          type: "framework_art",
          framework: framework.qualifiedId || framework.id,
        },
        runtime: {
          protocolVersion: "loom.art.runtime.v1",
          entry: {
            command,
            args: splitArguments(values.runtimeArgs),
          },
        },
      };
    }
  }
};

export const defaultAuthoringValues = (
  fields: LoomFrameworkAuthoringField[] = [],
): Record<string, unknown> => Object.fromEntries(
  fields.map((field) => [field.id, field.default ?? (field.type === "boolean" ? false : "")]),
);

export function buildAuthoredArtPackage(
  framework: LoomFramework,
  draft: ArtAuthoringDraft,
): AuthoredArtPackage {
  const id = draft.id.trim();
  if (!/^[A-Za-z0-9_-][A-Za-z0-9_.-]*$/.test(id) || id.includes("..")) {
    throw new Error("Art ID 必须是安全的单段标识符。");
  }
  const schema = framework.authoringSchema;
  if (!framework.installed || !framework.enabled || !framework.ready || !schema) {
    throw new Error(`框架 ${frameworkFilterLabel(framework)} 未安装、未启用、未就绪或没有 authoring schema。`);
  }
  for (const field of schema.fields ?? []) {
    if (field.required && (draft.values[field.id] === undefined || asString(draft.values[field.id]) === "")) {
      throw new Error(`${field.label || field.id} 是必填项。`);
    }
  }
  const secretFields = new Set(
    (schema.fields ?? []).filter((field) => field.secret || field.type === "secret").map((field) => field.id),
  );
  const secretExecutionField = [...secretFields].find((field) => executionFieldIds.has(field));
  if (secretExecutionField) {
    throw new Error(`机密字段 ${secretExecutionField} 不能直接作为 Art 执行配置。`);
  }
  const persistedValues = Object.fromEntries(
    Object.entries(draft.values).filter(([field]) => !secretFields.has(field)),
  );
  const credentialBindings = Object.fromEntries(
    Object.entries(draft.values)
      .filter(([field, value]) => secretFields.has(field) && asString(value))
      .map(([field, value]) => [field, asString(value)]),
  );
  const { execution, runtime } = executionForFramework(framework, draft.values);
  const frameworkId = framework.qualifiedId || framework.id;
  const frameworkVersion = frameworkVersionRequirement(framework.version);
  const tool: LoomToolDefinition = {
    id,
    name: draft.name.trim() || schema.title || id,
    description: draft.description.trim() || schema.description || framework.description,
    enabled: true,
    execution,
    inputs: (draft.inputs ?? schema.inputs ?? []).map(portDefinition),
    outputs: (draft.outputs ?? schema.outputs ?? []).map(portDefinition),
    params: [],
    metadata: {
      packageSecurity: { version: "0.1.0" },
      dependencies: {
        framework: frameworkId,
        ...(frameworkVersion ? { frameworkVersion } : {}),
      },
      authoring: {
        schemaVersion: schema.schemaVersion,
        frameworkId,
        origin: "local",
        owner: "local-user",
        values: persistedValues,
        ...(Object.keys(credentialBindings).length ? { credentialBindings } : {}),
      },
    },
  };
  return { tool, runtime };
}
