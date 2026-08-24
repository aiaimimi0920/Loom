import { MAX_TEMPLATE_PORTS, safeName } from "./shared.ts";
import type { McpToolSchemaImportResult, ParsedPort, WorkflowPortType } from "./types.ts";

const schemaTypeToPortType = (name: string, schema: Record<string, unknown>): WorkflowPortType => {
  const lowerName = name.toLowerCase();
  const schemaType = schema.type;
  const format = typeof schema.format === "string" ? schema.format.toLowerCase() : "";
  const description = typeof schema.description === "string" ? schema.description.toLowerCase() : "";

  if (schemaType === "integer") return "int";
  if (schemaType === "number") return "float";
  if (schemaType === "boolean") return "boolean";
  if (lowerName.includes("image") || lowerName.includes("screenshot") || description.includes("image") || format.includes("binary")) {
    return "image";
  }
  if (lowerName.includes("path") || lowerName.includes("file") || format === "uri") return "file";
  return "string";
};

const portExecutionType = (type: WorkflowPortType, direction: "input" | "output") => {
  if (type === "image") return direction === "input" ? "image_path" : "image_buffer";
  if (type === "file") return "image_path";
  if (type === "boolean") return "bool";
  if (type === "int" || type === "float") return "number";
  return "string";
};

const readSchema = (tool: Record<string, unknown>) => {
  const inputSchema = tool.input_schema ?? tool.inputSchema;
  return inputSchema && typeof inputSchema === "object" && !Array.isArray(inputSchema)
    ? inputSchema as Record<string, unknown>
    : null;
};

export function portsFromMcpToolSchema(tool: unknown): McpToolSchemaImportResult | null {
  if (!tool || typeof tool !== "object" || Array.isArray(tool)) return null;
  const record = tool as Record<string, unknown>;
  const toolName = typeof record.name === "string" && record.name.trim() ? record.name.trim() : "mcp_tool";
  const schema = readSchema(record);
  const properties = schema?.properties && typeof schema.properties === "object" && !Array.isArray(schema.properties)
    ? schema.properties as Record<string, unknown>
    : {};

  const suggestedInputs: ParsedPort[] = [];
  for (const [name, property] of Object.entries(properties)) {
    if (suggestedInputs.length >= MAX_TEMPLATE_PORTS) break;
    if (!property || typeof property !== "object" || Array.isArray(property)) continue;
    const propertyRecord = property as Record<string, unknown>;
    const type = schemaTypeToPortType(name, propertyRecord);
    suggestedInputs.push({
      name: safeName(name),
      label: typeof propertyRecord.title === "string" && propertyRecord.title.trim() ? propertyRecord.title.trim() : name,
      type,
      originalValue: "",
      isInput: true,
      default: propertyRecord.default === undefined ? "" : String(propertyRecord.default),
      executionType: portExecutionType(type, "input"),
    });
  }

  const outputType = /screenshot|image|ocr|vision/i.test(toolName) ? "image" : "string";
  return {
    toolName,
    suggestedInputs,
    suggestedOutputs: [{
      name: outputType === "image" ? "image" : "result",
      label: outputType === "image" ? "Image" : "Result",
      type: outputType,
      originalValue: "",
      isInput: false,
      default: "",
      executionType: portExecutionType(outputType, "output"),
    }],
  };
}
