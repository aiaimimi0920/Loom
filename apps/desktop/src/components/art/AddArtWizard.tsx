// Coordinates the multi-mode Art authoring wizard.
import {
  Button,
  Disclosure,
  Field,
  Input,
  Notice,
  SegmentedControl,
  Select,
  Tab,
  TabList,
  TabPanel,
  TabsRoot,
  Textarea,
} from "@neuro/ui";
import type { LoomFramework, LoomMcpServer, LoomToolDefinition, LoomWorkflowMetadata } from "../../services/loomApi";
import { type ArtWizardMode, artWizardModes } from "../app/appShell";
import { ArtPortEditor, FrameworkAuthoringFieldInput } from "./ArtWizardFields";
import type { ArtCreationRequest, ArtWizardSubmitDraft } from "./artWizardModel";
import { useAddArtWizardController } from "./useAddArtWizardController";
import { CloudArtRequestFields } from "./CloudArtRequestFields";
import { WorkflowArtPreviewPolicy } from "./WorkflowArtPreviewPolicy";

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
  const controller = useAddArtWizardController({
    baseUrl,
    frameworks,
    mcpServers,
    workflows,
    tools,
    initialRequest,
    busy,
    onCreate,
  });
  const {
    additionalAuthoringFields,
    argsText,
    command,
    credentialNames,
    description,
    discoverMcpTools,
    frameworkValues,
    importRawCommand,
    inferScriptPorts,
    inputPorts,
    mcpArgumentsText,
    mcpDiscoveryBusy,
    mcpServerId,
    mcpToolLabel,
    mcpToolName,
    mcpTools,
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
    setCommand,
    setDescription,
    setFrameworkValues,
    setInputPorts,
    setMcpArgumentsText,
    setMcpServerId,
    setMcpToolName,
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
  } = controller;

  return (
    <TabsRoot
      className="add-art-wizard"
      value={mode}
      onValueChange={(value) => setMode(value as ArtWizardMode)}
      aria-label="创建 Art"
    >
      <TabList label="Art 类型" className="loom-art-mode-tabs">
        {artWizardModes.map((item) => (
          <Tab key={item.id} value={item.id}>
            {item.title}
          </Tab>
        ))}
      </TabList>

      <form
        className="art-creator-panel"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <div className="art-creator-panel__scroll">
          <div className="art-creator-identity">
            <Field label="仓库名称">
              <Input
                value={repositoryName}
                onChange={(event) => setRepositoryName(event.target.value)}
                placeholder={`${mode}-my-art`}
              />
            </Field>
            <Field label="Art 名称">
              <Input value={name} onChange={(event) => setName(event.target.value)} placeholder={selectedMode.title} />
            </Field>
            <div className="art-creator-identity__description">
              <Field label="描述">
                <Input
                  value={description}
                  onChange={(event) => setDescription(event.target.value)}
                  placeholder={selectedMode.subtitle}
                />
              </Field>
            </div>
          </div>

          <TabPanel className="art-creator-config" value={mode}>
            {mode === "cloud_api" ? <CloudArtRequestFields controller={controller} /> : null}

            {mode === "mcp" ? (
              <div className="art-creator-fields">
                <Field label="MCP 服务">
                  <Select value={mcpServerId} onChange={(event) => setMcpServerId(event.target.value)}>
                    <option value="">选择服务</option>
                    {mcpServers.map((server) => (
                      <option key={server.id} value={server.id}>
                        {server.name || server.id}
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field label="MCP 工具">
                  <Input
                    value={mcpToolName}
                    onChange={(event) => setMcpToolName(event.target.value)}
                    placeholder="search"
                  />
                </Field>
                <div className="art-creator-field--wide">
                  <Field label="默认参数">
                    <Textarea
                      compact
                      value={mcpArgumentsText}
                      onChange={(event) => setMcpArgumentsText(event.target.value)}
                      spellCheck={false}
                    />
                  </Field>
                </div>
                <div className="art-creator-inline-action">
                  <Button
                    variant="secondary"
                    type="button"
                    onClick={discoverMcpTools}
                    loading={mcpDiscoveryBusy}
                    disabled={mcpDiscoveryBusy}
                  >
                    {"发现工具"}
                  </Button>
                  <Select
                    value={selectedMcpSchemaToolName}
                    onChange={(event) => setSelectedMcpSchemaToolName(event.target.value)}
                    disabled={!mcpTools.length}
                    aria-label="已发现的 MCP 工具"
                  >
                    <option value="">选择已发现工具</option>
                    {mcpTools.map((tool, index) => {
                      const label = mcpToolLabel(tool);
                      return (
                        <option key={`${label}-${index}`} value={label}>
                          {label}
                        </option>
                      );
                    })}
                  </Select>
                  <Button
                    variant="secondary"
                    type="button"
                    onClick={useSelectedMcpToolSchema}
                    disabled={!selectedMcpSchemaToolName}
                  >
                    使用结构
                  </Button>
                </div>
              </div>
            ) : null}

            {mode === "process" ? (
              <div className="art-script-creator">
                <SegmentedControl
                  label="脚本入口"
                  hideLabel
                  name="art-script-entry"
                  value={scriptEntryKind}
                  onValueChange={setScriptEntryKind}
                  options={[
                    { value: "python", label: "Python" },
                    { value: "command", label: "命令" },
                  ]}
                />
                {scriptEntryKind === "python" ? (
                  <div className="art-creator-fields">
                    <div className="art-creator-field--wide">
                      <div className="art-creator-path-row">
                        <Field label="源码文件">
                          <Input
                            value={scriptSourcePath}
                            onChange={(event) => setScriptSourcePath(event.target.value)}
                            placeholder="C:\\path\\to\\main.py"
                          />
                        </Field>
                        <Button
                          variant="secondary"
                          type="button"
                          onClick={readScriptSource}
                          loading={sourceBusyAction === "read"}
                          disabled={Boolean(sourceBusyAction)}
                        >
                          {"读取"}
                        </Button>
                      </div>
                    </div>
                    <div className="art-creator-field--wide">
                      <div className="art-creator-path-row">
                        <Field label="art.json（可选）">
                          <Input
                            value={scriptArtJsonPath}
                            onChange={(event) => setScriptArtJsonPath(event.target.value)}
                            placeholder="C:\\path\\to\\art.json"
                          />
                        </Field>
                        <Button
                          variant="secondary"
                          type="button"
                          onClick={readScriptArtJson}
                          loading={sourceBusyAction === "art-json"}
                          disabled={Boolean(sourceBusyAction)}
                        >
                          {"读取"}
                        </Button>
                      </div>
                    </div>
                    <div className="art-creator-field--full">
                      <Field label="源码">
                        <Textarea
                          className="art-script-source"
                          value={scriptSourceCode}
                          onChange={(event) => setScriptSourceCode(event.target.value)}
                          placeholder="def run(args): ..."
                          spellCheck={false}
                        />
                      </Field>
                    </div>
                    <div className="art-creator-inline-action">
                      <Button
                        variant="secondary"
                        type="button"
                        onClick={inferScriptPorts}
                        loading={sourceBusyAction === "infer"}
                        disabled={Boolean(sourceBusyAction)}
                      >
                        {"推断端口"}
                      </Button>
                    </div>
                  </div>
                ) : (
                  <div className="art-creator-fields">
                    <div className="art-creator-field--wide">
                      <Field label="资源目录（可选）">
                        <Input
                          value={scriptSourceDirectory}
                          onChange={(event) => setScriptSourceDirectory(event.target.value)}
                          placeholder="C:\\path\\to\\package"
                        />
                      </Field>
                    </div>
                    <Field label="运行命令">
                      <Input
                        value={command}
                        onChange={(event) => setCommand(event.target.value)}
                        placeholder="runtime/source/tool.exe"
                      />
                    </Field>
                    <Field label="参数（每行一个）">
                      <Textarea compact value={argsText} onChange={(event) => setArgsText(event.target.value)} />
                    </Field>
                    <div className="art-creator-field--wide">
                      <div className="art-creator-path-row">
                        <Field label="命令行">
                          <Input
                            value={rawCommandText}
                            onChange={(event) => setRawCommandText(event.target.value)}
                            placeholder="ffmpeg -i {{inputs.image.path}} {{outputs.result.path}}"
                          />
                        </Field>
                        <Button variant="secondary" type="button" onClick={importRawCommand}>
                          解析
                        </Button>
                      </div>
                    </div>
                  </div>
                )}
              </div>
            ) : null}

            {mode === "workflow" ? (
              <div className="art-creator-fields">
                <div className="art-creator-field--wide">
                  <Field label="工作流">
                    <Select
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
                      {workflows.map((workflow) => (
                        <option key={workflow.id} value={workflow.id}>
                          {workflow.name || workflow.id}
                        </option>
                      ))}
                    </Select>
                  </Field>
                </div>
                {workflowInterfaceLoading ? <Notice>读取流程结构...</Notice> : null}
                {workflowInterfaceError ? <Notice tone="danger">{workflowInterfaceError}</Notice> : null}
                {workflowPreviewNodeOptions.length ? (
                  <WorkflowArtPreviewPolicy
                    options={workflowPreviewNodeOptions}
                    output={workflowPreviewOutput}
                    requiredNodes={workflowPreviewRequiredNodes}
                    onNodeRequired={setWorkflowNodeRequired}
                    onSelectNode={selectWorkflowPreviewNode}
                    onOutputChange={setWorkflowPreviewOutput}
                  />
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
          </TabPanel>

          <Disclosure
            className="art-creator-ports"
            title={
              <span className="art-creator-ports__title">
                端口{" "}
                <small>
                  {inputPorts.length} / {paramPorts.length} / {outputPorts.length}
                </small>
              </span>
            }
          >
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
          </Disclosure>

          {wizardMessage ? (
            <Notice tone={wizardMessage.kind === "error" ? "danger" : "success"}>{wizardMessage.text}</Notice>
          ) : null}
          {!selectedFrameworkReady ? <Notice tone="warning">{selectedMode.title}未安装或未就绪。</Notice> : null}
        </div>
        <div className="art-creator-panel__footer">
          <Button
            variant="primary"
            loading={busy}
            type="submit"
            disabled={busy || Boolean(sourceBusyAction) || !selectedFrameworkReady}
          >
            {"创建 Art"}
          </Button>
        </div>
      </form>
    </TabsRoot>
  );
}
