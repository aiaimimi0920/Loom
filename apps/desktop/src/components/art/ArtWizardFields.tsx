// Reusable framework and port editors for Art authoring.
import { Button, Checkbox, Field, Input, Panel, Select, Textarea } from "@neuro/ui";
import { type Dispatch, type SetStateAction, useMemo } from "react";
import { uiTextFor, useUiText } from "../../i18n/index.ts";
import { type LoomFrameworkAuthoringField } from "../../services/loomApi";
import { type WorkflowParamBindingCandidate } from "../../services/workflowStudio";
import {
  ArtPortCaptureMode,
  ArtWizardPortDraft,
  createPortDraft,
  defaultExecutionTypeForPort,
  defaultWidgetForParam,
  outputCaptureModes,
} from "./artWizardModel";

export function FrameworkAuthoringFieldInput({
  field,
  value,
  credentialNames,
  onChange,
}: {
  field: LoomFrameworkAuthoringField;
  value: unknown;
  credentialNames: string[];
  onChange: (value: unknown) => void;
}) {
  const { t } = useUiText();
  if (field.secret || field.type === "secret") {
    return (
      <Field label={`${field.label}${field.required ? " *" : ""}`}>
        <Select
          compact
          value={String(value ?? "")}
          onChange={(event) => onChange(event.target.value)}
          required={field.required}
        >
          <option value="">{t("art.edit.param.selectGlobalSecret")}</option>
          {credentialNames.map((credential) => (
            <option key={credential} value={credential}>
              {credential}
            </option>
          ))}
        </Select>
      </Field>
    );
  }
  if (field.type === "boolean") {
    return (
      <Checkbox
        checked={Boolean(value)}
        onChange={(event) => onChange(event.target.checked)}
        label={`${field.label}${field.required ? " *" : ""}`}
      />
    );
  }
  if (field.type === "enum") {
    return (
      <Field label={`${field.label}${field.required ? " *" : ""}`}>
        <Select compact value={String(value ?? "")} onChange={(event) => onChange(event.target.value)}>
          <option value="">{t("art.wizard.field.selectOption")}</option>
          {(field.options ?? []).map((option) => (
            <option key={`${field.id}-${String(option.value)}`} value={String(option.value)}>
              {option.label}
            </option>
          ))}
        </Select>
      </Field>
    );
  }
  if (field.type === "json") {
    return (
      <Field label={`${field.label}${field.required ? " *" : ""}`}>
        <Textarea
          compact
          value={typeof value === "string" ? value : JSON.stringify(value ?? {}, null, 2)}
          placeholder={field.placeholder || "{}"}
          onChange={(event) => onChange(event.target.value)}
        />
      </Field>
    );
  }
  return (
    <Field label={`${field.label}${field.required ? " *" : ""}`}>
      <Input
        compact
        type={field.type === "number" ? "number" : "text"}
        value={typeof value === "number" || typeof value === "string" ? value : ""}
        min={field.minimum ?? undefined}
        max={field.maximum ?? undefined}
        step={field.step ?? undefined}
        placeholder={field.placeholder ?? undefined}
        onChange={(event) => onChange(field.type === "number" ? Number(event.target.value) : event.target.value)}
      />
    </Field>
  );
}

