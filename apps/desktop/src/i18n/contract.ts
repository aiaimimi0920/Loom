// Validates message templates and `{ $param }` variable contracts across locale shards.

export type MessageParameters = ReadonlySet<string>;
export type MessageContracts = ReadonlyMap<string, MessageParameters>;
export type LocaleShard = Readonly<Record<string, string>>;

const VARIABLE_PATTERN = /^[A-Za-z][A-Za-z0-9_]*$/;

export interface ParsedTemplate {
  readonly segments: ReadonlyArray<{ readonly kind: "text"; readonly value: string } | { readonly kind: "var"; readonly name: string }>;
  readonly parameters: ReadonlySet<string>;
}

export function parseMessageTemplate(id: string, template: string): ParsedTemplate {
  const segments: Array<{ kind: "text"; value: string } | { kind: "var"; name: string }> = [];
  const parameters = new Set<string>();
  let cursor = 0;

  while (cursor < template.length) {
    const open = template.indexOf("{", cursor);
    const close = template.indexOf("}", cursor);
    if (close !== -1 && (open === -1 || close < open)) {
      throw new Error(`Unmatched closing brace in interface message "${id}"`);
    }
    if (open === -1) {
      segments.push({ kind: "text", value: template.slice(cursor) });
      break;
    }
    if (open > cursor) {
      segments.push({ kind: "text", value: template.slice(cursor, open) });
    }
    const matchingClose = template.indexOf("}", open + 1);
    if (matchingClose === -1) {
      throw new Error(`Unclosed placeholder in interface message "${id}"`);
    }
    const rawInner = template.slice(open + 1, matchingClose).trim();
    const varName = rawInner.startsWith("$") ? rawInner.slice(1).trim() : rawInner;
    if (!VARIABLE_PATTERN.test(varName)) {
      throw new Error(`Invalid placeholder "${rawInner}" in interface message "${id}"`);
    }
    segments.push({ kind: "var", name: varName });
    parameters.add(varName);
    cursor = matchingClose + 1;
  }

  return { segments, parameters };
}

export function mergeLocaleShards(languageCode: string, shards: readonly LocaleShard[]): Map<string, ParsedTemplate> {
  const merged = new Map<string, ParsedTemplate>();
  for (const shard of shards) {
    for (const [id, value] of Object.entries(shard)) {
      if (!id.trim()) {
        throw new Error(`${languageCode}: empty interface message ID`);
      }
      if (merged.has(id)) {
        throw new Error(`${languageCode}: duplicate interface message "${id}"`);
      }
      merged.set(id, parseMessageTemplate(id, value));
    }
  }
  return merged;
}
