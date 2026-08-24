import {
  MAX_WORKFLOW_EDGES,
  MAX_WORKFLOW_NODES,
  MAX_WORKFLOW_NODE_FIELDS,
  MAX_WORKFLOW_PARAMETERS,
  MAX_WORKFLOW_YAML_CHARS,
  cloneStringRecord,
  defineOwnValue,
  safeName,
  stripQuotes,
} from "./shared.ts";
import type { WorkflowGraphLite, WorkflowGraphNodePatch, WorkflowStudioNode } from "./types.ts";
import { assertWorkflowGraphLimits, assertWorkflowNodeLimits, boundedWithEntries } from "./validation.ts";

const newGraph = (): WorkflowGraphLite => ({ name: "未命名工作流", description: "", nodes: [] });

const yamlLines = function* (yaml: string): Generator<string> {
  let start = 0;
  for (let index = 0; index < yaml.length; index += 1) {
    if (yaml.charCodeAt(index) !== 10) continue;
    const end = index > start && yaml.charCodeAt(index - 1) === 13 ? index - 1 : index;
    yield yaml.slice(start, end);
    start = index + 1;
  }
  if (start <= yaml.length) yield yaml.slice(start);
};

const parseNeeds = (value: string): string[] => {
  const unwrapped = value.replace(/^\[|\]$/g, "");
  const needs: string[] = [];
  let start = 0;
  for (let index = 0; index <= unwrapped.length; index += 1) {
    if (index < unwrapped.length && unwrapped.charCodeAt(index) !== 44) continue;
    const item = stripQuotes(unwrapped.slice(start, index).trim());
    start = index + 1;
    if (!item) continue;
    if (needs.length >= MAX_WORKFLOW_NODE_FIELDS) {
      throw new RangeError("Workflow node has too many dependencies.");
    }
    needs.push(item);
  }
  return needs;
};

export function parseWorkflowYamlLite(yaml: string): WorkflowGraphLite {
  if (yaml.length > MAX_WORKFLOW_YAML_CHARS) throw new RangeError("Workflow YAML is too large.");
  const graph = newGraph();
  let current: WorkflowStudioNode | null = null;
  let currentWithFields = 0;
  let parameterFields = 0;
  let inWith = false;

  let dependencyEdges = 0;
  for (const rawLine of yamlLines(yaml)) {
    const line = rawLine.replace(/\t/g, "  ");
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;

    const topLevel = /^([a-zA-Z0-9_-]+):\s*(.*)$/.exec(trimmed);
    if (topLevel && !rawLine.startsWith(" ")) {
      if (topLevel[1] === "name") graph.name = stripQuotes(topLevel[2]) || graph.name;
      if (topLevel[1] === "description") graph.description = stripQuotes(topLevel[2]);
      inWith = false;
      continue;
    }

    const nodeStart = /^-\s*id:\s*(.+)$/.exec(trimmed);
    if (nodeStart) {
      if (graph.nodes.length >= MAX_WORKFLOW_NODES) throw new RangeError("Workflow has too many nodes.");
      current = { id: stripQuotes(nodeStart[1]), uses: "", needs: [], with: {} };
      graph.nodes.push(current);
      currentWithFields = 0;
      inWith = false;
      continue;
    }

    if (!current) continue;
    const keyValue = /^([a-zA-Z0-9_-]+):\s*(.*)$/.exec(trimmed);
    if (!keyValue) continue;
    const [, key, value] = keyValue;
    if (key === "uses") {
      current.uses = stripQuotes(value);
      inWith = false;
    } else if (key === "needs") {
      const needs = parseNeeds(value);
      dependencyEdges += needs.length;
      if (dependencyEdges > MAX_WORKFLOW_EDGES) throw new RangeError("Workflow has too many dependency edges.");
      current.needs = needs;
      inWith = false;
    } else if (key === "with") {
      inWith = true;
    } else if (inWith) {
      currentWithFields += 1;
      if (currentWithFields > MAX_WORKFLOW_NODE_FIELDS) throw new RangeError("Workflow node has too many parameters.");
      parameterFields += 1;
      if (parameterFields > MAX_WORKFLOW_PARAMETERS) throw new RangeError("Workflow has too many parameters.");
      defineOwnValue(current.with, key, stripQuotes(value));
    }
  }

  return graph;
}

const yamlScalar = (value: string) => {
  if (value.length > MAX_WORKFLOW_YAML_CHARS) throw new RangeError("Workflow YAML field is too large.");
  const trimmed = value.trim();
  if (!trimmed) return '""';
  if (/^[a-zA-Z0-9_./:@${}\-]+$/.test(trimmed)) return trimmed;
  return JSON.stringify(trimmed);
};

const uniqueNodeId = (nodes: WorkflowStudioNode[], preferred: string) => {
  const existing = new Set(nodes.map((node) => node.id));
  const base = safeName(preferred, "step");
  if (!existing.has(base)) return base;
  let suffix = 2;
  while (existing.has(`${base}-${suffix}`)) suffix += 1;
  return `${base}-${suffix}`;
};

