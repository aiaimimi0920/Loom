// Canvas workflow, durable workflow bundle, and tool-definition clients.
import type { LoomToolDefinition, LoomWorkflowBundle, LoomWorkflowBundleResponse } from "./coreTypes.ts";
import { deleteJson, getJson, putJson } from "./transport.ts";

export async function deleteCanvasWorkflow(baseUrl: string, workflowId: string): Promise<void> {
  await deleteJson(baseUrl, `/v1/hook-bridge/canvas/workflows/${encodeURIComponent(workflowId)}`);
}

export async function renameCanvasWorkflow(
  baseUrl: string,
  workflowId: string,
  name: string,
): Promise<void> {
  await putJson(
    baseUrl,
    `/v1/hook-bridge/canvas/workflows/${encodeURIComponent(workflowId)}/rename`,
    { name },
  );
}

export async function getWorkflowBundle(baseUrl: string, workflowId: string): Promise<LoomWorkflowBundle> {
  const response = await getJson<LoomWorkflowBundleResponse>(
    baseUrl,
    `/v1/workflows/${encodeURIComponent(workflowId)}`,
  );
  if (!response.workflow) {
    throw new Error(`Loom 本地服务没有返回工作流 ${workflowId}。`);
  }
  return response.workflow;
}

export async function deleteWorkflowBundle(baseUrl: string, workflowId: string): Promise<void> {
  await deleteJson(baseUrl, `/v1/workflows/${encodeURIComponent(workflowId)}`);
}

export async function saveToolDefinition(
  baseUrl: string,
  tool: LoomToolDefinition,
): Promise<LoomToolDefinition> {
  const response = await putJson<{ tool?: LoomToolDefinition }>(
    baseUrl,
    `/v1/tools/${encodeURIComponent(tool.id)}`,
    tool,
  );
  return response.tool ?? tool;
}

export async function deleteToolDefinition(baseUrl: string, toolId: string): Promise<void> {
  await deleteJson(baseUrl, `/v1/tools/${encodeURIComponent(toolId)}`);
}
