import { deleteJson, getJson, postJson, putJson } from "./transport.ts";

export interface ProjectionPeerIdentity { peerId: string; publicKey: string }
export interface ProjectionPeer extends ProjectionPeerIdentity { name: string; origin: string; enabled: boolean }
export interface ProjectionPeerState { revision: number; identity: ProjectionPeerIdentity; peers: ProjectionPeer[] }
const path = "/v1/projection-peers";
const invalid = () => new Error("Loom 对等连接响应无效，请刷新后重试");
function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw invalid();
  return value as Record<string, unknown>;
}
function text(value: unknown, max: number): string {
  if (typeof value !== "string" || !value || new TextEncoder().encode(value).length > max) throw invalid();
  return value;
}
function identity(value: unknown): ProjectionPeerIdentity {
  const item = record(value);
  return { peerId: text(item.peerId, 69), publicKey: text(item.publicKey, 44) };
}
export function parseProjectionPeerState(value: unknown): ProjectionPeerState {
  const item = record(value);
  if (!Number.isSafeInteger(item.revision) || (item.revision as number) < 0
    || !Array.isArray(item.peers) || item.peers.length > 16) throw invalid();
  const peers = item.peers.map((value): ProjectionPeer => {
    const peer = record(value);
    if (typeof peer.enabled !== "boolean") throw invalid();
    return { ...identity(peer), name: text(peer.name, 128), origin: text(peer.origin, 256), enabled: peer.enabled };
  });
  if (new Set(peers.map((peer) => peer.peerId)).size !== peers.length) throw invalid();
  return { revision: item.revision as number, identity: identity(item.identity), peers };
}
export async function listProjectionPeers(baseUrl: string): Promise<ProjectionPeerState> {
  return parseProjectionPeerState(await getJson<unknown>(baseUrl, path));
}
export async function saveProjectionPeer(baseUrl: string, expectedRevision: number, peer: ProjectionPeer): Promise<ProjectionPeerState> {
  return parseProjectionPeerState(await putJson<unknown>(baseUrl, path, { expectedRevision, peer }));
}
export async function removeProjectionPeer(baseUrl: string, expectedRevision: number, peerId: string): Promise<ProjectionPeerState> {
  return parseProjectionPeerState(await deleteJson<unknown>(baseUrl, path, { expectedRevision, peerId }));
}
export async function probeProjectionPeer(baseUrl: string, expectedRevision: number, peerId: string): Promise<void> {
  const result = record(await postJson<unknown>(baseUrl, path + "/probe", { peerId }));
  if (result.verified !== true || result.peerId !== peerId || result.revision !== expectedRevision) throw invalid();
}
