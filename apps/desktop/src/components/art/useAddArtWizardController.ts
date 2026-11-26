// Owns cross-mode authoring state, workflow hydration, and submission assembly.
import { defaultAuthoringValues } from "../../services/artAuthoring";
import { frameworkIdentity } from "../../services/artHubUi";
import {
  listPluginCredentials,
  type LoomFramework,
  type LoomMcpServer,
  type LoomToolDefinition,
  type LoomWorkflowMetadata,
} from "../../services/loomApi";
import {
  autoTemplateResponse,
  collectWorkflowPreviewNodeOptions,
  parseCurlCommand,
  parseRawCommand,
  type WorkflowGraphLite,
  type WorkflowOutputBinding,
  type WorkflowPreviewNodeOption,
} from "../../services/workflowStudio";
import {
  artModeById,
  type ArtWizardMode,
  defaultCurlCommand,
  defaultResponseSample,
  type StudioMessage,
} from "../app/appShell";
import {
  applyWorkflowInputBindingsToDrafts,
  applyWorkflowOutputBindingToDrafts,
  type ArtCreationRequest,
  type ArtWizardPortDraft,
  type ArtWizardSubmitDraft,
  createPortDraft,
  defaultWizardPorts,
  portDraftFromParsedPort,
  toolPortDrafts,
  workflowBindingsFromTool,
} from "./artWizardModel";
import { useEffect, useMemo, useRef, useState } from "react";
import { useUiText } from "../../i18n/index.ts";
import { useArtScriptSource } from "./useArtScriptSource";
import { useArtMcpDiscovery } from "./useArtMcpDiscovery";
import { useArtWorkflowInterface } from "./useArtWorkflowInterface";

