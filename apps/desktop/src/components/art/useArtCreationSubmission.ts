// Owns authored-package assembly and ordered creation submission; presentation remains in the registry.
import { useState } from "react";
import { useUiText } from "../../i18n/index.ts";
import { buildAuthoredArtPackage } from "../../services/artAuthoring";
import { frameworkIdentity } from "../../services/artHubUi";
import {
  createAuthoredArtPackage,
  readPythonArtSource,
  type LoomArtRuntimeManifest,
  type LoomFramework,
  type LoomToolDefinition,
  type LoomToolExecution,
  type LoomWorkflowMetadata,
} from "../../services/loomApi";
import {
  artModeById,
  basenameWithoutExtension,
  normalizeToolId,
  parseListText,
  pythonProcessAdapterSource,
  type StudioMessage,
} from "../app/appShell";
import {
  type ArtWizardSubmitDraft,
  defaultWizardPorts,
  recordValue,
  toolParamFromDraft,
  toolPortFromDraft,
  workflowBindingsFromDraft,
} from "./artWizardModel";

export function useArtCreationSubmission({
  frameworks,
  workflows,
  baseUrl,
  refresh,
  reloadFrameworks,
  setRegistryMessage,
}: {
  frameworks: LoomFramework[];
  workflows: LoomWorkflowMetadata[];
  baseUrl: string;
  refresh: () => Promise<void>;
  reloadFrameworks: () => Promise<void>;
  setRegistryMessage: (message: StudioMessage | null) => void;
}) {
  const { t, tf } = useUiText();
  const [busyWizard, setBusyWizard] = useState(false);
  const createArtToolFromWizard = async (draft: ArtWizardSubmitDraft): Promise<boolean> => {
    const selectedFramework =
      frameworks.find((framework) => frameworkIdentity(framework) === `neuro.official/${draft.mode}`) ??
      frameworks.find((framework) => frameworkIdentity(framework) === draft.mode);
    const modeInfo = artModeById(draft.mode);
    const selectedWorkflow = workflows.find((workflow) => workflow.id === draft.workflowId);
    const derivedName = draft.name.trim() || selectedWorkflow?.name || modeInfo.title;
    const identityName = draft.name.trim() || selectedWorkflow?.name;
    const repositoryName = normalizeToolId(
      draft.repositoryName || (identityName ? `${draft.mode}-${identityName}` : draft.mode),
    );
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
            throw new Error(t("art.wizard.error.missingPythonSource"));
          }
          const source = draft.scriptSourceCode.trim()
            ? draft.scriptSourceCode
            : (await readPythonArtSource(baseUrl, draft.scriptSourcePath.trim())).content;
          const metadata =
            authored.tool.metadata &&
            typeof authored.tool.metadata === "object" &&
            !Array.isArray(authored.tool.metadata)
              ? (authored.tool.metadata as Record<string, unknown>)
              : {};
          const existingAuthoring =
            metadata.authoring && typeof metadata.authoring === "object" && !Array.isArray(metadata.authoring)
              ? (metadata.authoring as Record<string, unknown>)
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
          text: tf("art.wizard.message.createdAndInstalled", { name: derivedName }),
        });
        await refresh();
        await reloadFrameworks();
        return true;
      } catch (error) {
        setRegistryMessage({
          kind: "error",
          text: error instanceof Error ? error.message : t("art.wizard.error.createPackageFailed"),
        });
        return false;
      } finally {
        setBusyWizard(false);
      }
    }
    let execution: LoomToolExecution;
    let runtime: LoomArtRuntimeManifest | undefined;
    const fallbackPorts = defaultWizardPorts(draft.mode);
    const inputs = (draft.inputPorts.length ? draft.inputPorts : fallbackPorts.inputs).map((port) =>
      toolPortFromDraft(port, "input"),
    );
    const outputs = (draft.outputPorts.length ? draft.outputPorts : fallbackPorts.outputs).map((port) =>
      toolPortFromDraft(port, "output"),
    );
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
          setRegistryMessage({ kind: "error", text: t("art.wizard.error.mcpFieldsRequired") });
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
          setRegistryMessage({ kind: "error", text: t("art.wizard.error.workflowRequired") });
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
        setRegistryMessage({
          kind: "error",
          text: tf("art.wizard.error.missingAuthoringSchema", { framework: draft.mode }),
        });
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
      setRegistryMessage({ kind: "info", text: tf("art.wizard.message.createdAndInstalled", { name: derivedName }) });
      await refresh();
      return true;
    } catch (error) {
      setRegistryMessage({
        kind: "error",
        text: error instanceof Error ? error.message : t("art.wizard.error.createWizardFailed"),
      });
      return false;
    } finally {
      setBusyWizard(false);
    }
  };
  return { busyWizard, createArtToolFromWizard };
}
