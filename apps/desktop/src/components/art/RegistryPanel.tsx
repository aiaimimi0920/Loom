// Owns installed Art registry actions and authoring orchestration.
import { Badge, Button, IconButton, Notice, Panel } from "@neuro/ui";
import {
  artFrameworkReference,
  artPackageIdentity,
  filterToolsByFrameworks,
  frameworkFilterLabel,
  frameworkIdentity,
} from "../../services/artHubUi";
import { artMcpDependencyIds, resolveArtMcpDependencies } from "../../services/artMcpDependencies";
import {
  autoUpdateArts,
  deleteToolDefinition,
  getArtManagement,
  type LoomArtManagement,
  type LoomArtManagementSettingsInput,
  type LoomFramework,
  LoomMcpServer,
  LoomToolDefinition,
  LoomWorkflowMetadata,
  saveArtManagementSettings,
  saveToolDefinition,
  uninstallArtPackage,
  updateArtToVersion,
  updateMcpServerCredentials,
} from "../../services/loomApi";
import { artFrameworkIconKind, artFrameworkIconLabel, ArtIcon, StudioMessage } from "../app/appShell";
import { pushAppToast, requestAppConfirmation, requestAppConfirmationWithOption } from "../feedback/AppFeedback";
import { McpCredentialDialog } from "../mcp/McpHub";
import { AddArtWizard } from "./AddArtWizard";
import { ArtCreationDialog } from "./ArtCreationDialog";
import { ArtEditDialog } from "./ArtEditDialog";
import type { ArtCreationRequest } from "./artWizardModel";
import { useArtCreationSubmission } from "./useArtCreationSubmission";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

