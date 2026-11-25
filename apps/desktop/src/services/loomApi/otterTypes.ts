// Otter bridge, managed-device, and workflow-instantiation contracts.
import type { OtterLocaleState } from "./otterLocaleTypes.ts";

export interface LoomOtterBridgeStatus {
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
  online?: boolean;
  isLocal?: boolean;
  enabled?: boolean;
  locale?: OtterLocaleState | null;
}

export interface LoomDevicesResponse {
  devices: LoomManagedDevice[];
  pending: LoomManagedDevice[];
  connectedClients: number;
}

export interface OtterWorkflowInstantiateResponse {
  protocolVersion?: string;
  status?: string;
  method?: string;
  broadcasted?: boolean;
  subscribedClients?: number;
  params?: unknown;
}
