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
import { useUiText } from "../../i18n/index.ts";
import type { LoomFramework, LoomMcpServer, LoomToolDefinition, LoomWorkflowMetadata } from "../../services/loomApi";
import { type ArtWizardMode, artWizardModes } from "../app/appShell";
import { ArtPortEditor, FrameworkAuthoringFieldInput } from "./ArtWizardFields";
import type { ArtCreationRequest, ArtWizardSubmitDraft } from "./artWizardModel";
import { CloudArtRequestFields } from "./CloudArtRequestFields";
import { useAddArtWizardController } from "./useAddArtWizardController";
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
  const { t, tf } = useUiText();
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
      aria-label={t("art.action.createArt")}
    >
      <TabList label={t("art.wizard.modeTabsAria")} className="loom-art-mode-tabs">
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
            <Field label={t("art.wizard.field.repositoryName")}>
              <Input
                value={repositoryName}
                onChange={(event) => setRepositoryName(event.target.value)}
                placeholder={`${mode}-my-art`}
              />
            </Field>
            <Field label={t("art.wizard.field.artName")}>
              <Input value={name} onChange={(event) => setName(event.target.value)} placeholder={selectedMode.title} />
            </Field>
            <div className="art-creator-identity__description">
              <Field label={t("art.edit.field.description")}>
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
                <Field label={t("art.wizard.mcp.service")}>
                  <Select value={mcpServerId} onChange={(event) => setMcpServerId(event.target.value)}>
                    <option value="">{t("art.wizard.mcp.selectService")}</option>
                    {mcpServers.map((server) => (
                      <option key={server.id} value={server.id}>
                        {server.name || server.id}
                      </option>
                    ))}
                  </Select>
                </Field>
                <Field label={t("art.wizard.mcp.tool")}>
                  <Input
                    value={mcpToolName}
                    onChange={(event) => setMcpToolName(event.target.value)}
                    placeholder="search"
                  />
                </Field>
                <div className="art-creator-field--wide">
                  <Field label={t("art.wizard.mcp.defaultArguments")}>
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
                    {t("art.wizard.mcp.discoverTools")}
                  </Button>
                  <Select
                    value={selectedMcpSchemaToolName}
                    onChange={(event) => setSelectedMcpSchemaToolName(event.target.value)}
                    disabled={!mcpTools.length}
                    aria-label={t("art.wizard.mcp.discoveredToolsAria")}
                  >
                    <option value="">{t("art.wizard.mcp.selectDiscoveredTool")}</option>
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
                    {t("art.wizard.mcp.useSchema")}
                  </Button>
                </div>
              </div>
            ) : null}

            {mode === "process" ? (
              <div className="art-script-creator">
                <SegmentedControl
                  label={t("art.wizard.script.entryAria")}
                  hideLabel
                  name="art-script-entry"
                  value={scriptEntryKind}
                  onValueChange={setScriptEntryKind}
                  options={[
                    { value: "python", label: t("art.wizard.script.entryPython") },
                    { value: "command", label: t("art.wizard.script.entryCommand") },
                  ]}
                />
                {scriptEntryKind === "python" ? (
                  <div className="art-creator-fields">
                    <div className="art-creator-field--wide">
                      <div className="art-creator-path-row">
                        <Field label={t("art.wizard.script.sourceFile")}>
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
                          {t("art.wizard.script.read")}
                        </Button>
                      </div>
                    </div>
                    <div className="art-creator-field--wide">
                      <div className="art-creator-path-row">
                        <Field label={t("art.wizard.script.artJsonOptional")}>
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
                          {t("art.wizard.script.read")}
                        </Button>
                      </div>
                    </div>
                    <div className="art-creator-field--full">
                      <Field label={t("art.wizard.script.sourceCode")}>
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
                        {t("art.wizard.script.inferPorts")}
                      </Button>
                    </div>
                  </div>
                ) : (
                  <div className="art-creator-fields">
                    <div className="art-creator-field--wide">
                      <Field label={t("art.wizard.script.resourceDirOptional")}>
                        <Input
                          value={scriptSourceDirectory}
                          onChange={(event) => setScriptSourceDirectory(event.target.value)}
                          placeholder="C:\\path\\to\\package"
                        />
                      </Field>
                    </div>
                    <Field label={t("art.wizard.script.runtimeCommand")}>
                      <Input
                        value={command}
                        onChange={(event) => setCommand(event.target.value)}
                        placeholder="runtime/source/tool.exe"
                      />
                    </Field>
                    <Field label={t("art.wizard.script.argsPerLine")}>
                      <Textarea compact value={argsText} onChange={(event) => setArgsText(event.target.value)} />
                    </Field>
                    <div className="art-creator-field--wide">
                      <div className="art-creator-path-row">
                        <Field label={t("art.wizard.script.commandLine")}>
                          <Input
                            value={rawCommandText}
                            onChange={(event) => setRawCommandText(event.target.value)}
                            placeholder="ffmpeg -i {{inputs.image.path}} {{outputs.result.path}}"
                          />
                        </Field>
                        <Button variant="secondary" type="button" onClick={importRawCommand}>
                          {t("art.wizard.script.parse")}
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
                  <Field label={t("art.wizard.workflow.field")}>
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
                      <option value="">{t("art.wizard.workflow.selectSaved")}</option>
                      {workflows.map((workflow) => (
                        <option key={workflow.id} value={workflow.id}>
                          {workflow.name || workflow.id}
                        </option>
                      ))}
                    </Select>
                  </Field>
                </div>
                {workflowInterfaceLoading ? <Notice>{t("art.wizard.workflow.loadingInterface")}</Notice> : null}
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
                {t("art.wizard.ports.title")}{" "}
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
          {!selectedFrameworkReady ? (
            <Notice tone="warning">{tf("art.wizard.frameworkNotReady", { title: selectedMode.title })}</Notice>
          ) : null}
        </div>
        <div className="art-creator-panel__footer">
          <Button
            variant="primary"
            loading={busy}
            type="submit"
            disabled={busy || Boolean(sourceBusyAction) || !selectedFrameworkReady}
          >
            {t("art.action.createArt")}
          </Button>
        </div>
      </form>
    </TabsRoot>
  );
}
