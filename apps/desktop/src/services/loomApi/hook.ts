// Hook bridge, device, session, and canvas workflow daemon clients.
import { DEFAULT_LOOM_DAEMON_URL } from "./defaults.ts";
import type { LoomWorkflowMetadata } from "./coreTypes.ts";
import type {
  HookSessionSnapshot,
  HookWorkflowInstantiateResponse,
  LoomDeviceKind,
  LoomDevicesResponse,
  LoomHookBridgeStatus,
} from "./hookTypes.ts";
import { deleteJson, getJson, postJson, putJson } from "./transport.ts";

export async function startHookBridge(
  baseUrl = DEFAULT_LOOM_DAEMON_URL,
  port?: number,
): Promise<LoomHookBridgeStatus> {
  const body = typeof port === "number" ? { port } : {};
  return await postJson<LoomHookBridgeStatus>(baseUrl, "/v1/hook-bridge/start", body);
}

export async function stopHookBridge(baseUrl = DEFAULT_LOOM_DAEMON_URL): Promise<LoomHookBridgeStatus> {
  return await postJson<LoomHookBridgeStatus>(baseUrl, "/v1/hook-bridge/stop", {});
}

export async function listManagedDevices(
  baseUrl = DEFAULT_LOOM_DAEMON_URL,
): Promise<LoomDevicesResponse> {
  return await getJson<LoomDevicesResponse>(baseUrl, "/v1/devices");
}

export async function addManagedDevice(
  baseUrl: string,
  input: { name: string; kind: LoomDeviceKind; address: string },
): Promise<LoomDevicesResponse> {
  return await postJson<LoomDevicesResponse>(baseUrl, "/v1/devices", input);
}

export async function approveManagedDevice(
  baseUrl: string,
  deviceId: string,
): Promise<LoomDevicesResponse> {
  return await postJson<LoomDevicesResponse>(
    baseUrl,
    `/v1/devices/${encodeURIComponent(deviceId)}/approve`,
    {},
  );
}

export async function updateManagedDevice(
  baseUrl: string,
  deviceId: string,
  input: { name: string; kind: LoomDeviceKind; address: string; enabled: boolean },
): Promise<LoomDevicesResponse> {
  return await putJson<LoomDevicesResponse>(
    baseUrl,
    `/v1/devices/${encodeURIComponent(deviceId)}`,
    input,
  );
}

export async function removeManagedDevice(
  baseUrl: string,
  deviceId: string,
): Promise<LoomDevicesResponse> {
  return await deleteJson<LoomDevicesResponse>(
    baseUrl,
    `/v1/devices/${encodeURIComponent(deviceId)}`,
  );
}

export async function readHookSession(baseUrl = DEFAULT_LOOM_DAEMON_URL): Promise<HookSessionSnapshot> {
  return await getJson<HookSessionSnapshot>(baseUrl, "/v1/hook-bridge/session");
}

export async function instantiateHookWorkflow(
  baseUrl: string,
  request: {
    nodes: unknown[];
    edges: unknown[];
    mode?: string;
    workflowId?: string | null;
  },
): Promise<HookWorkflowInstantiateResponse> {
  return await postJson<HookWorkflowInstantiateResponse>(
    baseUrl,
    "/v1/hook-bridge/workflows/instantiate",
    request,
  );
}

export async function updateHookWorkflowNode(
  baseUrl: string,
  request: {
    workflowId: string;
    nodeId: string;
    param: string;
    value: unknown;
  },
): Promise<Record<string, unknown>> {
  return await postJson<Record<string, unknown>>(
    baseUrl,
    "/v1/hook-bridge/workflows/nodes/update",
    request,
  );
}

export async function saveWorkflowBundle(
  baseUrl: string,
  workflow: Pick<LoomWorkflowMetadata, "id">,
  data: string,
): Promise<void> {
  await putJson(baseUrl, `/v1/workflows/${encodeURIComponent(workflow.id)}`, { data });
}

export async function saveHookCanvasWorkflow(
  baseUrl: string,
  request: {
    workflowId: string;
    selectedNodeId: string;
    workflowName?: string;
  },
): Promise<void> {
  await putJson(
    baseUrl,
    `/v1/hook-bridge/canvas/workflows/${encodeURIComponent(request.workflowId)}`,
    {
      selectedNodeId: request.selectedNodeId,
      workflowName: request.workflowName ?? request.workflowId,
    },
  );
}