export function ArtPortEditor({
  inputPorts,
  paramPorts,
  outputPorts,
  workflowMode,
  paramBindingCandidates,
  paramBindingsLoading,
  setInputPorts,
  setParamPorts,
  setOutputPorts,
}: {
  inputPorts: ArtWizardPortDraft[];
  paramPorts: ArtWizardPortDraft[];
  outputPorts: ArtWizardPortDraft[];
  workflowMode: boolean;
  paramBindingCandidates: WorkflowParamBindingCandidate[];
  paramBindingsLoading: boolean;
  setInputPorts: Dispatch<SetStateAction<ArtWizardPortDraft[]>>;
  setParamPorts: Dispatch<SetStateAction<ArtWizardPortDraft[]>>;
  setOutputPorts: Dispatch<SetStateAction<ArtWizardPortDraft[]>>;
}) {
  const { t, tf } = useUiText();
  const updateInputPort = (index: number, patch: Partial<ArtWizardPortDraft>) => {
    setInputPorts((ports) => ports.map((port, portIndex) => (portIndex === index ? { ...port, ...patch } : port)));
  };
  const updateOutputPort = (index: number, patch: Partial<ArtWizardPortDraft>) => {
    setOutputPorts((ports) => ports.map((port, portIndex) => (portIndex === index ? { ...port, ...patch } : port)));
  };
  const updateParamPort = (index: number, patch: Partial<ArtWizardPortDraft>) => {
    setParamPorts((ports) => ports.map((port, portIndex) => (portIndex === index ? { ...port, ...patch } : port)));
  };
  const bindingCandidatesByNode = useMemo(() => {
    const groups = new Map<string, { label: string; candidates: WorkflowParamBindingCandidate[] }>();
    for (const candidate of paramBindingCandidates) {
      const group = groups.get(candidate.nodeId) ?? {
        label: `${candidate.nodeLabel} · ${candidate.nodeId}`,
        candidates: [],
      };
      group.candidates.push(candidate);
      groups.set(candidate.nodeId, group);
    }
    return [...groups.entries()];
  }, [paramBindingCandidates]);
  const selectedBindingKey = (port: ArtWizardPortDraft) =>
    paramBindingCandidates.find(
      (candidate) => candidate.nodeId === port.bindingNodeId && candidate.target === port.bindingTarget,
    )?.key ?? "";
  const updateParamBinding = (index: number, key: string) => {
    const port = paramPorts[index];
    if (!port) return;
    const candidate = paramBindingCandidates.find((item) => item.key === key);
    if (!candidate) {
      updateParamPort(index, { bindingNodeId: "", bindingTarget: "", bindingKind: "" });
      return;
    }
    const trimmedLabel = port.label.trim();
    const defaultName = !port.name.trim() || port.name.trim() === "param";
    const defaultLabel =
      !trimmedLabel ||
      trimmedLabel === uiTextFor("en", "art.wizard.ports.defaultParamLabel") ||
      trimmedLabel === uiTextFor("zh-Hans", "art.wizard.ports.defaultParamLabel");
    updateParamPort(index, {
      bindingNodeId: candidate.nodeId,
      bindingTarget: candidate.target,
      bindingKind: "param",
      type: candidate.type,
      executionType: candidate.executionType,
      widget: candidate.widget || defaultWidgetForParam(candidate.type),
      dataType: candidate.dataType || "",
      min: candidate.min,
      max: candidate.max,
      step: candidate.step,
      options: candidate.options,
      multiline: candidate.multiline ?? false,
      group: candidate.group || "",
      required: candidate.required ?? false,
      secret: candidate.secret ?? false,
      ...(defaultName ? { name: candidate.target, id: candidate.target } : {}),
      ...(defaultLabel ? { label: candidate.paramLabel } : {}),
      ...(!port.defaultValue.trim() ? { defaultValue: candidate.defaultValue } : {}),
    });
  };
  const portTypeOptions = (
    <>
      <option value="image">{t("art.wizard.portType.image")}</option>
      <option value="file">{t("art.wizard.portType.file")}</option>
      <option value="string">{t("art.wizard.portType.string")}</option>
      <option value="int">{t("art.wizard.portType.int")}</option>
      <option value="float">{t("art.wizard.portType.float")}</option>
      <option value="boolean">{t("art.wizard.portType.boolean")}</option>
    </>
  );

  return (
    <div className="advanced-port-editor">
      <div className="port-editor-section">
        <div className="section-heading-row section-heading-row--compact">
          <h4>{t("art.wizard.ports.inputs")}</h4>
          <Button
            variant="quiet"
            compact
            type="button"
            onClick={() => setInputPorts((ports) => [...ports, createPortDraft("input")])}
          >
            {t("art.wizard.ports.add")}
          </Button>
        </div>
        {inputPorts.map((port, index) => (
          <Panel
            className="port-editor-row"
            key={`input-${index}`}
            aria-label={tf("art.wizard.ports.inputRowAria", { index: index + 1 })}
          >
            <Input
              aria-label={tf("art.wizard.ports.inputNameAria", { index: index + 1 })}
              compact
              value={port.name}
              onChange={(event) => updateInputPort(index, { name: event.target.value })}
              placeholder={t("art.wizard.ports.placeholder.name")}
            />
            <Input
              aria-label={tf("art.wizard.ports.inputLabelAria", { index: index + 1 })}
              compact
              value={port.label}
              onChange={(event) => updateInputPort(index, { label: event.target.value })}
              placeholder={t("art.wizard.ports.placeholder.label")}
            />
            <Select
              aria-label={tf("art.wizard.ports.inputTypeAria", { index: index + 1 })}
              compact
              value={port.type}
              onChange={(event) => {
                const nextType = event.target.value;
                updateInputPort(index, {
                  type: nextType,
                  executionType: defaultExecutionTypeForPort(nextType, "input"),
                });
              }}
            >
              {portTypeOptions}
            </Select>
            <Input
              aria-label={tf("art.wizard.ports.inputExecTypeAria", { index: index + 1 })}
              compact
              value={port.executionType}
              onChange={(event) => updateInputPort(index, { executionType: event.target.value })}
              placeholder={t("art.wizard.ports.placeholder.execType")}
            />
            <Input
              aria-label={tf("art.wizard.ports.inputDefaultAria", { index: index + 1 })}
              compact
              value={port.defaultValue}
              onChange={(event) => updateInputPort(index, { defaultValue: event.target.value })}
              placeholder={t("art.wizard.ports.placeholder.default")}
            />
            <Checkbox
              aria-label={tf("art.wizard.ports.inputDisableAria", { index: index + 1 })}
              checked={port.disabled}
              onChange={(event) => updateInputPort(index, { disabled: event.target.checked })}
              label={t("art.action.disable")}
            />
            <Button
              aria-label={tf("art.wizard.ports.inputDeleteAria", { index: index + 1 })}
              variant="quiet"
              compact
              type="button"
              onClick={() => setInputPorts((ports) => ports.filter((_, portIndex) => portIndex !== index))}
            >
              {t("art.action.delete")}
            </Button>
          </Panel>
        ))}
      </div>
      <div className="port-editor-section">
        <div className="section-heading-row section-heading-row--compact">
          <h4>{t("art.wizard.ports.params")}</h4>
          <Button
            variant="quiet"
            compact
            type="button"
            onClick={() =>
              setParamPorts((ports) => [
                ...ports,
                createPortDraft("input", {
                  name: "param",
                  label: t("art.wizard.ports.defaultParamLabel"),
                  type: "string",
                }),
              ])
            }
          >
            {t("art.wizard.ports.add")}
          </Button>
        </div>
        {paramPorts.map((port, index) => (
          <Panel
            className="port-editor-row"
            key={`param-${index}`}
            aria-label={tf("art.wizard.ports.paramRowAria", { index: index + 1 })}
          >
            <Input
              aria-label={tf("art.wizard.ports.paramIdAria", { index: index + 1 })}
              compact
              value={port.name}
              onChange={(event) => updateParamPort(index, { name: event.target.value })}
              placeholder={t("art.wizard.ports.placeholder.paramId")}
            />
            <Input
              aria-label={tf("art.wizard.ports.paramLabelAria", { index: index + 1 })}
              compact
              value={port.label}
              onChange={(event) => updateParamPort(index, { label: event.target.value })}
              placeholder={t("art.wizard.ports.placeholder.displayName")}
            />
            <Select
              aria-label={tf("art.wizard.ports.paramTypeAria", { index: index + 1 })}
              compact
              value={port.type}
              onChange={(event) => {
                const nextType = event.target.value;
                updateParamPort(index, {
                  type: nextType,
                  executionType: defaultExecutionTypeForPort(nextType, "input"),
                  widget: defaultWidgetForParam(nextType),
                });
              }}
            >
              {portTypeOptions}
            </Select>
            {!workflowMode ? (
              <Input
                aria-label={tf("art.wizard.ports.paramExecTypeAria", { index: index + 1 })}
                compact
                value={port.executionType}
                onChange={(event) => updateParamPort(index, { executionType: event.target.value })}
                placeholder={t("art.wizard.ports.placeholder.execType")}
              />
            ) : null}
            <Input
              aria-label={tf("art.wizard.ports.paramDefaultAria", { index: index + 1 })}
              compact
              value={port.defaultValue}
              onChange={(event) => updateParamPort(index, { defaultValue: event.target.value })}
              placeholder={t("art.wizard.ports.placeholder.defaultValue")}
            />
            {workflowMode ? (
              <Select
                compact
                className="port-binding-select"
                value={selectedBindingKey(port)}
                onChange={(event) => updateParamBinding(index, event.target.value)}
                disabled={paramBindingsLoading || !paramBindingCandidates.length}
                aria-label={tf("art.wizard.ports.bindParamAria", {
                  name: port.label || port.name || tf("art.wizard.ports.paramFallbackName", { index: index + 1 }),
                })}
                title={t("art.wizard.ports.bindNodeParamTitle")}
              >
                <option value="">
                  {paramBindingsLoading
                    ? t("art.wizard.ports.loadingWorkflowParams")
                    : t("art.wizard.ports.bindToNodeParam")}
                </option>
                {bindingCandidatesByNode.map(([nodeId, group]) => (
                  <optgroup key={nodeId} label={group.label}>
                    {group.candidates.map((candidate) => (
                      <option key={candidate.key} value={candidate.key}>
                        {candidate.paramLabel} · {candidate.target}
                      </option>
                    ))}
                  </optgroup>
                ))}
              </Select>
            ) : null}
            <Button
              aria-label={tf("art.wizard.ports.paramDeleteAria", { index: index + 1 })}
              variant="quiet"
              compact
              type="button"
              onClick={() => setParamPorts((ports) => ports.filter((_, portIndex) => portIndex !== index))}
            >
              {t("art.action.delete")}
            </Button>
          </Panel>
        ))}
      </div>
      <div className="port-editor-section">
        <div className="section-heading-row section-heading-row--compact">
          <h4>{t("art.wizard.ports.outputs")}</h4>
          <Button
            variant="quiet"
            compact
            type="button"
            onClick={() => setOutputPorts((ports) => [...ports, createPortDraft("output")])}
          >
            {t("art.wizard.ports.add")}
          </Button>
        </div>
        {outputPorts.map((port, index) => (
          <Panel
            className="port-editor-row"
            key={`output-${index}`}
            aria-label={tf("art.wizard.ports.outputRowAria", { index: index + 1 })}
          >
            <Input
              aria-label={tf("art.wizard.ports.outputNameAria", { index: index + 1 })}
              compact
              value={port.name}
              onChange={(event) => updateOutputPort(index, { name: event.target.value })}
              placeholder={t("art.wizard.ports.placeholder.name")}
            />
            <Input
              aria-label={tf("art.wizard.ports.outputLabelAria", { index: index + 1 })}
              compact
              value={port.label}
              onChange={(event) => updateOutputPort(index, { label: event.target.value })}
              placeholder={t("art.wizard.ports.placeholder.label")}
            />
            <Select
              aria-label={tf("art.wizard.ports.outputTypeAria", { index: index + 1 })}
              compact
              value={port.type}
              onChange={(event) => {
                const nextType = event.target.value;
                updateOutputPort(index, {
                  type: nextType,
                  executionType: defaultExecutionTypeForPort(nextType, "output"),
                });
              }}
            >
              {portTypeOptions}
            </Select>
            <Input
              aria-label={tf("art.wizard.ports.outputExecTypeAria", { index: index + 1 })}
              compact
              value={port.executionType}
              onChange={(event) => updateOutputPort(index, { executionType: event.target.value })}
              placeholder={t("art.wizard.ports.placeholder.execType")}
            />
            <Select
              compact
              value={port.captureMode}
              onChange={(event) => updateOutputPort(index, { captureMode: event.target.value as ArtPortCaptureMode })}
              aria-label={t("art.wizard.ports.captureModeAria")}
            >
              {outputCaptureModes.map((captureMode) => (
                <option key={captureMode} value={captureMode}>
                  {captureMode}
                </option>
              ))}
            </Select>
            <Input
              aria-label={tf("art.wizard.ports.outputJsonPathAria", { index: index + 1 })}
              compact
              value={port.jsonPath}
              onChange={(event) => updateOutputPort(index, { jsonPath: event.target.value })}
              placeholder={t("art.wizard.ports.placeholder.jsonPath")}
            />
            <Input
              aria-label={tf("art.wizard.ports.outputFilenameAria", { index: index + 1 })}
              compact
              value={port.filename}
              onChange={(event) => updateOutputPort(index, { filename: event.target.value })}
              placeholder={t("art.wizard.ports.placeholder.filename")}
            />
            <Button
              aria-label={tf("art.wizard.ports.outputDeleteAria", { index: index + 1 })}
              variant="quiet"
              compact
              type="button"
              onClick={() => setOutputPorts((ports) => ports.filter((_, portIndex) => portIndex !== index))}
            >
              {t("art.action.delete")}
            </Button>
          </Panel>
        ))}
      </div>
    </div>
  );
}
