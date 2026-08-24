// Owns installed Art registry actions and authoring orchestration.
import { buildAuthoredArtPackage } from "../../services/artAuthoring";
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
  createAuthoredArtPackage,
  deleteToolDefinition,
  getArtManagement,
  type LoomArtManagement,
  type LoomArtManagementSettingsInput,
  type LoomArtRuntimeManifest,
  type LoomFramework,
  LoomMcpServer,
  LoomToolDefinition,
  LoomToolExecution,
  LoomWorkflowMetadata,
  readPythonArtSource,
  saveArtManagementSettings,
  saveToolDefinition,
  uninstallArtPackage,
  updateArtToVersion,
  updateMcpServerCredentials,
} from "../../services/loomApi";
import {
  artFrameworkIconKind,
  artFrameworkIconLabel,
  ArtIcon,
  artModeById,
  basenameWithoutExtension,
  normalizeToolId,
  parseListText,
  pythonProcessAdapterSource,
  StudioMessage,
} from "../app/appShell";
import { pushAppToast, requestAppConfirmation, requestAppConfirmationWithOption } from "../feedback/AppFeedback";
import { McpCredentialDialog } from "../mcp/McpHub";
import { AddArtWizard } from "./AddArtWizard";
import { ArtCreationDialog } from "./ArtCreationDialog";
import { ArtEditDialog } from "./ArtEditDialog";
import {
  ArtCreationRequest,
  ArtWizardSubmitDraft,
  defaultWizardPorts,
  recordValue,
  toolParamFromDraft,
  toolPortFromDraft,
  workflowBindingsFromDraft,
} from "./artWizardModel";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

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
  const [busyWizard, setBusyWizard] = useState(false);
  const [registryMessage, setRegistryMessage] = useState<StudioMessage | null>(null);
  const [editingTool, setEditingTool] = useState<LoomToolDefinition | null>(null);
  const [artManagement, setArtManagement] = useState<LoomArtManagement | null>(null);
  const [artManagementLoading, setArtManagementLoading] = useState(false);
  const [editBusyAction, setEditBusyAction] = useState<"save" | "update" | null>(null);
  const [editError, setEditError] = useState<string | null>(null);
  const [credentialServer, setCredentialServer] = useState<LoomMcpServer | null>(null);
  const [credentialBusy, setCredentialBusy] = useState(false);
  const editButtonRefs = useRef<Record<string, HTMLButtonElement | null>>({});
  const autoUpdateAttemptedUrl = useRef<string | null>(null);
  const visibleTools = useMemo(
    () => filterToolsByFrameworks(tools, frameworks, selectedFrameworkIds),
    [frameworks, selectedFrameworkIds, tools],
  );

  useEffect(() => {
    if (createDialogOpen) setRegistryMessage(null);
  }, [createDialogOpen]);

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
        const confirmation = mcpDependencies.length > 0
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
          text: removedCount || retainedCount
            ? `已卸载 ${tool.name || tool.id}；移除 ${removedCount} 个未使用 MCP，保留 ${retainedCount} 个仍在使用的 MCP。`
            : `已卸载 ${tool.name || tool.id}。`,
        });
      } else {
        if (!await requestAppConfirmation({
          title: "删除工具",
          message: `将删除 ${tool.name || tool.id}。此操作不可撤销。`,
          confirmLabel: "删除",
          tone: "danger",
        })) return;
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
    const toolId = editingTool?.id;
    setEditingTool(null);
    setArtManagement(null);
    setArtManagementLoading(false);
    setEditBusyAction(null);
    setEditError(null);
    if (toolId) {
      window.setTimeout(() => editButtonRefs.current[toolId]?.focus(), 0);
    }
  }, [editingTool]);

  const openToolEditor = async (tool: LoomToolDefinition) => {
    setEditingTool(tool);
    setArtManagement(null);
    setArtManagementLoading(true);
    setEditError(null);
    try {
      await autoUpdateArts(baseUrl);
      setArtManagement(await getArtManagement(baseUrl, tool.id));
    } catch (error) {
      setEditError(error instanceof Error ? error.message : "无法读取 Art 设置。");
    } finally {
      setArtManagementLoading(false);
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

  const saveMcpDependencyCredentials = async (
    values: Record<string, string>,
    clear: string[],
  ) => {
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

  const createArtToolFromWizard = async (draft: ArtWizardSubmitDraft): Promise<boolean> => {
    const selectedFramework = frameworks.find(
      (framework) => frameworkIdentity(framework) === `neuro.official/${draft.mode}`,
    ) ?? frameworks.find((framework) => frameworkIdentity(framework) === draft.mode);
    const modeInfo = artModeById(draft.mode);
    const selectedWorkflow = workflows.find((workflow) => workflow.id === draft.workflowId);
    const derivedName = draft.name.trim() || selectedWorkflow?.name || modeInfo.title;
    const repositoryName = normalizeToolId(draft.repositoryName || `${draft.mode}-${derivedName}`);
    const description = draft.description.trim() || modeInfo.subtitle;
    const workflowBindings = workflowBindingsFromDraft(draft);
    if (selectedFramework?.authoringSchema) {
      setBusyWizard(true);
      try {
        const authored = buildAuthoredArtPackage(selectedFramework, {
          id: repositoryName,
          name: derivedName,
          description,
          values: draft.frameworkValues,
          inputs: draft.inputPorts.map((port) => ({
            name: port.name,
            label: port.label,
            type: port.type,
            executionType: port.executionType,
          })),
          outputs: draft.outputPorts.map((port) => ({
            name: port.name,
            label: port.label,
            type: port.type,
            executionType: port.executionType,
          })),
        });
        authored.tool.inputs = draft.inputPorts.map((port) => toolPortFromDraft(port, "input"));
        authored.tool.params = draft.paramPorts.map(toolParamFromDraft);
        authored.tool.outputs = draft.outputPorts.map((port) => toolPortFromDraft(port, "output"));
        if (draft.mode === "workflow" && workflowBindings) {
          authored.tool.execution = {
            ...authored.tool.execution,
            workflowBindings,
          };
        }
        const templateMetadata = recordValue(draft.templateTool?.metadata);
        if (templateMetadata) {
          authored.tool.metadata = {
            ...templateMetadata,
            ...(recordValue(authored.tool.metadata) ?? {}),
          };
        }
        if (draft.mode === "cloud_api") {
          authored.tool.execution = {
            ...authored.tool.execution,
            contentType: draft.contentType.trim() || "application/json",
          };
        }
        if (draft.mode === "process" && draft.scriptEntryKind === "python") {
          if (!draft.scriptSourceCode.trim() && !draft.scriptSourcePath.trim()) {
            throw new Error("请输入 Python 源码路径或源码内容。");
          }
          const source = draft.scriptSourceCode.trim()
            ? draft.scriptSourceCode
            : (await readPythonArtSource(baseUrl, draft.scriptSourcePath.trim())).content;
          const metadata = authored.tool.metadata && typeof authored.tool.metadata === "object" && !Array.isArray(authored.tool.metadata)
            ? authored.tool.metadata as Record<string, unknown>
            : {};
          const existingAuthoring = metadata.authoring && typeof metadata.authoring === "object" && !Array.isArray(metadata.authoring)
            ? metadata.authoring as Record<string, unknown>
            : {};
          authored.tool.metadata = {
            ...metadata,
            authoring: {
              ...existingAuthoring,
              profile: "python_source",
              sourceName: basenameWithoutExtension(draft.scriptSourcePath || `${repositoryName}.py`),
            },
          };
          await createAuthoredArtPackage(
            baseUrl,
            authored.tool,
            {
              protocolVersion: "loom.art.runtime.v1",
              entry: { command: "python.exe", args: ["runtime/adapter.py"] },
            },
            {
              files: [
                { path: "runtime/adapter.py", content: pythonProcessAdapterSource },
                { path: "runtime/source.py", content: source },
              ],
            },
          );
        } else {
          await createAuthoredArtPackage(
            baseUrl,
            authored.tool,
            authored.runtime,
            draft.mode === "process" && draft.scriptSourceDirectory.trim()
              ? {
                  sourceDirectory: draft.scriptSourceDirectory.trim(),
                  sourceDirectoryTarget: "runtime/source",
                }
              : undefined,
          );
        }
        setRegistryMessage({
          kind: "info",
          text: `已创建并安装 Art ${derivedName}。`,
        });
        await refresh();
        await reloadFrameworks();
        return true;
      } catch (error) {
        setRegistryMessage({
          kind: "error",
          text: error instanceof Error ? error.message : "无法创建框架 Art 包。",
        });
        return false;
      } finally {
        setBusyWizard(false);
      }
    }
    let execution: LoomToolExecution;
    let runtime: LoomArtRuntimeManifest | undefined;
    const fallbackPorts = defaultWizardPorts(draft.mode);
    const inputs = (draft.inputPorts.length ? draft.inputPorts : fallbackPorts.inputs)
      .map((port) => toolPortFromDraft(port, "input"));
    const outputs = (draft.outputPorts.length ? draft.outputPorts : fallbackPorts.outputs)
      .map((port) => toolPortFromDraft(port, "output"));
    const params: LoomToolDefinition["params"] = draft.paramPorts.map(toolParamFromDraft);

    switch (draft.mode) {
      case "process": {
        execution = {
          type: "framework_art",
          framework: "process",
        };
        runtime = {
          protocolVersion: "loom.art.runtime.v1",
          entry: {
            command: draft.command.trim() || "echo",
            args: parseListText(draft.argsText),
          },
        };
        break;
      }
      case "cloud_api": {
        execution = {
          type: "cloud_api",
          endpoint: draft.endpoint.trim() || "http://127.0.0.1:8765/v1/shared-images/convert",
          method: draft.method.trim().toUpperCase() || "POST",
          contentType: draft.contentType.trim() || "application/json",
          headers: draft.headersText,
          body: draft.bodyText,
        };
        break;
      }
      case "mcp": {
        if (!draft.mcpServerId.trim() || !draft.mcpToolName.trim()) {
          setRegistryMessage({ kind: "error", text: "MCP 关联 Art 需要 MCP 服务和工具名。" });
          return false;
        }
        execution = {
          type: "mcp",
          serverId: draft.mcpServerId.trim(),
          toolName: draft.mcpToolName.trim(),
        };
        break;
      }
      case "workflow": {
        if (!draft.workflowId.trim()) {
          setRegistryMessage({ kind: "error", text: "工作流 Art 需要已保存工作流。" });
          return false;
        }
        execution = {
          type: "workflow",
          workflowId: draft.workflowId.trim(),
          ...(workflowBindings ? { workflowBindings } : {}),
        };
        break;
      }
      default: {
        setRegistryMessage({ kind: "error", text: `框架 ${draft.mode} 没有可用的 authoring schema。` });
        return false;
      }
    }

    const tool: LoomToolDefinition = {
      id: repositoryName,
      name: derivedName,
      description,
      enabled: true,
      execution,
      inputs,
      outputs,
      params,
      metadata: {
        ...(recordValue(draft.templateTool?.metadata) ?? {}),
        packageSecurity: { version: "0.1.0" },
        dependencies: {
          framework: selectedFramework?.qualifiedId || selectedFramework?.id || draft.mode,
        },
        authoring: { origin: "local", owner: "local-user" },
      },
    };

    setBusyWizard(true);
    try {
      await createAuthoredArtPackage(baseUrl, tool, runtime);
      setRegistryMessage({ kind: "info", text: `已创建并安装 Art ${derivedName}。` });
      await refresh();
      return true;
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : "无法通过添加 Art 向导创建 Art。",
      });
      return false;
    } finally {
      setBusyWizard(false);
    }
  };

  return (
    <section className="content-grid">
      {registryMessage ? (
        <p className={registryMessage.kind === "error" ? "error-text" : "success-text"}>{registryMessage.text}</p>
      ) : null}
      <div className="card-grid art-registry-grid">
        {visibleTools.map((tool) => {
          const enabled = tool.enabled !== false;
          const frameworkReference = artFrameworkReference(tool) || tool.execution?.type || null;
          const framework = frameworks.find((candidate) => (
            frameworkIdentity(candidate) === frameworkReference || candidate.id === frameworkReference
          ));
          const frameworkLabel = framework
            ? frameworkFilterLabel(framework)
            : artFrameworkIconLabel(frameworkReference);
          const mcpDependencies = resolveArtMcpDependencies(tool, mcpServers);
          const blockedMcpDependency = mcpDependencies.find((dependency) => dependency.status !== "ready");
          const displayedMcpDependency = blockedMcpDependency ?? mcpDependencies[0];
          const mcpStatus = displayedMcpDependency?.status;
          const mcpStatusText = mcpStatus === "credentials_required"
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
            <article
              className={`glass-card art-registry-card ${enabled ? "art-registry-card--enabled" : "art-registry-card--disabled"}`}
              key={tool.id}
            >
              <div className="art-registry-card__head">
                <h3 title={tool.name || tool.id}>{tool.name || tool.id}</h3>
                <span
                  className="art-registry-card__framework-icon"
                  role="img"
                  aria-label={`${frameworkLabel} Art`}
                  title={frameworkLabel}
                >
                  <ArtIcon kind={artFrameworkIconKind(frameworkReference)} />
                </span>
              </div>
              <div className="art-registry-card__body">
                {tool.description ? (
                  <p className="art-registry-card__description" title={tool.description}>{tool.description}</p>
                ) : null}
                {mcpStatusText && displayedMcpDependency ? (
                  mcpStatus === "credentials_required" && displayedMcpDependency.server ? (
                    <button
                      className="art-registry-card__mcp-configuration"
                      type="button"
                      title={displayedMcpDependency.dependencyId}
                      aria-label={`${tool.name || tool.id}: 配置 ${displayedMcpDependency.server.name || displayedMcpDependency.server.id} MCP 凭据`}
                      onClick={() => setCredentialServer(displayedMcpDependency.server)}
                      disabled={toolBusy || credentialBusy}
                    >
                      {mcpStatusText}
                    </button>
                  ) : (
                    <p
                      className={`art-registry-card__mcp-state art-registry-card__mcp-state--${mcpStatus}`}
                      title={displayedMcpDependency.dependencyId}
                    >
                      {mcpStatusText}
                    </p>
                  )
                ) : null}
              </div>
              <div className="art-registry-card__actions">
                <button
                  className="art-card-action"
                  type="button"
                  ref={(element) => { editButtonRefs.current[tool.id] = element; }}
                  aria-label={`编辑 ${tool.name || tool.id}`}
                  title="编辑"
                  onClick={() => {
                    void openToolEditor(tool);
                  }}
                  disabled={toolBusy}
                >
                  <ArtIcon kind="edit" />
                </button>
                <button
                  className={`art-card-action art-card-action--toggle ${enabled ? "art-card-action--active" : ""}`}
                  type="button"
                  aria-label={`${enabled ? "禁用" : "启用"} ${tool.name || tool.id}`}
                  aria-pressed={enabled}
                  aria-busy={toolBusy && busyToolAction === "toggle"}
                  title={enabled ? "禁用" : "启用"}
                  onClick={() => void toggleTool(tool)}
                  disabled={toolBusy}
                >
                  <ArtIcon kind="power" />
                </button>
                <button
                  className="art-card-action art-card-action--danger"
                  type="button"
                  aria-label={`删除 ${tool.name || tool.id}`}
                  aria-busy={toolBusy && busyToolAction === "delete"}
                  title="删除"
                  onClick={() => void removeTool(tool)}
                  disabled={toolBusy}
                >
                  <ArtIcon kind="trash" />
                </button>
              </div>
            </article>
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
