// Owns installed Art registry actions and authoring orchestration.
import { Badge, Button, IconButton, Notice, Panel } from "@neuro/ui";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useUiText } from "../../i18n/index.ts";
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
  const { t, tf } = useUiText();
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
    const displayName = tool.name || tool.id;
    try {
      const packageIdentity = artPackageIdentity(tool);
      if (packageIdentity) {
        const mcpDependencies = artMcpDependencyIds(tool);
        const confirmation =
          mcpDependencies.length > 0
            ? await requestAppConfirmationWithOption({
                title: t("art.registry.uninstallConfirm.title"),
                message: tf("art.registry.uninstallConfirm.withMcpMessage", {
                  name: displayName,
                  count: mcpDependencies.length,
                }),
                optionLabel: t("art.registry.uninstallConfirm.removeUnusedMcpOption"),
                optionDefault: false,
                confirmLabel: t("art.action.uninstall"),
                tone: "danger",
              })
            : {
                accepted: await requestAppConfirmation({
                  title: t("art.registry.uninstallConfirm.title"),
                  message: tf("art.registry.uninstallConfirm.basicMessage", { name: displayName }),
                  confirmLabel: t("art.action.uninstall"),
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
              ? tf("art.registry.uninstalledWithMcpSummary", {
                  name: displayName,
                  removed: removedCount,
                  retained: retainedCount,
                })
              : tf("art.registry.uninstalledMessage", { name: displayName }),
        });
      } else {
        if (
          !(await requestAppConfirmation({
            title: t("art.registry.deleteToolConfirm.title"),
            message: tf("art.registry.deleteToolConfirm.message", { name: displayName }),
            confirmLabel: t("art.action.delete"),
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
        text: error instanceof Error ? error.message : t("art.registry.deleteToolFailed"),
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
        text: tf(nextEnabled ? "art.registry.enabledMessage" : "art.registry.disabledMessage", {
          name: tool.name || tool.id,
        }),
      });
      await refresh();
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : t("art.registry.toggleFailed"),
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
        setEditError(error instanceof Error ? error.message : t("art.registry.loadSettingsFailed"));
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
      const detail = error instanceof Error ? error.message : t("art.registry.updateFailed");
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
      const detail = error instanceof Error ? error.message : t("art.registry.updateVersionFailed");
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
      const message = tf("art.registry.mcpCredentialsUpdated", {
        name: credentialServer.name || credentialServer.id,
      });
      setCredentialServer(null);
      setRegistryMessage({ kind: "info", text: message });
      pushAppToast({ level: "info", text: message });
      await refresh();
    } catch (error) {
      const detail = error instanceof Error ? error.message : t("art.registry.mcpCredentialsSaveFailed");
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
          const displayName = tool.name || tool.id;
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
              ? t("art.registry.mcpStatus.credentialsRequired")
              : mcpStatus === "disabled"
                ? t("art.registry.mcpStatus.disabled")
                : mcpStatus === "missing"
                  ? t("art.registry.mcpStatus.missing")
                  : mcpStatus === "ready"
                    ? t("art.registry.mcpStatus.ready")
                    : null;
          const toolBusy = busyToolId === tool.id;
          return (
            <Panel className="art-registry-entry" data-enabled={enabled} key={tool.id}>
              <div className="art-registry-card__head">
                <h3 title={displayName}>{displayName}</h3>
                <div className="art-registry-entry__state">
                  <Badge tone={enabled ? "success" : "neutral"}>
                    {enabled ? t("art.status.enabled") : t("art.status.disabled")}
                  </Badge>
                  <span
                    className="art-registry-card__framework-icon"
                    role="img"
                    aria-label={tf("art.card.frameworkIconAria", { framework: frameworkLabel })}
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
                      aria-label={tf("art.registry.configureMcpCredentialsAria", {
                        art: displayName,
                        server: displayedMcpDependency.server.name || displayedMcpDependency.server.id,
                      })}
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
                  label={tf("art.action.editFor", { name: displayName })}
                  title={t("art.action.edit")}
                  onClick={() => {
                    void openToolEditor(tool);
                  }}
                  disabled={toolBusy}
                >
                  <ArtIcon kind="edit" />
                </IconButton>
                <IconButton
                  variant={enabled ? "positive" : "secondary"}
                  label={tf(enabled ? "art.action.disableFor" : "art.action.enableFor", { name: displayName })}
                  aria-pressed={enabled}
                  loading={toolBusy && busyToolAction === "toggle"}
                  title={enabled ? t("art.action.disable") : t("art.action.enable")}
                  onClick={() => void toggleTool(tool)}
                  disabled={toolBusy}
                >
                  <ArtIcon kind="power" />
                </IconButton>
                <IconButton
                  variant="danger"
                  label={tf("art.action.deleteFor", { name: displayName })}
                  loading={toolBusy && busyToolAction === "delete"}
                  title={t("art.action.delete")}
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
