// Owns explicit Capability Plugin lifecycle operations outside settings autosave.
import {
  approveCapabilityPermissions,
  type CapabilityCatalogItem,
  type CapabilityCatalogSnapshot,
  type CapabilityInstalledVersion,
  type CapabilityPermissionGrant,
  type CapabilityPluginRecord,
  type CapabilityPluginSnapshot,
  disableCapability,
  enableCapability,
  getCapabilityCatalog,
  installCatalogCapability,
  installLocalCapability,
  listCapabilityGrants,
  listCapabilityPlugins,
  rollbackCapability,
  uninstallCapability,
  upgradeCapability,
} from "../../../services/loomApi";
import { pushAppToast, requestAppConfirmation } from "../../feedback/AppFeedback";
import { useCallback, useEffect, useRef, useState } from "react";

const MAX_LOCAL_CAPABILITY_BYTES = 8 * 1024 * 1024;
const EMPTY_PLUGINS: CapabilityPluginSnapshot = { plugins: [], diskBytesByPlugin: {} };
const EMPTY_CATALOG: CapabilityCatalogSnapshot = { configured: false, packages: [] };

export type CapabilityPermissionAction = "enable" | "upgrade" | "rollback";

export interface CapabilityPermissionReview {
  qualifiedId: string;
  name: string;
  digest: string;
  version: string;
  publisherId: string;
  permissions: string[];
  action: CapabilityPermissionAction;
}

const newestVersion = (plugin: CapabilityPluginRecord): CapabilityInstalledVersion | null =>
  plugin.versions.reduce<CapabilityInstalledVersion | null>((newest, candidate) => {
    if (!newest) return candidate;
    const installedOrder = Date.parse(candidate.installedAt) - Date.parse(newest.installedAt);
    if (Number.isFinite(installedOrder) && installedOrder !== 0) {
      return installedOrder > 0 ? candidate : newest;
    }
    return candidate.version.localeCompare(newest.version, undefined, { numeric: true }) > 0
      ? candidate
      : newest;
  }, null);

const versionForDigest = (plugin: CapabilityPluginRecord, digest?: string) =>
  digest ? plugin.versions.find((version) => version.digest === digest) ?? null : newestVersion(plugin);

const capabilityFileDataUrl = async (file: File): Promise<string> => {
  if (!file.name.toLocaleLowerCase().endsWith(".zip")) {
    throw new Error("本地能力扩展必须是 ZIP 包。");
  }
  if (file.size === 0 || file.size > MAX_LOCAL_CAPABILITY_BYTES) {
    throw new Error("本地能力扩展 ZIP 必须大于 0 字节且不超过 8 MiB。");
  }
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 32_768) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 32_768));
  }
  return `data:application/zip;base64,${btoa(binary)}`;
};

