import {
  MAX_IMPORT_TEXT_CHARS,
  MAX_TEMPLATE_DEPTH,
  MAX_TEMPLATE_PORTS,
  MAX_TEMPLATE_VALUES,
  defineOwnValue,
  safeName,
} from "./shared.ts";
import type { ParsedPort, WorkflowPortType } from "./types.ts";

const fileExtensions = new Set([
  "png", "jpg", "jpeg", "webp", "gif", "bmp", "tiff", "svg",
  "mp3", "wav", "aac", "flac", "m4a", "ogg",
  "mp4", "mkv", "avi", "mov", "webm",
  "txt", "json", "pdf", "zip",
]);

const isFilePath = (value: string) => {
  const extension = value.split(".").pop()?.toLowerCase();
  return extension ? fileExtensions.has(extension) : false;
};

const inferPrimitiveType = (value: unknown): WorkflowPortType => {
  if (typeof value === "number") return Number.isInteger(value) ? "int" : "float";
  if (typeof value === "boolean") return "boolean";
  if (typeof value === "string" && isFilePath(value)) return "file";
  return "string";
};

type JsonContainer = unknown[] | Record<string, unknown>;
type TemplateDirection = "input" | "output";

interface TemplateFrame {
  value: unknown;
  parent: JsonContainer;
  key: string | number;
  depth: number;
  namePath: string;
  jsonPath: string;
}

const setContainerValue = (parent: JsonContainer, key: string | number, value: unknown) => {
  if (Array.isArray(parent)) {
    parent[Number(key)] = value;
  } else {
    defineOwnValue(parent, String(key), value);
  }
};

const templateJsonValues = (value: unknown, ports: ParsedPort[], direction: TemplateDirection): unknown => {
  const holder: Record<string, unknown> = {};
  const stack: TemplateFrame[] = [{
    value,
    parent: holder,
    key: "value",
    depth: 1,
    namePath: "",
    jsonPath: "",
  }];
  let valuesSeen = 0;

  while (stack.length) {
    const frame = stack.pop()!;
    valuesSeen += 1;
    if (valuesSeen > MAX_TEMPLATE_VALUES) throw new RangeError("Imported JSON has too many values.");
    if (frame.depth > MAX_TEMPLATE_DEPTH) throw new RangeError("Imported JSON is nested too deeply.");

    if (Array.isArray(frame.value)) {
      const output = new Array<unknown>(frame.value.length);
      setContainerValue(frame.parent, frame.key, output);
      for (let index = frame.value.length - 1; index >= 0; index -= 1) {
        stack.push({
          value: frame.value[index],
          parent: output,
          key: index,
          depth: frame.depth + 1,
          namePath: frame.namePath ? `${frame.namePath}_${index}` : String(index),
          jsonPath: `${frame.jsonPath}[${index}]`,
        });
      }
      continue;
    }

    if (frame.value && typeof frame.value === "object") {
      const output: Record<string, unknown> = {};
      setContainerValue(frame.parent, frame.key, output);
      const entries = Object.entries(frame.value);
      for (let index = entries.length - 1; index >= 0; index -= 1) {
        const [childKey, childValue] = entries[index];
        stack.push({
          value: childValue,
          parent: output,
          key: childKey,
          depth: frame.depth + 1,
          namePath: frame.namePath ? `${frame.namePath}_${childKey}` : childKey,
          jsonPath: frame.jsonPath ? `${frame.jsonPath}.${childKey}` : childKey,
        });
      }
      continue;
    }

    if (frame.value === undefined || frame.value === null) {
      setContainerValue(frame.parent, frame.key, frame.value);
      continue;
    }
    if (ports.length >= MAX_TEMPLATE_PORTS) throw new RangeError("Imported JSON produces too many ports.");

    const name = safeName(frame.namePath, direction === "input" ? "param" : "result");
    ports.push({
      name,
      type: inferPrimitiveType(frame.value),
      originalValue: String(frame.value),
      isInput: direction === "input",
      label: name,
      ...(direction === "input" ? { default: String(frame.value) } : { jsonPath: frame.jsonPath }),
    });
    setContainerValue(
      frame.parent,
      frame.key,
      direction === "input" ? `{{inputs.${name}.value}}` : `{{outputs.${name}.value}}`,
    );
  }

  return holder.value;
};

export const templateObjectInputs = (value: unknown, ports: ParsedPort[]): unknown =>
  templateJsonValues(value, ports, "input");

const templateObjectOutputs = (value: unknown, ports: ParsedPort[]): unknown =>
  templateJsonValues(value, ports, "output");

export function parseTemplate(template: string): ParsedPort[] {
  if (template.length > MAX_IMPORT_TEXT_CHARS) return [];
  const ports: ParsedPort[] = [];
  const seen = new Set<string>();
  const regex =
    /\{\{(?:(inputs|outputs)\.)?([a-zA-Z0-9_-]+)(?:\.(path|value))?\}\}|\{\{(-{1,2}[a-zA-Z0-9_-]+)\}\}/g;
  let match: RegExpExecArray | null;

  while ((match = regex.exec(template)) !== null && ports.length < MAX_TEMPLATE_PORTS) {
    const flagName = match[4]?.replace(/^-+/, "");
    const category = match[1] || "inputs";
    const name = flagName || match[2];
    if (!name) continue;
    const key = `${category}.${name}`;
    if (seen.has(key)) continue;
    seen.add(key);

    const isInput = category !== "outputs";
    const property = match[3];
    const lower = name.toLowerCase();
    const type: WorkflowPortType = flagName
      ? "boolean"
      : property === "path" || lower.includes("file") || lower.includes("path")
        ? "file"
        : lower.includes("image") || lower.includes("img") || lower.includes("photo")
          ? "image"
          : lower.includes("width") || lower.includes("height") || lower.includes("seed")
            ? "int"
            : lower.includes("scale") || lower.includes("strength") || lower.includes("ratio")
              ? "float"
              : "string";

    ports.push({
      name,
      type,
      originalValue: "",
      isInput,
      label: flagName ? match[4] : name,
      default: flagName ? "false" : "",
    });
  }

  return ports;
}

export function autoTemplateResponse(jsonString: string): { templatedJson: string; ports: ParsedPort[] } {
  const ports: ParsedPort[] = [];
  if (jsonString.length > MAX_IMPORT_TEXT_CHARS) return { templatedJson: jsonString, ports };
  try {
    const parsed = JSON.parse(jsonString) as unknown;
    const templated = templateObjectOutputs(parsed, ports);
    return { templatedJson: JSON.stringify(templated, null, 2), ports };
  } catch {
    return { templatedJson: jsonString, ports: [] };
  }
}