const serializeNeeds = (needs: string[]): string => {
  const encoded: string[] = [];
  let length = 0;
  for (const neededId of needs) {
    const scalar = yamlScalar(neededId);
    length += scalar.length + (encoded.length ? 2 : 0);
    if (length > MAX_WORKFLOW_YAML_CHARS) throw new RangeError("Workflow YAML is too large.");
    encoded.push(scalar);
  }
  return encoded.join(", ");
};

export function serializeWorkflowGraphLite(graph: WorkflowGraphLite): string {
  assertWorkflowGraphLimits(graph);
  const lines: string[] = [];
  let outputLength = 0;
  const pushLine = (line: string) => {
    outputLength += line.length + 1;
    if (outputLength > MAX_WORKFLOW_YAML_CHARS) throw new RangeError("Workflow YAML is too large.");
    lines.push(line);
  };

  pushLine(`name: ${yamlScalar(graph.name || "未命名工作流")}`);
  pushLine(`description: ${yamlScalar(graph.description || "")}`);
  if (!graph.nodes.length) {
    pushLine("nodes: []");
    return `${lines.join("\n")}\n`;
  }

  pushLine("nodes:");
  for (const node of graph.nodes) {
    const withEntries = boundedWithEntries(node.with);
    pushLine(`  - id: ${yamlScalar(node.id)}`);
    pushLine(`    uses: ${yamlScalar(node.uses)}`);
    if (node.needs.length) pushLine(`    needs: [${serializeNeeds(node.needs)}]`);
    if (withEntries.length) {
      pushLine("    with:");
      for (const [key, value] of withEntries) pushLine(`      ${safeName(key)}: ${yamlScalar(value)}`);
    }
  }

  return `${lines.join("\n")}\n`;
}

export function updateWorkflowGraphNode(
  graph: WorkflowGraphLite,
  nodeId: string,
  patch: WorkflowGraphNodePatch,
): WorkflowGraphLite {
  assertWorkflowGraphLimits(graph);
  const previousNode = graph.nodes.find((node) => node.id === nodeId);
  if (!previousNode) return graph;

  const requestedId = (patch.id ?? previousNode.id).trim();
  const otherNodes = graph.nodes.filter((node) => node.id !== nodeId);
  const nextId = requestedId === previousNode.id ? previousNode.id : uniqueNodeId(otherNodes, requestedId);
  const replacement = {
    id: nextId,
    uses: patch.uses ?? previousNode.uses,
    needs: patch.needs ?? previousNode.needs,
    with: patch.with ?? previousNode.with,
  };
  assertWorkflowNodeLimits(replacement);

  const updated: WorkflowGraphLite = {
    ...graph,
    nodes: graph.nodes.map((node) => {
      if (node.id === nodeId) {
        return {
          id: replacement.id,
          uses: replacement.uses,
          needs: [...replacement.needs],
          with: cloneStringRecord(replacement.with),
        };
      }
      return {
        ...node,
        needs: node.needs.map((neededId) => (neededId === nodeId ? nextId : neededId)),
        with: cloneStringRecord(node.with),
      };
    }),
  };
  assertWorkflowGraphLimits(updated);
  return updated;
}

export function addWorkflowGraphNode(
  graph: WorkflowGraphLite,
  node: Partial<WorkflowStudioNode> = {},
): WorkflowGraphLite {
  assertWorkflowGraphLimits(graph);
  if (graph.nodes.length >= MAX_WORKFLOW_NODES) throw new RangeError("Workflow has too many nodes.");
  const nextId = uniqueNodeId(graph.nodes, node.id || `step-${graph.nodes.length + 1}`);
  const rawNeeds = node.needs || [];
  const rawWith = node.with || {};
  assertWorkflowNodeLimits({ needs: rawNeeds, with: rawWith });
  const nextNode: WorkflowStudioNode = {
    id: nextId,
    uses: node.uses || "",
    needs: [...rawNeeds],
    with: cloneStringRecord(rawWith),
  };
  const updated = {
    ...graph,
    nodes: [
      ...graph.nodes.map((current) => ({
        ...current,
        needs: [...current.needs],
        with: cloneStringRecord(current.with),
      })),
      nextNode,
    ],
  };
  assertWorkflowGraphLimits(updated);
  return updated;
}

export function deleteWorkflowGraphNode(graph: WorkflowGraphLite, nodeId: string): WorkflowGraphLite {
  assertWorkflowGraphLimits(graph);
  const updated = {
    ...graph,
    nodes: graph.nodes
      .filter((node) => node.id !== nodeId)
      .map((node) => ({
        ...node,
        needs: node.needs.filter((neededId) => neededId !== nodeId),
        with: cloneStringRecord(node.with),
      })),
  };
  assertWorkflowGraphLimits(updated);
  return updated;
}
