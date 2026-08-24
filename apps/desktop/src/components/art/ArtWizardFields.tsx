// Reusable framework and port editors for Art authoring.
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
import { type Dispatch, type SetStateAction, useMemo } from "react";

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
  if (field.secret || field.type === "secret") {
    return (
      <label className="field-label">
        {field.label}{field.required ? " *" : ""}
        <select className="studio-input" value={String(value ?? "")} onChange={(event) => onChange(event.target.value)} required={field.required}>
          <option value="">选择全局机密</option>
          {credentialNames.map((credential) => <option key={credential} value={credential}>{credential}</option>)}
        </select>
      </label>
    );
  }
  if (field.type === "boolean") {
    return (
      <label className="checkbox-line">
        <input type="checkbox" checked={Boolean(value)} onChange={(event) => onChange(event.target.checked)} />
        <span>{field.label}{field.required ? " *" : ""}</span>
      </label>
    );
  }
  if (field.type === "enum") {
    return (
      <label className="field-label">
        {field.label}{field.required ? " *" : ""}
        <select className="studio-input" value={String(value ?? "")} onChange={(event) => onChange(event.target.value)}>
          <option value="">请选择</option>
          {(field.options ?? []).map((option) => (
            <option key={`${field.id}-${String(option.value)}`} value={String(option.value)}>{option.label}</option>
          ))}
        </select>
      </label>
    );
  }
  if (field.type === "json") {
    return (
      <label className="field-label">
        {field.label}{field.required ? " *" : ""}
        <textarea
          className="studio-textarea studio-textarea--compact"
          value={typeof value === "string" ? value : JSON.stringify(value ?? {}, null, 2)}
          placeholder={field.placeholder || "{}"}
          onChange={(event) => onChange(event.target.value)}
        />
      </label>
    );
  }
  return (
    <label className="field-label">
      {field.label}{field.required ? " *" : ""}
      <input
        className="studio-input"
        type={field.type === "number" ? "number" : "text"}
        value={typeof value === "number" || typeof value === "string" ? value : ""}
        min={field.minimum ?? undefined}
        max={field.maximum ?? undefined}
        step={field.step ?? undefined}
        placeholder={field.placeholder ?? undefined}
        onChange={(event) => onChange(field.type === "number" ? Number(event.target.value) : event.target.value)}
      />
    </label>
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
  const updateInputPort = (index: number, patch: Partial<ArtWizardPortDraft>) => {
    setInputPorts((ports) => ports.map((port, portIndex) => portIndex === index ? { ...port, ...patch } : port));
  };
  const updateOutputPort = (index: number, patch: Partial<ArtWizardPortDraft>) => {
    setOutputPorts((ports) => ports.map((port, portIndex) => portIndex === index ? { ...port, ...patch } : port));
  };
  const updateParamPort = (index: number, patch: Partial<ArtWizardPortDraft>) => {
    setParamPorts((ports) => ports.map((port, portIndex) => portIndex === index ? { ...port, ...patch } : port));
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
  const selectedBindingKey = (port: ArtWizardPortDraft) => (
    paramBindingCandidates.find((candidate) => (
      candidate.nodeId === port.bindingNodeId && candidate.target === port.bindingTarget
    ))?.key ?? ""
  );
  const updateParamBinding = (index: number, key: string) => {
    const port = paramPorts[index];
    if (!port) return;
    const candidate = paramBindingCandidates.find((item) => item.key === key);
    if (!candidate) {
      updateParamPort(index, { bindingNodeId: "", bindingTarget: "", bindingKind: "" });
      return;
    }
    const defaultName = !port.name.trim() || port.name.trim() === "param";
    const defaultLabel = !port.label.trim() || port.label.trim() === "参数";
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
      <option value="image">图像 image</option>
      <option value="file">文件 file</option>
      <option value="string">文本 string</option>
      <option value="int">整数 int</option>
      <option value="float">小数 float</option>
      <option value="boolean">布尔 boolean</option>
    </>
  );

  return (
    <div className="advanced-port-editor">
      <div className="port-editor-section">
        <div className="section-heading-row section-heading-row--compact">
          <h4>输入</h4>
          <button className="ghost-button" type="button" onClick={() => setInputPorts((ports) => [...ports, createPortDraft("input")])}>
            添加
          </button>
        </div>
        {inputPorts.map((port, index) => (
          <div className="port-editor-row" key={`input-${index}`}>
            <input className="studio-input" value={port.name} onChange={(event) => updateInputPort(index, { name: event.target.value })} placeholder="name" />
            <input className="studio-input" value={port.label} onChange={(event) => updateInputPort(index, { label: event.target.value })} placeholder="label" />
            <select
              className="studio-input"
              value={port.type}
              onChange={(event) => {
                const nextType = event.target.value;
                updateInputPort(index, { type: nextType, executionType: defaultExecutionTypeForPort(nextType, "input") });
              }}
            >
              {portTypeOptions}
            </select>
            <input className="studio-input" value={port.executionType} onChange={(event) => updateInputPort(index, { executionType: event.target.value })} placeholder="execution type" />
            <input className="studio-input" value={port.defaultValue} onChange={(event) => updateInputPort(index, { defaultValue: event.target.value })} placeholder="default" />
            <label className="checkbox-line checkbox-line--compact">
              <input type="checkbox" checked={port.disabled} onChange={(event) => updateInputPort(index, { disabled: event.target.checked })} />
              <span>禁用</span>
            </label>
            <button className="ghost-button" type="button" onClick={() => setInputPorts((ports) => ports.filter((_, portIndex) => portIndex !== index))}>
              删除
            </button>
          </div>
        ))}
      </div>
      <div className="port-editor-section">
        <div className="section-heading-row section-heading-row--compact">
          <h4>参数</h4>
          <button className="ghost-button" type="button" onClick={() => setParamPorts((ports) => [...ports, createPortDraft("input", { name: "param", label: "参数", type: "string" })])}>
            添加
          </button>
        </div>
        {paramPorts.map((port, index) => (
          <div className={workflowMode ? "port-editor-row port-editor-row--param-workflow" : "port-editor-row"} key={`param-${index}`}>
            <input className="studio-input" value={port.name} onChange={(event) => updateParamPort(index, { name: event.target.value })} placeholder="参数 ID" />
            <input className="studio-input" value={port.label} onChange={(event) => updateParamPort(index, { label: event.target.value })} placeholder="显示名称" />
            <select
              className="studio-input"
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
            </select>
            {!workflowMode ? (
              <input className="studio-input" value={port.executionType} onChange={(event) => updateParamPort(index, { executionType: event.target.value })} placeholder="execution type" />
            ) : null}
            <input className="studio-input" value={port.defaultValue} onChange={(event) => updateParamPort(index, { defaultValue: event.target.value })} placeholder="默认值" />
            {workflowMode ? (
              <select
                className="studio-input port-binding-select"
                value={selectedBindingKey(port)}
                onChange={(event) => updateParamBinding(index, event.target.value)}
                disabled={paramBindingsLoading || !paramBindingCandidates.length}
                aria-label={`绑定 ${port.label || port.name || `参数 ${index + 1}`}`}
                title="绑定到流程节点参数"
              >
                <option value="">{paramBindingsLoading ? "读取流程参数..." : "绑定到节点参数"}</option>
                {bindingCandidatesByNode.map(([nodeId, group]) => (
                  <optgroup key={nodeId} label={group.label}>
                    {group.candidates.map((candidate) => (
                      <option key={candidate.key} value={candidate.key}>
                        {candidate.paramLabel} · {candidate.target}
                      </option>
                    ))}
                  </optgroup>
                ))}
              </select>
            ) : null}
            <button className="ghost-button" type="button" onClick={() => setParamPorts((ports) => ports.filter((_, portIndex) => portIndex !== index))}>
              删除
            </button>
          </div>
        ))}
      </div>
      <div className="port-editor-section">
        <div className="section-heading-row section-heading-row--compact">
          <h4>输出</h4>
          <button className="ghost-button" type="button" onClick={() => setOutputPorts((ports) => [...ports, createPortDraft("output")])}>
            添加
          </button>
        </div>
        {outputPorts.map((port, index) => (
          <div className="port-editor-row port-editor-row--output" key={`output-${index}`}>
            <input className="studio-input" value={port.name} onChange={(event) => updateOutputPort(index, { name: event.target.value })} placeholder="name" />
            <input className="studio-input" value={port.label} onChange={(event) => updateOutputPort(index, { label: event.target.value })} placeholder="label" />
            <select
              className="studio-input"
              value={port.type}
              onChange={(event) => {
                const nextType = event.target.value;
                updateOutputPort(index, { type: nextType, executionType: defaultExecutionTypeForPort(nextType, "output") });
              }}
            >
              {portTypeOptions}
            </select>
            <input className="studio-input" value={port.executionType} onChange={(event) => updateOutputPort(index, { executionType: event.target.value })} placeholder="execution type" />
            <select className="studio-input" value={port.captureMode} onChange={(event) => updateOutputPort(index, { captureMode: event.target.value as ArtPortCaptureMode })} aria-label="捕获模式">
              {outputCaptureModes.map((captureMode) => <option key={captureMode} value={captureMode}>{captureMode}</option>)}
            </select>
            <input className="studio-input" value={port.jsonPath} onChange={(event) => updateOutputPort(index, { jsonPath: event.target.value })} placeholder="JSONPath" />
            <input className="studio-input" value={port.filename} onChange={(event) => updateOutputPort(index, { filename: event.target.value })} placeholder="filename/template" />
            <button className="ghost-button" type="button" onClick={() => setOutputPorts((ports) => ports.filter((_, portIndex) => portIndex !== index))}>
              删除
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
