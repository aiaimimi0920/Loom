import { getJson, putJson } from "./transport.ts";
export interface ProjectionDeviceRef { deviceId: string; peerId?: string }
export interface ProjectionDeviceChoice extends ProjectionDeviceRef { name: string }
export interface ProjectionGroup { groupId: string; name: string; members: ProjectionDeviceRef[] }
export type ProjectionReceiveDecision = "auto" | "confirm" | "reject";
export interface ProjectionReceiveRule {
  deviceId: string; policy: ProjectionReceiveDecision;
  whitelist: { devices: ProjectionDeviceRef[]; groups: string[]; users: string[] };
  blacklist: ProjectionDeviceRef[];
}
export interface ProjectionSettings { revision: number; groups: ProjectionGroup[]; rules: ProjectionReceiveRule[] }
export interface ProjectionSettingsView { settings: ProjectionSettings; devices: ProjectionDeviceChoice[]; directoryStatus: string }
export const projectionDeviceKey = (device: ProjectionDeviceRef) => JSON.stringify([device.peerId ?? null, device.deviceId]);
export function projectionGroupNameError(name: string): string {
  if (!name.trim()) return "设备组名称不能为空";
  if (new TextEncoder().encode(name).length > 128) return "设备组名称不能超过 128 个 UTF-8 字节";
  if ([...name].some((character) => { const code = character.codePointAt(0)!; return code < 32 || (code >= 127 && code <= 159); })) return "设备组名称不能包含控制字符";
  return "";
}
const invalid = () => new Error("投射规则响应无效，请刷新 Loom 配置");
const path = "/v1/projection-settings";
function record(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw invalid();
  return value as Record<string, unknown>;
}
function text(value: unknown, maximum = 160): string {
  if (typeof value !== "string" || !value || new TextEncoder().encode(value).length > maximum) throw invalid();
  return value;
}
function array<T>(value: unknown, parse: (item: unknown) => T, maximum = 64): T[] {
  if (!Array.isArray(value) || value.length > maximum) throw invalid();
  return value.map(parse);
}
function device(value: unknown): ProjectionDeviceRef {
  const item = record(value);
  return { deviceId: text(item.deviceId), ...(item.peerId === undefined ? {} : { peerId: text(item.peerId, 69) }) };
}
export function parseProjectionSettings(value: unknown): ProjectionSettings {
  const item = record(value);
  if (item.storageVersion !== 1 || !Number.isSafeInteger(item.revision) || Number(item.revision) < 0) throw invalid();
  const groups = array(item.groups, (value): ProjectionGroup => {
    const group = record(value);
    return { groupId: text(group.groupId), name: text(group.name, 128), members: array(group.members, device) };
  }, 32);
  const rules = array(item.rules, (value): ProjectionReceiveRule => {
    const rule = record(value); const whitelist = record(rule.whitelist);
    if (rule.policy !== "auto" && rule.policy !== "confirm" && rule.policy !== "reject") throw invalid();
    return { deviceId: text(rule.deviceId), policy: rule.policy, blacklist: array(rule.blacklist, device),
      whitelist: { devices: array(whitelist.devices, device), groups: array(whitelist.groups, (id) => text(id)), users: array(whitelist.users, (id) => text(id)) } };
  });
  if (new Set(groups.map((group) => group.groupId)).size !== groups.length || new Set(rules.map((rule) => rule.deviceId)).size !== rules.length) throw invalid();
  return { revision: Number(item.revision), groups, rules };
}
export async function loadProjectionSettings(baseUrl: string): Promise<ProjectionSettingsView> {
  const value = record(await getJson<unknown>(baseUrl, path));
  const devices = array(value.targets, (value): ProjectionDeviceChoice => {
    const target = record(value);
    if (target.route === "shared_loom") return { deviceId: text(target.deviceId), name: text(target.name, 1024) };
    if (target.route !== "offline_peer") throw invalid();
    return { deviceId: text(target.remoteDeviceId), peerId: text(target.peerId, 69), name: text(target.peerName, 128) + " / " + text(target.name, 1024) };
  });
  const status = record(value.peerDirectory).status;
  if (!["complete", "partial", "busy", "unavailable"].includes(String(status))) throw invalid();
  return { settings: parseProjectionSettings(value), devices, directoryStatus: String(status) };
}
export async function saveProjectionSettings(baseUrl: string, settings: ProjectionSettings): Promise<ProjectionSettings> {
  return parseProjectionSettings(await putJson<unknown>(baseUrl, path, { expectedRevision: settings.revision, groups: settings.groups, rules: settings.rules }));
}