export function useAddArtWizardController({
  baseUrl,
  frameworks,
  mcpServers,
  workflows,
  tools,
  initialRequest,
  busy,
  onCreate,
}: {
  baseUrl: string;
  frameworks: LoomFramework[];
  mcpServers: LoomMcpServer[];
  workflows: LoomWorkflowMetadata[];
  tools: LoomToolDefinition[];
  initialRequest: ArtCreationRequest | null;
  busy: boolean;
  onCreate: (draft: ArtWizardSubmitDraft) => Promise<boolean>;
}) {
  const { t } = useUiText();
  const initialMode = initialRequest?.mode ?? "cloud_api";
  const initialTemplate = initialRequest?.templateTool;
  const initialWorkflowBindings = workflowBindingsFromTool(initialTemplate);
  const [mode, setMode] = useState<ArtWizardMode>(initialMode);
  const [frameworkValues, setFrameworkValues] = useState<Record<string, unknown>>(
    initialRequest?.workflowId ? { workflowId: initialRequest.workflowId } : {},
  );
  const [repositoryName, setRepositoryName] = useState(initialRequest?.repositoryName ?? "");
  const [name, setName] = useState(initialRequest?.name ?? "");
  const [description, setDescription] = useState(initialRequest?.description ?? "");
  const [command, setCommand] = useState("python.exe");
  const [argsText, setArgsText] = useState("");
  const [endpoint, setEndpoint] = useState("https://api.example.com/v1/process");
  const [method, setMethod] = useState("POST");
  const [contentType, setContentType] = useState("application/json");
  const [headersText, setHeadersText] = useState('{"Content-Type":"application/json"}');
  const [bodyText, setBodyText] = useState('{"image":"{{inputs.image.path}}"}');
  const [mcpServerId, setMcpServerId] = useState("");
  const [mcpToolName, setMcpToolName] = useState("");
  const [mcpArgumentsText, setMcpArgumentsText] = useState("{}");
  const [workflowId, setWorkflowId] = useState(initialRequest?.workflowId ?? "");
  const [workflowGraph, setWorkflowGraph] = useState<WorkflowGraphLite | null>(null);
  const [workflowPreviewOutput, setWorkflowPreviewOutput] = useState<WorkflowOutputBinding | undefined>(
    initialMode === "workflow" ? initialWorkflowBindings.previewOutput : undefined,
  );
  const [workflowPreviewRequiredNodes, setWorkflowPreviewRequiredNodes] = useState<string[]>(
    initialMode === "workflow" ? (initialWorkflowBindings.previewRequiredNodes ?? []) : [],
  );
  const [rawCommandText, setRawCommandText] = useState("ffmpeg -i {{inputs.image.path}} {{outputs.result.path}}");
  const [cloudCurlText, setCloudCurlText] = useState(defaultCurlCommand);
  const [cloudResponseText, setCloudResponseText] = useState(defaultResponseSample);
  const [scriptEntryKind, setScriptEntryKind] = useState<"python" | "command">("python");
  const [inputPorts, setInputPorts] = useState<ArtWizardPortDraft[]>(() => {
    const ports = toolPortDrafts(initialTemplate?.inputs, "input");
    return applyWorkflowInputBindingsToDrafts(
      ports.length ? ports : defaultWizardPorts(initialMode).inputs,
      initialTemplate,
      new Set(["input_image", "input_value"]),
    );
  });
  const [paramPorts, setParamPorts] = useState<ArtWizardPortDraft[]>(() =>
    applyWorkflowInputBindingsToDrafts(
      toolPortDrafts(initialTemplate?.params, "input"),
      initialTemplate,
      new Set(["param"]),
      true,
    ),
  );
  const [outputPorts, setOutputPorts] = useState<ArtWizardPortDraft[]>(() => {
    const ports = toolPortDrafts(initialTemplate?.outputs, "output");
    return applyWorkflowOutputBindingToDrafts(
      ports.length ? ports : defaultWizardPorts(initialMode).outputs,
      initialTemplate,
    );
  });
  const [wizardMessage, setWizardMessage] = useState<StudioMessage | null>(null);
  const [credentialNames, setCredentialNames] = useState<string[]>([]);
  const workflowInterfaceAppliedRef = useRef("");
  const {
    scriptSourcePath,
    setScriptSourcePath,
    scriptSourceCode,
    setScriptSourceCode,
    scriptArtJsonPath,
    setScriptArtJsonPath,
    scriptSourceDirectory,
    setScriptSourceDirectory,
    sourceBusyAction,
    readScriptSource,
    readScriptArtJson,
    inferScriptPorts,
  } = useArtScriptSource({
    baseUrl,
    repositoryName,
    name,
    setRepositoryName,
    setName,
    setDescription,
    setInputPorts,
    setOutputPorts,
    setWizardMessage,
  });
  const {
    mcpTools,
    selectedMcpSchemaToolName,
    setSelectedMcpSchemaToolName,
    mcpDiscoveryBusy,
    mcpToolLabel,
    discoverMcpTools,
    useSelectedMcpToolSchema,
  } = useArtMcpDiscovery({
    baseUrl,
    mcpServers,
    mcpServerId,
    mcpToolName,
    setMcpToolName,
    setInputPorts,
    setOutputPorts,
    setWizardMessage,
  });

  const workflowPreviewNodeOptions = useMemo(
    () => (workflowGraph ? collectWorkflowPreviewNodeOptions(workflowGraph, tools) : []),
    [tools, workflowGraph],
  );
  const selectedMode = artModeById(mode);
  const selectedFramework = useMemo(
    () =>
      frameworks.find((framework) => frameworkIdentity(framework) === `neuro.official/${mode}`) ??
      frameworks.find((framework) => frameworkIdentity(framework) === mode),
    [frameworks, mode],
  );
  const selectedAuthoringSchema = selectedFramework?.authoringSchema ?? null;
  const selectedFrameworkReady = Boolean(
    selectedFramework?.installed && selectedFramework.enabled && selectedFramework.ready && selectedAuthoringSchema,
  );
  const handledFieldIds = useMemo(
    () =>
      new Set(
        mode === "cloud_api"
          ? ["endpoint", "method", "headers", "body"]
          : mode === "mcp"
            ? ["serverId", "toolName", "arguments"]
            : mode === "workflow"
              ? ["workflowId"]
              : ["runtimeCommand", "runtimeArgs"],
      ),
    [mode],
  );
  const additionalAuthoringFields = (selectedAuthoringSchema?.fields ?? []).filter(
    (field) => !handledFieldIds.has(field.id),
  );

  useEffect(() => {
    if (!mcpServerId && mcpServers[0]) setMcpServerId(mcpServers[0].id);
  }, [mcpServerId, mcpServers]);

  useEffect(() => {
    if (!workflowId && workflows[0]) setWorkflowId(workflows[0].id);
  }, [workflowId, workflows]);

  useEffect(() => {
    let active = true;
    void listPluginCredentials(baseUrl)
      .then((credentials) => {
        if (!active) return;
        setCredentialNames(
          credentials
            .filter((credential) => !credential.scope.frameworkId && !credential.scope.artId)
            .map((credential) => credential.name),
        );
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [baseUrl]);

  useEffect(() => {
    const defaults = defaultWizardPorts(mode);
    const schema = selectedFramework?.authoringSchema;
    const template = initialRequest?.mode === mode ? initialRequest.templateTool : undefined;
    const templateInputs = toolPortDrafts(template?.inputs, "input");
    const templateParams = toolPortDrafts(template?.params, "input");
    const templateOutputs = toolPortDrafts(template?.outputs, "output");
    setInputPorts(
      applyWorkflowInputBindingsToDrafts(
        templateInputs.length
          ? templateInputs
          : (schema?.inputs?.map((port) =>
              createPortDraft("input", {
                name: port.name,
                label: port.label,
                type: port.type,
                executionType: port.executionType,
              }),
            ) ?? defaults.inputs),
        template,
        new Set(["input_image", "input_value"]),
      ),
    );
    setParamPorts(applyWorkflowInputBindingsToDrafts(templateParams, template, new Set(["param"]), true));
    setOutputPorts(
      applyWorkflowOutputBindingToDrafts(
        templateOutputs.length
          ? templateOutputs
          : (schema?.outputs?.map((port) =>
              createPortDraft("output", {
                name: port.name,
                label: port.label,
                type: port.type,
                executionType: port.executionType,
              }),
            ) ?? defaults.outputs),
        template,
      ),
    );
    setFrameworkValues({
      ...defaultAuthoringValues(schema?.fields),
      ...(initialRequest?.mode === mode && initialRequest.workflowId ? { workflowId: initialRequest.workflowId } : {}),
    });
    setWizardMessage(null);
    const templateBindings = workflowBindingsFromTool(template);
    setWorkflowGraph(null);
    setWorkflowPreviewOutput(mode === "workflow" ? templateBindings.previewOutput : undefined);
    setWorkflowPreviewRequiredNodes(mode === "workflow" ? (templateBindings.previewRequiredNodes ?? []) : []);
    workflowInterfaceAppliedRef.current = "";
  }, [initialRequest, mode, selectedFramework]);
  const { workflowParamCandidates, workflowInterfaceLoading, workflowInterfaceError } = useArtWorkflowInterface({
    baseUrl,
    initialRequest,
    mode,
    tools,
    workflowId,
    workflowInterfaceAppliedRef,
    setWorkflowGraph,
    setInputPorts,
    setOutputPorts,
    setParamPorts,
    setWorkflowPreviewOutput,
    setWorkflowPreviewRequiredNodes,
  });

  const importRawCommand = () => {
    const parsed = parseRawCommand(rawCommandText);
    if (!parsed) {
      setWizardMessage({ kind: "error", text: t("art.wizard.message.enterCommand") });
      return;
    }
    setCommand(parsed.command);
    setArgsText(parsed.argsText);
    const parsedInputs = parsed.ports
      .filter((port) => port.isInput)
      .map((port) => portDraftFromParsedPort(port, "input"));
    const parsedOutputs = parsed.ports
      .filter((port) => !port.isInput)
      .map((port) => portDraftFromParsedPort(port, "output"));
    if (parsedInputs.length) setInputPorts(parsedInputs);
    if (parsedOutputs.length) setOutputPorts(parsedOutputs);
    setWizardMessage({ kind: "info", text: t("art.wizard.message.parsedCommand") });
  };

  const importCloudSmartTemplate = () => {
    const parsed = parseCurlCommand(cloudCurlText);
    if (!parsed) {
      setWizardMessage({ kind: "error", text: t("art.wizard.message.enterValidCurl") });
      return;
    }
    setEndpoint(parsed.url || endpoint);
    setMethod(parsed.method || "POST");
    if (Object.keys(parsed.headers).length) setHeadersText(JSON.stringify(parsed.headers, null, 2));
    const nextContentType = parsed.headers["Content-Type"] || parsed.headers["content-type"];
    if (nextContentType) setContentType(nextContentType);
    if (parsed.body) setBodyText(parsed.body);
    if (parsed.suggestedInputs.length)
      setInputPorts(parsed.suggestedInputs.map((port) => portDraftFromParsedPort(port, "input")));
    const responsePreview = autoTemplateResponse(cloudResponseText);
    if (responsePreview.ports.length)
      setOutputPorts(responsePreview.ports.map((port) => portDraftFromParsedPort(port, "output")));
    setWizardMessage({ kind: "info", text: t("art.wizard.message.importedCurl") });
  };

  const setWorkflowNodeRequired = (nodeId: string, required: boolean) => {
    setWorkflowPreviewRequiredNodes((current) => {
      const next = new Set(current);
      if (required || workflowPreviewOutput?.nodeId === nodeId) next.add(nodeId);
      else next.delete(nodeId);
      return [...next];
    });
  };

  const selectWorkflowPreviewNode = (option: WorkflowPreviewNodeOption) => {
    const output =
      option.outputs.find(
        (candidate) =>
          workflowPreviewOutput?.nodeId === option.nodeId && workflowPreviewOutput.output === candidate.name,
      ) ?? option.outputs[0];
    if (!output) return;
    setWorkflowPreviewOutput({ nodeId: option.nodeId, output: output.name, kind: "node_result" });
    setWorkflowNodeRequired(option.nodeId, true);
  };

  const submit = async () => {
    const values = { ...frameworkValues };
    if (mode === "cloud_api") Object.assign(values, { endpoint, method, headers: headersText, body: bodyText });
    if (mode === "mcp")
      Object.assign(values, { serverId: mcpServerId, toolName: mcpToolName, arguments: mcpArgumentsText });
    if (mode === "workflow") Object.assign(values, { workflowId });
    if (mode === "process")
      Object.assign(values, {
        runtimeCommand: scriptEntryKind === "python" ? "python.exe" : command,
        runtimeArgs: scriptEntryKind === "python" ? "runtime/adapter.py" : argsText,
      });
    await onCreate({
      mode,
      frameworkValues: values,
      repositoryName,
      name,
      description,
      command,
      argsText,
      endpoint,
      method,
      contentType,
      headersText,
      bodyText,
      mcpServerId,
      mcpToolName,
      workflowId,
      workflowPreviewOutput,
      workflowPreviewRequiredNodes,
      scriptEntryKind,
      scriptSourcePath,
      scriptSourceCode,
      scriptSourceDirectory,
      inputPorts,
      paramPorts,
      outputPorts,
      templateTool: initialRequest?.mode === mode ? initialRequest.templateTool : undefined,
    });
  };

  return {
    additionalAuthoringFields,
    argsText,
    bodyText,
    cloudCurlText,
    cloudResponseText,
    command,
    contentType,
    credentialNames,
    description,
    discoverMcpTools,
    endpoint,
    frameworkValues,
    headersText,
    importCloudSmartTemplate,
    importRawCommand,
    inferScriptPorts,
    inputPorts,
    mcpArgumentsText,
    mcpDiscoveryBusy,
    mcpServerId,
    mcpToolLabel,
    mcpToolName,
    mcpTools,
    method,
    mode,
    name,
    outputPorts,
    paramPorts,
    rawCommandText,
    readScriptArtJson,
    readScriptSource,
    repositoryName,
    scriptArtJsonPath,
    scriptEntryKind,
    scriptSourceCode,
    scriptSourceDirectory,
    scriptSourcePath,
    selectWorkflowPreviewNode,
    selectedFrameworkReady,
    selectedMcpSchemaToolName,
    selectedMode,
    setArgsText,
    setBodyText,
    setCloudCurlText,
    setCloudResponseText,
    setCommand,
    setContentType,
    setDescription,
    setEndpoint,
    setFrameworkValues,
    setHeadersText,
    setInputPorts,
    setMcpArgumentsText,
    setMcpServerId,
    setMcpToolName,
    setMethod,
    setMode,
    setName,
    setOutputPorts,
    setParamPorts,
    setRawCommandText,
    setRepositoryName,
    setScriptArtJsonPath,
    setScriptEntryKind,
    setScriptSourceCode,
    setScriptSourceDirectory,
    setScriptSourcePath,
    setSelectedMcpSchemaToolName,
    setWorkflowGraph,
    setWorkflowId,
    setWorkflowNodeRequired,
    setWorkflowPreviewOutput,
    setWorkflowPreviewRequiredNodes,
    sourceBusyAction,
    submit,
    useSelectedMcpToolSchema,
    wizardMessage,
    workflowId,
    workflowInterfaceAppliedRef,
    workflowInterfaceError,
    workflowInterfaceLoading,
    workflowParamCandidates,
    workflowPreviewNodeOptions,
    workflowPreviewOutput,
    workflowPreviewRequiredNodes,
  };
}
