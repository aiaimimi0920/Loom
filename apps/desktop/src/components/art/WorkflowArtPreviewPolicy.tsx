import { CheckboxGroup, Field, MultiSelect, Panel, RadioGroup, Select } from "@neuro/ui";
import { useId } from "react";
import type { WorkflowOutputBinding, WorkflowPreviewNodeOption } from "../../services/workflowStudio";

/** Adapts workflow policy choices; node dependencies and output selection remain in the wizard controller. */
export function WorkflowArtPreviewPolicy({
  options,
  output,
  requiredNodes,
  onNodeRequired,
  onSelectNode,
  onOutputChange,
}: {
  options: WorkflowPreviewNodeOption[];
  output: WorkflowOutputBinding | undefined;
  requiredNodes: string[];
  onNodeRequired: (nodeId: string, required: boolean) => void;
  onSelectNode: (option: WorkflowPreviewNodeOption) => void;
  onOutputChange: (output: WorkflowOutputBinding) => void;
}) {
  const name = useId();
  const selected = options.find((option) => option.nodeId === output?.nodeId);
  const requiredOptions = options.map((option) => ({
    value: option.nodeId,
    label: option.label,
    disabled: option.nodeId === output?.nodeId,
  }));
  const values = options
    .filter((option) => requiredNodes.includes(option.nodeId) || option.nodeId === output?.nodeId)
    .map((option) => option.nodeId);
  const changeRequired = (next: string[]) => {
    for (const option of options) {
      if (next.includes(option.nodeId) !== values.includes(option.nodeId))
        onNodeRequired(option.nodeId, next.includes(option.nodeId));
    }
  };
  const selectNode = (value: string) => {
    const option = options.find((item) => item.nodeId === value);
    if (option) onSelectNode(option);
  };
  const previewOptions = options.map((option) => ({
    value: option.nodeId,
    label: option.label,
    disabled: !option.outputs.length,
  }));
  return (
    <Panel className="art-workflow-preview-policy" aria-label="流程预览策略">
      {options.length > 6 ? (
        <MultiSelect
          label="必要节点"
          options={requiredOptions}
          values={values}
          onValuesChange={changeRequired}
          search={{ label: "搜索流程节点", emptyMessage: "没有匹配的节点" }}
        />
      ) : (
        <CheckboxGroup label="必要节点" options={requiredOptions} values={values} onValuesChange={changeRequired} />
      )}
      {options.length > 6 ? (
        <Field label="预览节点">
          <Select value={selected?.nodeId ?? ""} onChange={(event) => selectNode(event.target.value)}>
            <option value="" disabled>
              选择预览节点
            </option>
            {previewOptions.map((option) => (
              <option key={option.value} value={option.value} disabled={option.disabled}>
                {option.label}
              </option>
            ))}
          </Select>
        </Field>
      ) : (
        <RadioGroup
          label="预览节点"
          name={name}
          options={previewOptions}
          value={selected?.nodeId ?? ""}
          onValueChange={selectNode}
        />
      )}
      <Field label="预览端口">
        <Select
          value={selected && output ? output.output : ""}
          disabled={!selected || selected.outputs.length < 2}
          onChange={(event) => {
            if (selected) onOutputChange({ nodeId: selected.nodeId, output: event.target.value, kind: "node_result" });
          }}
        >
          {!selected ? (
            <option value="">—</option>
          ) : (
            selected.outputs.map((port) => (
              <option key={port.name} value={port.name}>
                {port.label}
              </option>
            ))
          )}
        </Select>
      </Field>
    </Panel>
  );
}
