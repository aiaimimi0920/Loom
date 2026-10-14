import { getJson, postJson, putJson } from "./transport.ts";

export type OtterApprovalPolicy = "manual" | "password" | "authenticated";
export interface LoomConnectionIdentity {
  protocolVersion: "loom.connection.v1";
  loomId: string;
  name: string;
  discoverable: boolean;
  endpoint: string | null;
  approval: OtterApprovalPolicy;
  passwordRequired: boolean;
}
export interface OtterConnectionRequest {
  id: string;
  deviceId: string;
  name: string;
  phase: "pending" | "approved" | "rejected" | "cancelled";
  expiresAtMs: number;
}
export interface OtterConnectionsView {
  loom: LoomConnectionIdentity;
  requests: OtterConnectionRequest[];
}
export interface OtterConnectionPolicyDraft {
  name: string;
  discoverable: boolean;
  endpoint: string | null;
  approval: OtterApprovalPolicy;
  password?: string;
}
export const loadOtterConnections = (baseUrl: string) => getJson<OtterConnectionsView>(baseUrl, "/v1/otter/connections");
export const saveOtterConnectionPolicy = (baseUrl: string, draft: OtterConnectionPolicyDraft) =>
  putJson<LoomConnectionIdentity>(baseUrl, "/v1/otter/connections/policy", draft);
export const decideOtterConnection = (baseUrl: string, requestId: string, approve: boolean) =>
  postJson<OtterConnectionRequest>(baseUrl, "/v1/otter/connections/decision", { requestId, approve });