export function useCapabilityExtensionsController(baseUrl: string) {
  const [snapshot, setSnapshot] = useState(EMPTY_PLUGINS);
  const [catalog, setCatalog] = useState(EMPTY_CATALOG);
  const [grants, setGrants] = useState<CapabilityPermissionGrant[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [busyPluginId, setBusyPluginId] = useState<string | null>(null);
  const [permissionReview, setPermissionReview] = useState<CapabilityPermissionReview | null>(null);
  const generationRef = useRef(0);
  const operationRef = useRef(false);
  const operationIdRef = useRef(0);
  const mountedRef = useRef(true);
  const baseUrlRef = useRef(baseUrl);

  const refresh = useCallback(async () => {
    const generation = ++generationRef.current;
    setLoading(true);
    try {
      const [nextSnapshot, nextGrants, nextCatalog] = await Promise.all([
        listCapabilityPlugins(baseUrl),
        listCapabilityGrants(baseUrl),
        getCapabilityCatalog(baseUrl),
      ]);
      if (generation !== generationRef.current) return;
      setSnapshot(nextSnapshot);
      setGrants(nextGrants);
      setCatalog(nextCatalog);
      setLoadError(null);
    } catch (error) {
      if (generation === generationRef.current) {
        setLoadError(error instanceof Error ? error.message : "无法读取能力扩展状态。");
      }
    } finally {
      if (generation === generationRef.current) setLoading(false);
    }
  }, [baseUrl]);

  useEffect(() => {
    mountedRef.current = true;
    baseUrlRef.current = baseUrl;
    setPermissionReview(null);
    void refresh();
    return () => {
      mountedRef.current = false;
      generationRef.current += 1;
      operationIdRef.current += 1;
      operationRef.current = false;
    };
  }, [baseUrl, refresh]);

  const runMutation = useCallback(async <T,>(
    qualifiedId: string,
    operation: () => Promise<T>,
  ): Promise<T | null> => {
    if (operationRef.current) return null;
    operationRef.current = true;
    const operationId = ++operationIdRef.current;
    setBusyPluginId(qualifiedId);
    try {
      const result = await operation();
      if (!mountedRef.current || baseUrlRef.current !== baseUrl) return null;
      await refresh();
      return result;
    } catch (error) {
      if (mountedRef.current && baseUrlRef.current === baseUrl) {
        pushAppToast({
          level: "error",
          text: error instanceof Error ? error.message : "能力扩展操作失败。",
        });
      }
      return null;
    } finally {
      if (operationId === operationIdRef.current) {
        operationRef.current = false;
        if (mountedRef.current) setBusyPluginId(null);
      }
    }
  }, [baseUrl, refresh]);

  const queueReview = useCallback((
    plugin: CapabilityPluginRecord | undefined,
    version: CapabilityInstalledVersion,
    action: CapabilityPermissionAction,
  ) => {
    setPermissionReview({
      qualifiedId: plugin?.qualifiedId ?? "",
      name: plugin?.name || plugin?.qualifiedId || "能力扩展",
      digest: version.digest,
      version: version.version,
      publisherId: plugin?.publisherId || "",
      permissions: version.requestedPermissions,
      action,
    });
  }, []);

  const queueInstalledReview = useCallback(async (
    qualifiedId: string,
    digest: string,
    action: CapabilityPermissionAction,
  ): Promise<CapabilityPluginRecord | null> => {
    try {
      const nextSnapshot = await listCapabilityPlugins(baseUrl);
      if (!mountedRef.current || baseUrlRef.current !== baseUrl) return null;
      const plugin = nextSnapshot.plugins.find((candidate) => candidate.qualifiedId === qualifiedId);
      const version = plugin && versionForDigest(plugin, digest);
      if (!plugin || !version) {
        throw new Error("能力扩展已安装，但无法读取待启用版本。请刷新后重试。");
      }
      queueReview(plugin, version, action);
      return plugin;
    } catch (error) {
      if (mountedRef.current && baseUrlRef.current === baseUrl) {
        pushAppToast({
          level: "error",
          text: error instanceof Error ? error.message : "无法读取已安装能力扩展。",
        });
      }
      return null;
    }
  }, [baseUrl, queueReview]);

  const installFromCatalog = useCallback(async (item: CapabilityCatalogItem) => {
    const existing = snapshot.plugins.find((plugin) => plugin.qualifiedId === item.entry.qualifiedId);
    const report = await runMutation(item.entry.qualifiedId, () =>
      installCatalogCapability(baseUrl, item.entry.qualifiedId));
    if (!report) return;
    const plugin = await queueInstalledReview(
      report.qualifiedId,
      report.digest,
      existing?.activeDigest ? "upgrade" : "enable",
    );
    if (!plugin) return;
    pushAppToast({ level: "info", text: `${item.entry.name} 已验证并安装，启用前请确认权限。` });
  }, [baseUrl, queueInstalledReview, runMutation, snapshot.plugins]);

  const installFromFile = useCallback(async (file: File) => {
    let dataUrl: string;
    try {
      dataUrl = await capabilityFileDataUrl(file);
    } catch (error) {
      pushAppToast({ level: "error", text: error instanceof Error ? error.message : "无法读取 ZIP 包。" });
      return;
    }
    const report = await runMutation(file.name, () => installLocalCapability(baseUrl, dataUrl));
    if (!report) return;
    const plugin = await queueInstalledReview(
      report.qualifiedId,
      report.digest,
      snapshot.plugins.some((candidate) => (
        candidate.qualifiedId === report.qualifiedId && Boolean(candidate.activeDigest)
      )) ? "upgrade" : "enable",
    );
    if (!plugin) return;
    pushAppToast({ level: "info", text: `${plugin.name} 已安装，启用前请确认权限。` });
  }, [baseUrl, queueInstalledReview, runMutation, snapshot.plugins]);

  const requestEnable = useCallback((plugin: CapabilityPluginRecord) => {
    const version = versionForDigest(plugin);
    if (version) queueReview(plugin, version, plugin.activeDigest ? "upgrade" : "enable");
  }, [queueReview]);

  const requestRollback = useCallback((plugin: CapabilityPluginRecord) => {
    const version = versionForDigest(plugin, plugin.previousDigest);
    if (version) queueReview(plugin, version, "rollback");
  }, [queueReview]);

  const confirmPermissionReview = useCallback(async () => {
    const review = permissionReview;
    if (!review) return;
    const result = await runMutation(review.qualifiedId, async () => {
      if (review.permissions.length > 0) {
        await approveCapabilityPermissions(
          baseUrl,
          review.qualifiedId,
          review.digest,
          review.permissions,
        );
      }
      if (review.action === "upgrade") {
        return await upgradeCapability(baseUrl, review.qualifiedId, review.digest);
      }
      if (review.action === "rollback") {
        return await rollbackCapability(baseUrl, review.qualifiedId);
      }
      return await enableCapability(baseUrl, review.qualifiedId, review.digest);
    });
    if (!result) return;
    setPermissionReview(null);
    const actionCompleted = review.action === "upgrade"
      ? "已更新"
      : review.action === "rollback" ? "已回滚" : "已启用";
    pushAppToast({ level: "info", text: `${review.name} ${actionCompleted}。` });
  }, [baseUrl, permissionReview, runMutation]);

  const disable = useCallback(async (plugin: CapabilityPluginRecord) => {
    const result = await runMutation(plugin.qualifiedId, () => disableCapability(baseUrl, plugin.qualifiedId));
    if (result) pushAppToast({ level: "info", text: `${plugin.name} 已停用。` });
  }, [baseUrl, runMutation]);

  const uninstall = useCallback(async (plugin: CapabilityPluginRecord) => {
    const accepted = await requestAppConfirmation({
      title: "卸载能力扩展",
      message: `将删除 ${plugin.name} 的所有已安装版本、权限授权和配置。`,
      confirmLabel: "卸载",
    });
    if (!accepted) return;
    const result = await runMutation(plugin.qualifiedId, () => uninstallCapability(baseUrl, plugin.qualifiedId));
    if (result) pushAppToast({ level: "info", text: `${plugin.name} 已卸载。` });
  }, [baseUrl, runMutation]);

  return {
    busyPluginId,
    catalog,
    confirmPermissionReview,
    disable,
    grants,
    installFromCatalog,
    installFromFile,
    loadError,
    loading,
    permissionReview,
    refresh,
    requestEnable,
    requestRollback,
    setPermissionReview,
    snapshot,
    uninstall,
  };
}
