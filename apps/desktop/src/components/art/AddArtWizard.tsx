// Coordinates the multi-mode Art authoring wizard.
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
import { ArtPortEditor, FrameworkAuthoringFieldInput } from "./ArtWizardFields";
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
import { useAddArtWizardController } from "./useAddArtWizardController";

export function AddArtWizard({
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
  const {
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
  } = useAddArtWizardController({ baseUrl, frameworks, mcpServers, workflows, tools, initialRequest, busy, onCreate });

  return (
    <section className="add-art-wizard" aria-label="创建 Art">
      <div className="art-mode-grid" role="tablist" aria-label="Art 类型">
        {artWizardModes.map((item) => (
          <button
            className={mode === item.id ? "art-mode-card art-mode-card--active" : "art-mode-card"}
            type="button"
            role="tab"
            aria-selected={mode === item.id}
            key={item.id}
            onClick={() => setMode(item.id)}
          >
            {item.title}
          </button>
        ))}
      </div>

      <form className="art-creator-panel" onSubmit={(event) => { event.preventDefault(); void submit(); }}>
        <div className="art-creator-identity">
          <label className="field-label">
            仓库名称
            <input className="studio-input" value={repositoryName} onChange={(event) => setRepositoryName(event.target.value)} placeholder={`${mode}-my-art`} />
          </label>
          <label className="field-label">
            Art 名称
            <input className="studio-input" value={name} onChange={(event) => setName(event.target.value)} placeholder={selectedMode.title} />
          </label>
          <label className="field-label art-creator-identity__description">
            描述
            <input className="studio-input" value={description} onChange={(event) => setDescription(event.target.value)} placeholder={selectedMode.subtitle} />
          </label>
        </div>

        <div className="art-creator-config" role="tabpanel">
          {mode === "cloud_api" ? (
            <div className="art-creator-fields">
              <label className="field-label art-creator-field--wide">
                Endpoint
                <input className="studio-input" value={endpoint} onChange={(event) => setEndpoint(event.target.value)} placeholder="https://api.example.com/v1/process" />
              </label>
              <label className="field-label">
                方法
                <select className="studio-input" value={method} onChange={(event) => setMethod(event.target.value)}>
                  <option value="POST">POST</option>
                  <option value="PUT">PUT</option>
                  <option value="PATCH">PATCH</option>
                  <option value="GET">GET</option>
                  <option value="DELETE">DELETE</option>
                </select>
              </label>
              <label className="field-label">
                Content-Type
                <input className="studio-input" value={contentType} onChange={(event) => setContentType(event.target.value)} />
              </label>
              <label className="field-label">
                Headers
                <textarea className="studio-textarea studio-textarea--compact" value={headersText} onChange={(event) => setHeadersText(event.target.value)} spellCheck={false} />
              </label>
              <label className="field-label">
                Body
                <textarea className="studio-textarea studio-textarea--compact" value={bodyText} onChange={(event) => setBodyText(event.target.value)} spellCheck={false} />
              </label>
              <label className="field-label">
                cURL
                <textarea className="studio-textarea studio-textarea--compact" value={cloudCurlText} onChange={(event) => setCloudCurlText(event.target.value)} spellCheck={false} />
              </label>
              <label className="field-label">
                响应示例
                <textarea className="studio-textarea studio-textarea--compact" value={cloudResponseText} onChange={(event) => setCloudResponseText(event.target.value)} spellCheck={false} />
              </label>
              <div className="art-creator-inline-action">
                <button className="ghost-button" type="button" onClick={importCloudSmartTemplate}>导入 cURL</button>
              </div>
            </div>
          ) : null}

          {mode === "mcp" ? (
            <div className="art-creator-fields">
              <label className="field-label">
                MCP 服务
                <select className="studio-input" value={mcpServerId} onChange={(event) => setMcpServerId(event.target.value)}>
                  <option value="">选择服务</option>
                  {mcpServers.map((server) => <option key={server.id} value={server.id}>{server.name || server.id}</option>)}
                </select>
              </label>
              <label className="field-label">
                MCP 工具
                <input className="studio-input" value={mcpToolName} onChange={(event) => setMcpToolName(event.target.value)} placeholder="search" />
              </label>
              <label className="field-label art-creator-field--wide">
                默认参数
                <textarea className="studio-textarea studio-textarea--compact" value={mcpArgumentsText} onChange={(event) => setMcpArgumentsText(event.target.value)} spellCheck={false} />
              </label>
              <div className="art-creator-inline-action">
                <button className="ghost-button" type="button" onClick={discoverMcpTools} disabled={mcpDiscoveryBusy}>
                  {mcpDiscoveryBusy ? "发现中" : "发现工具"}
                </button>
                <select className="studio-input" value={selectedMcpSchemaToolName} onChange={(event) => setSelectedMcpSchemaToolName(event.target.value)} disabled={!mcpTools.length} aria-label="已发现的 MCP 工具">
                  <option value="">选择已发现工具</option>
                  {mcpTools.map((tool, index) => {
                    const label = mcpToolLabel(tool);
                    return <option key={`${label}-${index}`} value={label}>{label}</option>;
                  })}
                </select>
                <button className="ghost-button" type="button" onClick={useSelectedMcpToolSchema} disabled={!selectedMcpSchemaToolName}>使用结构</button>
              </div>
            </div>
          ) : null}

          {mode === "process" ? (
            <div className="art-script-creator">
              <div className="art-script-kind" role="group" aria-label="脚本入口">
                <button className={scriptEntryKind === "python" ? "art-script-kind__button art-script-kind__button--active" : "art-script-kind__button"} type="button" onClick={() => setScriptEntryKind("python")}>Python</button>
                <button className={scriptEntryKind === "command" ? "art-script-kind__button art-script-kind__button--active" : "art-script-kind__button"} type="button" onClick={() => setScriptEntryKind("command")}>命令</button>
              </div>
              {scriptEntryKind === "python" ? (
                <div className="art-creator-fields">
                  <label className="field-label art-creator-field--wide">
                    源码文件
                    <div className="art-creator-path-row">
                      <input className="studio-input" value={scriptSourcePath} onChange={(event) => setScriptSourcePath(event.target.value)} placeholder="C:\\path\\to\\main.py" />
                      <button className="ghost-button" type="button" onClick={readScriptSource} disabled={Boolean(sourceBusyAction)}>{sourceBusyAction === "read" ? "读取中" : "读取"}</button>
                    </div>
                  </label>
                  <label className="field-label art-creator-field--wide">
                    art.json（可选）
                    <div className="art-creator-path-row">
                      <input className="studio-input" value={scriptArtJsonPath} onChange={(event) => setScriptArtJsonPath(event.target.value)} placeholder="C:\\path\\to\\art.json" />
                      <button className="ghost-button" type="button" onClick={readScriptArtJson} disabled={Boolean(sourceBusyAction)}>{sourceBusyAction === "art-json" ? "读取中" : "读取"}</button>
                    </div>
                  </label>
                  <label className="field-label art-creator-field--full">
                    源码
                    <textarea className="studio-textarea art-script-source" value={scriptSourceCode} onChange={(event) => setScriptSourceCode(event.target.value)} placeholder="def run(args): ..." spellCheck={false} />
                  </label>
                  <div className="art-creator-inline-action">
                    <button className="ghost-button" type="button" onClick={inferScriptPorts} disabled={Boolean(sourceBusyAction)}>{sourceBusyAction === "infer" ? "推断中" : "推断端口"}</button>
                  </div>
                </div>
              ) : (
                <div className="art-creator-fields">
                  <label className="field-label art-creator-field--wide">
                    资源目录（可选）
                    <input className="studio-input" value={scriptSourceDirectory} onChange={(event) => setScriptSourceDirectory(event.target.value)} placeholder="C:\\path\\to\\package" />
                  </label>
                  <label className="field-label">
                    运行命令
                    <input className="studio-input" value={command} onChange={(event) => setCommand(event.target.value)} placeholder="runtime/source/tool.exe" />
                  </label>
                  <label className="field-label">
                    参数（每行一个）
                    <textarea className="studio-textarea studio-textarea--compact" value={argsText} onChange={(event) => setArgsText(event.target.value)} />
                  </label>
                  <label className="field-label art-creator-field--wide">
                    命令行
                    <div className="art-creator-path-row">
                      <input className="studio-input" value={rawCommandText} onChange={(event) => setRawCommandText(event.target.value)} placeholder="ffmpeg -i {{inputs.image.path}} {{outputs.result.path}}" />
                      <button className="ghost-button" type="button" onClick={importRawCommand}>解析</button>
                    </div>
                  </label>
                </div>
              )}
            </div>
          ) : null}

          {mode === "workflow" ? (
            <div className="art-creator-fields">
              <label className="field-label art-creator-field--wide">
                工作流
                <select
                  className="studio-input"
                  value={workflowId}
                  onChange={(event) => {
                    workflowInterfaceAppliedRef.current = "";
                    setWorkflowGraph(null);
                    setWorkflowPreviewOutput(undefined);
                    setWorkflowPreviewRequiredNodes([]);
                    setWorkflowId(event.target.value);
                  }}
                >
                  <option value="">选择已保存工作流</option>
                  {workflows.map((workflow) => <option key={workflow.id} value={workflow.id}>{workflow.name || workflow.id}</option>)}
                </select>
              </label>
              {workflowInterfaceLoading ? <small className="art-workflow-interface-state">读取流程结构...</small> : null}
              {workflowInterfaceError ? <small className="art-workflow-interface-state art-workflow-interface-state--error">{workflowInterfaceError}</small> : null}
              {workflowPreviewNodeOptions.length ? (
                <div className="art-workflow-preview-policy" aria-label="流程预览策略">
                  <div className="art-workflow-preview-policy__head" aria-hidden="true">
                    <span>节点</span>
                    <span>必要</span>
                    <span>预览输出</span>
                  </div>
                  <div className="art-workflow-preview-policy__list">
                    {workflowPreviewNodeOptions.map((option) => {
                      const selected = workflowPreviewOutput?.nodeId === option.nodeId;
                      const required = selected || workflowPreviewRequiredNodes.includes(option.nodeId);
                      return (
                        <div className="art-workflow-preview-policy__row" key={option.nodeId}>
                          <span className="art-workflow-preview-policy__name" title={`${option.label} · ${option.nodeId}`}>
                            {option.label}
                          </span>
                          <label className="art-workflow-preview-policy__toggle" title="预览发布前必须完成">
                            <input
                              type="checkbox"
                              aria-label={`${option.label} 必要`}
                              checked={required}
                              disabled={selected}
                              onChange={(event) => setWorkflowNodeRequired(option.nodeId, event.target.checked)}
                            />
                          </label>
                          <div className="art-workflow-preview-policy__output">
                            <label className="art-workflow-preview-policy__toggle" title="作为节点贴图预览">
                              <input
                                type="radio"
                                aria-label={`${option.label} 预览输出`}
                                name="workflow-preview-output"
                                checked={selected}
                                disabled={!option.outputs.length}
                                onChange={() => selectWorkflowPreviewNode(option)}
                              />
                            </label>
                            {selected && option.outputs.length > 1 ? (
                              <select
                                className="studio-input art-workflow-preview-policy__select"
                                aria-label={`${option.label} 预览端口`}
                                value={workflowPreviewOutput.output}
                                onChange={(event) => setWorkflowPreviewOutput({
                                  nodeId: option.nodeId,
                                  output: event.target.value,
                                  kind: "node_result",
                                })}
                              >
                                {option.outputs.map((output) => (
                                  <option key={output.name} value={output.name}>{output.label}</option>
                                ))}
                              </select>
                            ) : null}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </div>
              ) : null}
            </div>
          ) : null}

          {additionalAuthoringFields.length ? (
            <div className="art-creator-fields art-creator-fields--additional">
              {additionalAuthoringFields.map((field) => (
                <FrameworkAuthoringFieldInput
                  key={field.id}
                  field={field}
                  value={frameworkValues[field.id]}
                  credentialNames={credentialNames}
                  onChange={(value) => setFrameworkValues((current) => ({ ...current, [field.id]: value }))}
                />
              ))}
            </div>
          ) : null}
        </div>

        <details className="art-creator-ports">
          <summary>
            <span>端口</span>
            <small>{inputPorts.length} / {paramPorts.length} / {outputPorts.length}</small>
          </summary>
          <ArtPortEditor
            inputPorts={inputPorts}
            paramPorts={paramPorts}
            outputPorts={outputPorts}
            workflowMode={mode === "workflow"}
            paramBindingCandidates={workflowParamCandidates}
            paramBindingsLoading={workflowInterfaceLoading}
            setInputPorts={setInputPorts}
            setParamPorts={setParamPorts}
            setOutputPorts={setOutputPorts}
          />
        </details>

        {wizardMessage ? <p className={wizardMessage.kind === "error" ? "error-text" : "success-text"}>{wizardMessage.text}</p> : null}
        {!selectedFrameworkReady ? <p className="error-text">{selectedMode.title}未安装或未就绪。</p> : null}

        <div className="art-creator-panel__footer">
          <button className="signal-button" type="submit" disabled={busy || Boolean(sourceBusyAction) || !selectedFrameworkReady}>
            {busy ? "创建中" : "创建 Art"}
          </button>
        </div>
      </form>
    </section>
  );
}