export function RegistryPanel({
  tools,
  mcpServers,
  workflows,
  frameworks,
  selectedFrameworkIds,
  createDialogOpen,
  createRequest,
  onCloseCreateDialog,
  reloadFrameworks,
  baseUrl,
  refresh,
}: {
  tools: LoomToolDefinition[];
  mcpServers: LoomMcpServer[];
  workflows: LoomWorkflowMetadata[];
  frameworks: LoomFramework[];
  selectedFrameworkIds: ReadonlySet<string> | null;
  createDialogOpen: boolean;
  createRequest: ArtCreationRequest | null;
  onCloseCreateDialog: () => void;
  reloadFrameworks: () => Promise<void>;
  baseUrl: string;
  refresh: () => Promise<void>;
}) {
  const [busyToolId, setBusyToolId] = useState<string | null>(null);
  const [busyToolAction, setBusyToolAction] = useState<"delete" | "toggle" | "edit" | null>(null);
  const [registryMessage, setRegistryMessage] = useState<StudioMessage | null>(null);
  const [editingTool, setEditingTool] = useState<LoomToolDefinition | null>(null);
  const [artManagement, setArtManagement] = useState<LoomArtManagement | null>(null);
  const [artManagementLoading, setArtManagementLoading] = useState(false);
  const [editBusyAction, setEditBusyAction] = useState<"save" | "update" | null>(null);
  const [editError, setEditError] = useState<string | null>(null);
  const [credentialServer, setCredentialServer] = useState<LoomMcpServer | null>(null);
  const [credentialBusy, setCredentialBusy] = useState(false);
  const autoUpdateAttemptedUrl = useRef<string | null>(null);
  const editLoadVersion = useRef(0);
  const { busyWizard, createArtToolFromWizard } = useArtCreationSubmission({
    frameworks,
    workflows,
    baseUrl,
    refresh,
    reloadFrameworks,
    setRegistryMessage,
  });
  const visibleTools = useMemo(
    () => filterToolsByFrameworks(tools, frameworks, selectedFrameworkIds),
    [frameworks, selectedFrameworkIds, tools],
  );

  useEffect(() => {
    if (createDialogOpen) setRegistryMessage(null);
  }, [createDialogOpen]);

  useEffect(
    () => () => {
      editLoadVersion.current += 1;
    },
    [],
  );

  useEffect(() => {
    if (autoUpdateAttemptedUrl.current === baseUrl) return;
    autoUpdateAttemptedUrl.current = baseUrl;
    let active = true;
    void autoUpdateArts(baseUrl)
      .then((result) => {
        if (active && Array.isArray(result.updated) && result.updated.length) void refresh();
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [baseUrl, refresh]);

  const removeTool = async (tool: LoomToolDefinition) => {
    setRegistryMessage(null);
    setBusyToolId(tool.id);
    setBusyToolAction("delete");
    try {
      const packageIdentity = artPackageIdentity(tool);
      if (packageIdentity) {
        const mcpDependencies = artMcpDependencyIds(tool);
        const confirmation =
          mcpDependencies.length > 0
            ? await requestAppConfirmationWithOption({
                title: "卸载 Art",
                message: `将卸载 ${tool.name || tool.id}。它声明了 ${mcpDependencies.length} 个独立 MCP 服务依赖。`,
                optionLabel: "同时卸载未被其他 Art 使用的 MCP 服务",
                optionDefault: false,
                confirmLabel: "卸载",
                tone: "danger",
              })
            : {
                accepted: await requestAppConfirmation({
                  title: "卸载 Art",
                  message: `将卸载 ${tool.name || tool.id} 及其运行数据。`,
                  confirmLabel: "卸载",
                  tone: "danger",
                }),
                optionSelected: false,
              };
        if (!confirmation.accepted) return;
        const result = await uninstallArtPackage(baseUrl, packageIdentity, {
          removeUnusedMcpServers: confirmation.optionSelected,
        });
        const removedCount = result.removedMcpServers?.length || 0;
        const retainedCount = result.retainedMcpServers?.length || 0;
        setRegistryMessage({
          kind: "info",
          text:
            removedCount || retainedCount
              ? `已卸载 ${tool.name || tool.id}；移除 ${removedCount} 个未使用 MCP，保留 ${retainedCount} 个仍在使用的 MCP。`
              : `已卸载 ${tool.name || tool.id}。`,
        });
      } else {
        if (
          !(await requestAppConfirmation({
            title: "删除工具",
            message: `将删除 ${tool.name || tool.id}。此操作不可撤销。`,
            confirmLabel: "删除",
            tone: "danger",
          }))
        )
          return;
        await deleteToolDefinition(baseUrl, tool.id);
      }
      await refresh();
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法删除工具。",
      });
    } finally {
      setBusyToolId(null);
      setBusyToolAction(null);
    }
  };

  const toggleTool = async (tool: LoomToolDefinition) => {
    const nextEnabled = tool.enabled === false;
    setBusyToolId(tool.id);
    setBusyToolAction("toggle");
    try {
      await saveToolDefinition(baseUrl, { ...tool, enabled: nextEnabled });
      setRegistryMessage({
        kind: "info",
        text: `已${nextEnabled ? "启用" : "禁用"} ${tool.name || tool.id}。`,
      });
      await refresh();
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法切换 Art 状态。",
      });
    } finally {
      setBusyToolId(null);
      setBusyToolAction(null);
    }
  };

  const closeToolEditor = useCallback(() => {
    editLoadVersion.current += 1;
    setEditingTool(null);
    setArtManagement(null);
    setArtManagementLoading(false);
    setEditBusyAction(null);
    setEditError(null);
  }, []);

  const openToolEditor = async (tool: LoomToolDefinition) => {
    const version = ++editLoadVersion.current;
    setEditingTool(tool);
    setArtManagement(null);
    setArtManagementLoading(true);
    setEditError(null);
    try {
      await autoUpdateArts(baseUrl);
      if (version !== editLoadVersion.current) return;
      const management = await getArtManagement(baseUrl, tool.id);
      if (version === editLoadVersion.current) setArtManagement(management);
    } catch (error) {
      if (version === editLoadVersion.current)
        setEditError(error instanceof Error ? error.message : "无法读取 Art 设置。");
    } finally {
      if (version === editLoadVersion.current) setArtManagementLoading(false);
    }
  };

  const saveToolEdits = async (input: LoomArtManagementSettingsInput) => {
    if (!artManagement) return;
    setEditBusyAction("save");
    setEditError(null);
    try {
      await saveArtManagementSettings(baseUrl, artManagement.artId, input);
      await refresh();
      closeToolEditor();
    } catch (error) {
      const detail = error instanceof Error ? error.message : "无法更新 Art。";
      setEditError(detail);
      setRegistryMessage({
        kind: "error",
        text: detail,
      });
    } finally {
      setEditBusyAction(null);
    }
  };

  const updateToolVersion = async (version: string, input: LoomArtManagementSettingsInput) => {
    if (!artManagement) return;
    setEditBusyAction("update");
    setEditError(null);
    try {
      await saveArtManagementSettings(baseUrl, artManagement.artId, input);
      const updated = await updateArtToVersion(baseUrl, artManagement.artId, version);
      setArtManagement(updated);
      await refresh();
    } catch (error) {
      const detail = error instanceof Error ? error.message : "无法更新 Art 版本。";
      setEditError(detail);
      setRegistryMessage({ kind: "error", text: detail });
    } finally {
      setEditBusyAction(null);
    }
  };

  const saveMcpDependencyCredentials = async (values: Record<string, string>, clear: string[]) => {
    if (!credentialServer || credentialBusy) return;
    setCredentialBusy(true);
    setRegistryMessage(null);
    try {
      await updateMcpServerCredentials(baseUrl, credentialServer.id, values, clear);
      const message = `${credentialServer.name || credentialServer.id} 的 MCP 凭据已更新。`;
      setCredentialServer(null);
      setRegistryMessage({ kind: "info", text: message });
      pushAppToast({ level: "info", text: message });
      await refresh();
    } catch (error) {
      const detail = error instanceof Error ? error.message : "无法保存 MCP 凭据。";
      setRegistryMessage({ kind: "error", text: detail });
      pushAppToast({ level: "error", text: detail });
    } finally {
      setCredentialBusy(false);
    }
  };

  return (
    <section className="content-grid">
      {registryMessage ? (
        <Notice
          tone={registryMessage.kind === "error" ? "danger" : "success"}
          role={registryMessage.kind === "error" ? "alert" : "status"}
        >
          {registryMessage.text}
        </Notice>
      ) : null}
      <div className="card-grid art-registry-grid">
        {visibleTools.map((tool) => {
          const enabled = tool.enabled !== false;
          const frameworkReference = artFrameworkReference(tool) || tool.execution?.type || null;
          const framework = frameworks.find(
            (candidate) => frameworkIdentity(candidate) === frameworkReference || candidate.id === frameworkReference,
          );
          const frameworkLabel = framework
            ? frameworkFilterLabel(framework)
            : artFrameworkIconLabel(frameworkReference);
          const mcpDependencies = resolveArtMcpDependencies(tool, mcpServers);
          const blockedMcpDependency = mcpDependencies.find((dependency) => dependency.status !== "ready");
          const displayedMcpDependency = blockedMcpDependency ?? mcpDependencies[0];
          const mcpStatus = displayedMcpDependency?.status;
          const mcpStatusText =
            mcpStatus === "credentials_required"
              ? "需要配置 MCP 凭据"
              : mcpStatus === "disabled"
                ? "MCP 依赖已禁用"
                : mcpStatus === "missing"
                  ? "MCP 依赖未安装"
                  : mcpStatus === "ready"
                    ? "MCP 依赖就绪"
                    : null;
          const toolBusy = busyToolId === tool.id;
          return (
            <Panel className="art-registry-entry" data-enabled={enabled} key={tool.id}>
              <div className="art-registry-card__head">
                <h3 title={tool.name || tool.id}>{tool.name || tool.id}</h3>
                <div className="art-registry-entry__state">
                  <Badge tone={enabled ? "success" : "neutral"}>{enabled ? "已启用" : "已禁用"}</Badge>
                  <span
                    className="art-registry-card__framework-icon"
                    role="img"
                    aria-label={`${frameworkLabel} Art`}
                    title={frameworkLabel}
                  >
                    <ArtIcon kind={artFrameworkIconKind(frameworkReference)} />
                  </span>
                </div>
              </div>
              <div className="art-registry-card__body">
                {tool.description ? (
                  <p className="art-registry-card__description" title={tool.description}>
                    {tool.description}
                  </p>
                ) : null}
                {mcpStatusText && displayedMcpDependency ? (
                  mcpStatus === "credentials_required" && displayedMcpDependency.server ? (
                    <Button
                      compact
                      wrap
                      type="button"
                      title={displayedMcpDependency.dependencyId}
                      aria-label={`${tool.name || tool.id}: 配置 ${displayedMcpDependency.server.name || displayedMcpDependency.server.id} MCP 凭据`}
                      onClick={() => setCredentialServer(displayedMcpDependency.server)}
                      disabled={toolBusy || credentialBusy}
                    >
                      {mcpStatusText}
                    </Button>
                  ) : (
                    <Badge
                      tone={mcpStatus === "ready" ? "success" : mcpStatus === "missing" ? "danger" : "warning"}
                      title={displayedMcpDependency.dependencyId}
                    >
                      {mcpStatusText}
                    </Badge>
                  )
                ) : null}
              </div>
              <div className="art-registry-card__actions">
                <IconButton
                  label={`编辑 ${tool.name || tool.id}`}
                  title="编辑"
                  onClick={() => {
                    void openToolEditor(tool);
                  }}
                  disabled={toolBusy}
                >
                  <ArtIcon kind="edit" />
                </IconButton>
                <IconButton
                  variant={enabled ? "positive" : "secondary"}
                  label={`${enabled ? "禁用" : "启用"} ${tool.name || tool.id}`}
                  aria-pressed={enabled}
                  loading={toolBusy && busyToolAction === "toggle"}
                  title={enabled ? "禁用" : "启用"}
                  onClick={() => void toggleTool(tool)}
                  disabled={toolBusy}
                >
                  <ArtIcon kind="power" />
                </IconButton>
                <IconButton
                  variant="danger"
                  label={`删除 ${tool.name || tool.id}`}
                  loading={toolBusy && busyToolAction === "delete"}
                  title="删除"
                  onClick={() => void removeTool(tool)}
                  disabled={toolBusy}
                >
                  <ArtIcon kind="trash" />
                </IconButton>
              </div>
            </Panel>
          );
        })}
      </div>
      <ArtEditDialog
        tool={editingTool}
        management={artManagement}
        loading={artManagementLoading}
        busyAction={editBusyAction}
        error={editError}
        onClose={closeToolEditor}
        onSave={saveToolEdits}
        onUpdate={updateToolVersion}
      />
      <McpCredentialDialog
        server={credentialServer}
        busy={credentialBusy}
        onClose={() => {
          if (!credentialBusy) setCredentialServer(null);
        }}
        onSave={saveMcpDependencyCredentials}
      />
      <ArtCreationDialog
        open={createDialogOpen}
        busy={busyWizard}
        message={registryMessage}
        onClose={onCloseCreateDialog}
      >
        <AddArtWizard
          key={createRequest?.requestId ?? "manual"}
          baseUrl={baseUrl}
          frameworks={frameworks}
          mcpServers={mcpServers}
          workflows={workflows}
          tools={tools}
          initialRequest={createRequest}
          busy={busyWizard}
          onCreate={async (draft) => {
            const created = await createArtToolFromWizard(draft);
            if (created) onCloseCreateDialog();
            return created;
          }}
        />
      </ArtCreationDialog>
    </section>
  );
}
