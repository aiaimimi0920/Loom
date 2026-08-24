// Hook bridge, managed-device, workflow-instantiation, and session contracts.

export interface LoomHookBridgeStatus {
  running?: boolean;
  port?: number;
  ipcPort?: number;
  connectedClients?: number;
  subscribedClients?: number;
  protocol?: string;
  sessionMethod?: string;
  methods?: string[];
}

export type LoomDeviceKind = "computer" | "tablet" | "phone" | "other";
export type LoomDeviceApproval = "approved" | "pending";

export interface LoomManagedDevice {
  id: string;
  name: string;
  kind: LoomDeviceKind;
  address: string;
  approval: LoomDeviceApproval;
  createdAt: number;
  lastSeenAt?: number | null;
  isLocal?: boolean;
  enabled?: boolean;
}

export interface LoomDevicesResponse {
  devices: LoomManagedDevice[];
  pending: LoomManagedDevice[];
  connectedClients: number;
}

export interface HookWorkflowInstantiateResponse {
  protocolVersion?: string;
  status?: string;
  method?: string;
  broadcasted?: boolean;
  subscribedClients?: number;
  params?: unknown;
}


export interface HookSessionSnapshot {
  running?: boolean;
  port?: number;
  connectedClients?: number;
  subscribedClients?: number;
  protocol?: string;
  sessionPath?: string;
  available?: boolean;
  error?: string | null;
  session?: {
    stickers?: unknown[];
    links?: unknown[];
    [key: string]: unknown;
  };
}
