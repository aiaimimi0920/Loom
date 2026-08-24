// Owns Art wizard state, source discovery, and submission side effects.
import { defaultAuthoringValues } from "../../services/artAuthoring";
import { frameworkIdentity } from "../../services/artHubUi";
import {
  checkPythonArtJsonNearby,
  getWorkflowBundle,
  inferPythonArtPorts,
  listPluginCredentials,
  type LoomFramework,
  LoomMcpServer,
  LoomToolDefinition,
  LoomWorkflowMetadata,
  readPythonArtJson,
  readPythonArtSource,
  testMcpConnection,
} from "../../services/loomApi";
import { inferPortsFromPythonCode, mapArtJsonPorts, type PythonArtPort } from "../../services/pythonArtSource";
import {
  autoTemplateResponse,
  collectWorkflowParamBindingCandidates,
  collectWorkflowPreviewNodeOptions,
  inferWorkflowArtInterface,
  parseCurlCommand,
  parseRawCommand,
  parseWorkflowYamlLite,
  portsFromMcpToolSchema,
  type WorkflowGraphLite,
  type WorkflowOutputBinding,
  type WorkflowParamBindingCandidate,
  type WorkflowPreviewNodeOption,
} from "../../services/workflowStudio";
import {
  artModeById,
  ArtWizardMode,
  artWizardModes,
  basenameWithoutExtension,
  defaultCurlCommand,
  defaultResponseSample,
  normalizePythonPort,
  normalizeToolId,
  StudioMessage,
} from "../app/appShell";
import {
  applyWorkflowInputBindingsToDrafts,
  applyWorkflowOutputBindingToDrafts,
  ArtCreationRequest,
  ArtWizardPortDraft,
  ArtWizardSubmitDraft,
  createPortDraft,
  defaultWizardPorts,
  portDraftFromParsedPort,
  toolPortDrafts,
  workflowBindingsFromTool,
} from "./artWizardModel";
import {
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

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
    initialMode === "workflow" ? initialWorkflowBindings.previewRequiredNodes ?? [] : [],
  );
  const [rawCommandText, setRawCommandText] = useState("ffmpeg -i {{inputs.image.path}} {{outputs.result.path}}");
  const [cloudCurlText, setCloudCurlText] = useState(defaultCurlCommand);
  const [cloudResponseText, setCloudResponseText] = useState(defaultResponseSample);
  const [scriptEntryKind, setScriptEntryKind] = useState<"python" | "command">("python");
  const [scriptSourcePath, setScriptSourcePath] = useState("");
  const [scriptSourceCode, setScriptSourceCode] = useState("");
  const [scriptArtJsonPath, setScriptArtJsonPath] = useState("");
  const [scriptSourceDirectory, setScriptSourceDirectory] = useState("");
  const [sourceBusyAction, setSourceBusyAction] = useState<"read" | "art-json" | "infer" | null>(null);
  const [inputPorts, setInputPorts] = useState<ArtWizardPortDraft[]>(() => {
    const ports = toolPortDrafts(initialTemplate?.inputs, "input");
    return applyWorkflowInputBindingsToDrafts(
      ports.length ? ports : defaultWizardPorts(initialMode).inputs,
      initialTemplate,
      new Set(["input_image", "input_value"]),
    );
  });
  const [paramPorts, setParamPorts] = useState<ArtWizardPortDraft[]>(() => (
    applyWorkflowInputBindingsToDrafts(
      toolPortDrafts(initialTemplate?.params, "input"),
      initialTemplate,
      new Set(["param"]),
      true,
    )
  ));
  const [outputPorts, setOutputPorts] = useState<ArtWizardPortDraft[]>(() => {
    const ports = toolPortDrafts(initialTemplate?.outputs, "output");
    return applyWorkflowOutputBindingToDrafts(
      ports.length ? ports : defaultWizardPorts(initialMode).outputs,
      initialTemplate,
    );
  });
  const [wizardMessage, setWizardMessage] = useState<StudioMessage | null>(null);
  const [mcpTools, setMcpTools] = useState<unknown[]>([]);
  const [selectedMcpSchemaToolName, setSelectedMcpSchemaToolName] = useState("");
  const [mcpDiscoveryBusy, setMcpDiscoveryBusy] = useState(false);
  const [credentialNames, setCredentialNames] = useState<string[]>([]);
  const [workflowParamCandidates, setWorkflowParamCandidates] = useState<WorkflowParamBindingCandidate[]>([]);
  const [workflowInterfaceLoading, setWorkflowInterfaceLoading] = useState(false);
  const [workflowInterfaceError, setWorkflowInterfaceError] = useState<string | null>(null);
  const workflowInterfaceAppliedRef = useRef("");
  const sourceRequestVersionRef = useRef(0);
  const mcpDiscoveryVersionRef = useRef(0);
  const workflowPreviewNodeOptions = useMemo(
    () => workflowGraph ? collectWorkflowPreviewNodeOptions(workflowGraph, tools) : [],
    [tools, workflowGraph],
  );
  const selectedMode = artModeById(mode);
  const selectedFramework = useMemo(() => (
    frameworks.find((framework) => frameworkIdentity(framework) === `neuro.official/${mode}`)
    ?? frameworks.find((framework) => frameworkIdentity(framework) === mode)
  ), [frameworks, mode]);
  const selectedAuthoringSchema = selectedFramework?.authoringSchema ?? null;
  const selectedFrameworkReady = Boolean(
    selectedFramework?.installed
    && selectedFramework.enabled
    && selectedFramework.ready
    && selectedAuthoringSchema,
  );
  const handledFieldIds = useMemo(() => new Set(
    mode === "cloud_api" ? ["endpoint", "method", "headers", "body"]
      : mode === "mcp" ? ["serverId", "toolName", "arguments"]
        : mode === "workflow" ? ["workflowId"]
          : ["runtimeCommand", "runtimeArgs"],
  ), [mode]);
  const additionalAuthoringFields = (selectedAuthoringSchema?.fields ?? [])
    .filter((field) => !handledFieldIds.has(field.id));

  useEffect(() => () => {
    sourceRequestVersionRef.current += 1;
    mcpDiscoveryVersionRef.current += 1;
  }, []);

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
        setCredentialNames(credentials
          .filter((credential) => !credential.scope.frameworkId && !credential.scope.artId)
          .map((credential) => credential.name));
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
    setInputPorts(applyWorkflowInputBindingsToDrafts(
      templateInputs.length ? templateInputs : schema?.inputs?.map((port) => createPortDraft("input", {
        name: port.name,
        label: port.label,
        type: port.type,
        executionType: port.executionType,
      })) ?? defaults.inputs,
      template,
      new Set(["input_image", "input_value"]),
    ));
    setParamPorts(applyWorkflowInputBindingsToDrafts(
      templateParams,
      template,
      new Set(["param"]),
      true,
    ));
    setOutputPorts(applyWorkflowOutputBindingToDrafts(
      templateOutputs.length ? templateOutputs : schema?.outputs?.map((port) => createPortDraft("output", {
        name: port.name,
        label: port.label,
        type: port.type,
        executionType: port.executionType,
      })) ?? defaults.outputs,
      template,
    ));
    setFrameworkValues({
      ...defaultAuthoringValues(schema?.fields),
      ...(initialRequest?.mode === mode && initialRequest.workflowId
        ? { workflowId: initialRequest.workflowId }
        : {}),
    });
    setWizardMessage(null);
    const templateBindings = workflowBindingsFromTool(template);
    setWorkflowGraph(null);
    setWorkflowPreviewOutput(mode === "workflow" ? templateBindings.previewOutput : undefined);
    setWorkflowPreviewRequiredNodes(
      mode === "workflow" ? templateBindings.previewRequiredNodes ?? [] : [],
    );
    workflowInterfaceAppliedRef.current = "";
  }, [initialRequest, mode, selectedFramework]);

  useEffect(() => {
    if (mode !== "workflow" || !workflowId.trim()) {
      setWorkflowParamCandidates([]);
      setWorkflowGraph(null);
      setWorkflowInterfaceLoading(false);
      setWorkflowInterfaceError(null);
      return;
    }

    let active = true;
    setWorkflowInterfaceLoading(true);
    setWorkflowInterfaceError(null);
    void getWorkflowBundle(baseUrl, workflowId.trim())
      .then((bundle) => {
        if (!active) return;
        const workflow = parseWorkflowYamlLite(bundle.data);
        const candidates = collectWorkflowParamBindingCandidates(workflow, tools);
        const inferred = inferWorkflowArtInterface(workflow, tools);
        setWorkflowParamCandidates(candidates);
        setWorkflowGraph(workflow);

        if (workflowInterfaceAppliedRef.current !== workflowId) {
          const templateMatches = initialRequest?.mode === "workflow"
            && initialRequest.workflowId === workflowId
            && Boolean(initialRequest.templateTool);
          if (!templateMatches) {
            const inferredInputs = inferred.inputs
              .filter((port) => port.bindingKind !== "param")
              .map((port) => createPortDraft("input", {
                name: port.name,
                label: port.label,
                type: port.type,
                executionType: port.executionType,
                defaultValue: port.default || "",
                bindingNodeId: port.bindingNodeId || "",
                bindingTarget: port.bindingTarget || "",
                bindingKind: port.bindingKind || "",
              }));
            const inferredOutputs = inferred.outputs.map((port) => createPortDraft("output", {
              name: port.name,
              label: port.label,
              type: port.type,
              executionType: port.executionType,
              bindingNodeId: port.bindingNodeId || "",
              bindingTarget: port.bindingTarget || "",
              bindingKind: port.bindingKind || "",
            }));
            if (inferredInputs.length) setInputPorts(inferredInputs);
            if (inferredOutputs.length) setOutputPorts(inferredOutputs);
            setParamPorts([]);
            setWorkflowPreviewOutput(inferred.bindings.primaryOutput);
            setWorkflowPreviewRequiredNodes(
              inferred.bindings.primaryOutput ? [inferred.bindings.primaryOutput.nodeId] : [],
            );
          }
          workflowInterfaceAppliedRef.current = workflowId;
        }
      })
      .catch((error) => {
        if (!active) return;
        setWorkflowParamCandidates([]);
        setWorkflowGraph(null);
        setWorkflowInterfaceError(error instanceof Error ? error.message : "无法读取流程结构。");
      })
      .finally(() => {
        if (active) setWorkflowInterfaceLoading(false);
      });
    return () => {
      active = false;
    };
  }, [baseUrl, initialRequest, mode, tools, workflowId]);

  const mcpToolLabel = (tool: unknown) => {
    if (tool && typeof tool === "object" && !Array.isArray(tool)) {
      const record = tool as Record<string, unknown>;
      if (typeof record.name === "string" && record.name.trim()) return record.name.trim();
    }
    return "mcp_tool";
  };

  const applyPythonPorts = (ports: { inputs: PythonArtPort[]; outputs: PythonArtPort[] }) => {
    setInputPorts(ports.inputs.map((port) => createPortDraft("input", {
      name: port.name,
      label: port.label,
      type: port.type,
      executionType: port.executionType || port.execution_type,
      defaultValue: typeof port.default === "string" ? port.default : JSON.stringify(port.default ?? ""),
    })));
    setOutputPorts(ports.outputs.map((port) => createPortDraft("output", {
      name: port.name,
      label: port.label,
      type: port.type,
      executionType: port.executionType || port.execution_type,
    })));
  };

  const applyArtJson = (artJson: unknown, sourcePath = scriptSourcePath) => {
    applyPythonPorts(mapArtJsonPorts(artJson));
    if (!artJson || typeof artJson !== "object" || Array.isArray(artJson)) return;
    const record = artJson as Record<string, unknown>;
    const artId = typeof record.art_id === "string" ? record.art_id : basenameWithoutExtension(sourcePath);
    const label = typeof record.label === "string" ? record.label : artId;
    const nextDescription = typeof record.description === "string" ? record.description : "";
    if (artId) setRepositoryName(normalizeToolId(artId));
    if (label) setName(label);
    if (nextDescription) setDescription(nextDescription);
  };

  const readScriptSource = async () => {
    if (!scriptSourcePath.trim()) {
      setWizardMessage({ kind: "error", text: "请输入 Python 源码路径。" });
      return;
    }
    const requestVersion = ++sourceRequestVersionRef.current;
    setSourceBusyAction("read");
    try {
      const response = await readPythonArtSource(baseUrl, scriptSourcePath.trim());
      if (requestVersion !== sourceRequestVersionRef.current) return;
      setScriptSourcePath(response.path);
      setScriptSourceCode(response.content);
      const baseName = basenameWithoutExtension(response.path);
      if (!repositoryName.trim()) setRepositoryName(normalizeToolId(baseName));
      if (!name.trim()) setName(baseName);
      let configured = false;
      try {
        const nearby = await checkPythonArtJsonNearby(baseUrl, response.path);
        if (requestVersion !== sourceRequestVersionRef.current) return;
        if (nearby.found && nearby.artJson) {
          setScriptArtJsonPath(nearby.artJsonPath || "");
          applyArtJson(nearby.artJson, response.path);
          configured = true;
        }
      } catch {
        configured = false;
      }
      if (!configured) {
        const inferred = await inferPythonArtPorts(baseUrl, { path: response.path });
        if (requestVersion !== sourceRequestVersionRef.current) return;
        applyPythonPorts({
          inputs: (inferred.inputs || []).map(normalizePythonPort),
          outputs: (inferred.outputs || []).map(normalizePythonPort),
        });
      }
      setWizardMessage({ kind: "info", text: configured ? "已读取源码和 art.json。" : "已读取源码并推断端口。" });
    } catch (error) {
      if (requestVersion === sourceRequestVersionRef.current) {
        setWizardMessage({ kind: "error", text: error instanceof Error ? error.message : "无法读取 Python 源码。" });
      }
    } finally {
      if (requestVersion === sourceRequestVersionRef.current) setSourceBusyAction(null);
    }
  };

  const readScriptArtJson = async () => {
    if (!scriptArtJsonPath.trim()) {
      setWizardMessage({ kind: "error", text: "请输入 art.json 路径或 Art 目录。" });
      return;
    }
    const requestVersion = ++sourceRequestVersionRef.current;
    setSourceBusyAction("art-json");
    try {
      const response = await readPythonArtJson(baseUrl, scriptArtJsonPath.trim());
      if (requestVersion !== sourceRequestVersionRef.current) return;
      setScriptArtJsonPath(response.artJsonPath || scriptArtJsonPath);
      applyArtJson(response.artJson, scriptSourcePath);
      setWizardMessage({ kind: "info", text: "已读取 art.json。" });
    } catch (error) {
      if (requestVersion === sourceRequestVersionRef.current) {
        setWizardMessage({ kind: "error", text: error instanceof Error ? error.message : "无法读取 art.json。" });
      }
    } finally {
      if (requestVersion === sourceRequestVersionRef.current) setSourceBusyAction(null);
    }
  };

  const inferScriptPorts = async () => {
    if (!scriptSourcePath.trim() && !scriptSourceCode.trim()) {
      setWizardMessage({ kind: "error", text: "请输入源码路径或源码内容。" });
      return;
    }
    const requestVersion = ++sourceRequestVersionRef.current;
    setSourceBusyAction("infer");
    try {
      const response = await inferPythonArtPorts(baseUrl, {
        path: scriptSourcePath.trim() || undefined,
        code: scriptSourcePath.trim() ? undefined : scriptSourceCode,
      });
      if (requestVersion !== sourceRequestVersionRef.current) return;
      applyPythonPorts({
        inputs: (response.inputs || []).map(normalizePythonPort),
        outputs: (response.outputs || []).map(normalizePythonPort),
      });
      setWizardMessage({ kind: "info", text: "已更新端口。" });
    } catch (error) {
      if (requestVersion !== sourceRequestVersionRef.current) return;
      const fallback = inferPortsFromPythonCode(scriptSourceCode);
      if (fallback.inputs.length || fallback.outputs.length) {
        applyPythonPorts(fallback);
        setWizardMessage({ kind: "info", text: "已从源码更新端口。" });
      } else {
        setWizardMessage({ kind: "error", text: error instanceof Error ? error.message : "无法推断端口。" });
      }
    } finally {
      if (requestVersion === sourceRequestVersionRef.current) setSourceBusyAction(null);
    }
  };

  const importRawCommand = () => {
    const parsed = parseRawCommand(rawCommandText);
    if (!parsed) {
      setWizardMessage({ kind: "error", text: "请输入命令。" });
      return;
    }
    setCommand(parsed.command);
    setArgsText(parsed.argsText);
    const parsedInputs = parsed.ports.filter((port) => port.isInput).map((port) => portDraftFromParsedPort(port, "input"));
    const parsedOutputs = parsed.ports.filter((port) => !port.isInput).map((port) => portDraftFromParsedPort(port, "output"));
    if (parsedInputs.length) setInputPorts(parsedInputs);
    if (parsedOutputs.length) setOutputPorts(parsedOutputs);
    setWizardMessage({ kind: "info", text: "已解析命令。" });
  };

  const importCloudSmartTemplate = () => {
    const parsed = parseCurlCommand(cloudCurlText);
    if (!parsed) {
      setWizardMessage({ kind: "error", text: "请输入有效的 cURL 命令。" });
      return;
    }
    setEndpoint(parsed.url || endpoint);
    setMethod(parsed.method || "POST");
    if (Object.keys(parsed.headers).length) setHeadersText(JSON.stringify(parsed.headers, null, 2));
    const nextContentType = parsed.headers["Content-Type"] || parsed.headers["content-type"];
    if (nextContentType) setContentType(nextContentType);
    if (parsed.body) setBodyText(parsed.body);
    if (parsed.suggestedInputs.length) setInputPorts(parsed.suggestedInputs.map((port) => portDraftFromParsedPort(port, "input")));
    const responsePreview = autoTemplateResponse(cloudResponseText);
    if (responsePreview.ports.length) setOutputPorts(responsePreview.ports.map((port) => portDraftFromParsedPort(port, "output")));
    setWizardMessage({ kind: "info", text: "已导入请求和响应结构。" });
  };

  const discoverMcpTools = async () => {
    const server = mcpServers.find((item) => item.id === mcpServerId);
    if (!server) {
      setWizardMessage({ kind: "error", text: "请选择 MCP 服务。" });
      return;
    }
    const requestVersion = ++mcpDiscoveryVersionRef.current;
    setMcpDiscoveryBusy(true);
    try {
      const result = await testMcpConnection(baseUrl, server);
      if (requestVersion !== mcpDiscoveryVersionRef.current) return;
      const tools = Array.isArray(result.tools) ? result.tools : [];
      setMcpTools(tools);
      const firstToolName = tools[0] ? mcpToolLabel(tools[0]) : "";
      setSelectedMcpSchemaToolName(firstToolName);
      if (firstToolName && !mcpToolName.trim()) setMcpToolName(firstToolName);
      setWizardMessage({
        kind: result.success === false ? "error" : "info",
        text: result.success === false ? result.error || "MCP 连接失败。" : `发现 ${tools.length} 个工具。`,
      });
    } catch (error) {
      if (requestVersion === mcpDiscoveryVersionRef.current) {
        setWizardMessage({ kind: "error", text: error instanceof Error ? error.message : "无法发现 MCP 工具。" });
      }
    } finally {
      if (requestVersion === mcpDiscoveryVersionRef.current) setMcpDiscoveryBusy(false);
    }
  };

  const useSelectedMcpToolSchema = () => {
    const tool = mcpTools.find((item) => mcpToolLabel(item) === selectedMcpSchemaToolName);
    const parsed = portsFromMcpToolSchema(tool);
    if (!parsed) {
      setWizardMessage({ kind: "error", text: "该工具没有可用的 input_schema。" });
      return;
    }
    setMcpToolName(parsed.toolName);
    if (parsed.suggestedInputs.length) setInputPorts(parsed.suggestedInputs.map((port) => portDraftFromParsedPort(port, "input")));
    if (parsed.suggestedOutputs.length) setOutputPorts(parsed.suggestedOutputs.map((port) => portDraftFromParsedPort(port, "output")));
    setWizardMessage({ kind: "info", text: "已导入 MCP 工具结构。" });
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
    const output = option.outputs.find((candidate) => (
      workflowPreviewOutput?.nodeId === option.nodeId
      && workflowPreviewOutput.output === candidate.name
    )) ?? option.outputs[0];
    if (!output) return;
    setWorkflowPreviewOutput({ nodeId: option.nodeId, output: output.name, kind: "node_result" });
    setWorkflowNodeRequired(option.nodeId, true);
  };

  const submit = async () => {
    const values = { ...frameworkValues };
    if (mode === "cloud_api") Object.assign(values, { endpoint, method, headers: headersText, body: bodyText });
    if (mode === "mcp") Object.assign(values, { serverId: mcpServerId, toolName: mcpToolName, arguments: mcpArgumentsText });
    if (mode === "workflow") Object.assign(values, { workflowId });
    if (mode === "process") Object.assign(values, {
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

  return { additionalAuthoringFields, argsText, bodyText, cloudCurlText, cloudResponseText, command, contentType, credentialNames, description, discoverMcpTools, endpoint, frameworkValues, headersText, importCloudSmartTemplate, importRawCommand, inferScriptPorts, inputPorts, mcpArgumentsText, mcpDiscoveryBusy, mcpServerId, mcpToolLabel, mcpToolName, mcpTools, method, mode, name, outputPorts, paramPorts, rawCommandText, readScriptArtJson, readScriptSource, repositoryName, scriptArtJsonPath, scriptEntryKind, scriptSourceCode, scriptSourceDirectory, scriptSourcePath, selectWorkflowPreviewNode, selectedFrameworkReady, selectedMcpSchemaToolName, selectedMode, setArgsText, setBodyText, setCloudCurlText, setCloudResponseText, setCommand, setContentType, setDescription, setEndpoint, setFrameworkValues, setHeadersText, setInputPorts, setMcpArgumentsText, setMcpServerId, setMcpToolName, setMethod, setMode, setName, setOutputPorts, setParamPorts, setRawCommandText, setRepositoryName, setScriptArtJsonPath, setScriptEntryKind, setScriptSourceCode, setScriptSourceDirectory, setScriptSourcePath, setSelectedMcpSchemaToolName, setWorkflowGraph, setWorkflowId, setWorkflowNodeRequired, setWorkflowPreviewOutput, setWorkflowPreviewRequiredNodes, sourceBusyAction, submit, useSelectedMcpToolSchema, wizardMessage, workflowId, workflowInterfaceAppliedRef, workflowInterfaceError, workflowInterfaceLoading, workflowParamCandidates, workflowPreviewNodeOptions, workflowPreviewOutput, workflowPreviewRequiredNodes };
}
