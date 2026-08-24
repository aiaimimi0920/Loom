export const MAX_IMPORT_TEXT_CHARS = 1024 * 1024;
export const MAX_TEMPLATE_DEPTH = 64;
export const MAX_TEMPLATE_VALUES = 100_000;
export const MAX_TEMPLATE_PORTS = 4096;
export const MAX_COMMAND_TOKENS = 65_536;
export const MAX_WORKFLOW_YAML_CHARS = 4 * 1024 * 1024;
export const MAX_WORKFLOW_NODES = 4096;
export const MAX_WORKFLOW_NODE_FIELDS = 4096;
export const MAX_WORKFLOW_EDGES = 16_384;
export const MAX_WORKFLOW_PARAMETERS = 65_536;

export const safeName = (value: string, fallback = "value") =>
  value.replace(/[^a-zA-Z0-9_-]/g, "_").replace(/^_+|_+$/g, "") || fallback;

export const stripQuotes = (value: string) => value.replace(/^['"]|['"]$/g, "");

export const defineOwnValue = <T>(target: Record<string, T>, key: string, value: T): void => {
  // Assignment to __proto__ invokes Object.prototype's legacy setter. A data property preserves the
  // imported key without changing the target's prototype.
  Object.defineProperty(target, key, {
    configurable: true,
    enumerable: true,
    value,
    writable: true,
  });
};

export const cloneStringRecord = (value: Record<string, string>): Record<string, string> => {
  const output: Record<string, string> = {};
  for (const [key, item] of Object.entries(value)) defineOwnValue(output, key, item);
  return output;
};

export const isConfigured = (value: unknown) =>
  value !== undefined && value !== null && !(typeof value === "string" && value.trim() === "");
