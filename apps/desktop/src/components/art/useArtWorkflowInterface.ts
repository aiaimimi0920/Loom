// Hydrates a selected workflow interface without owning the wizard's editable ports or preview policy.
import { useEffect, useState, type Dispatch, type SetStateAction, type RefObject } from "react";
import { getWorkflowBundle, type LoomToolDefinition } from "../../services/loomApi";
import {
  collectWorkflowParamBindingCandidates,
  inferWorkflowArtInterface,
  parseWorkflowYamlLite,
  type WorkflowGraphLite,
  type WorkflowOutputBinding,
  type WorkflowParamBindingCandidate,
} from "../../services/workflowStudio";
import type { ArtWizardMode } from "../app/appShell";
import { createPortDraft, type ArtCreationRequest, type ArtWizardPortDraft } from "./artWizardModel";
export function useArtWorkflowInterface({
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
}: {
  baseUrl: string;
  initialRequest: ArtCreationRequest | null;
  mode: ArtWizardMode;
  tools: LoomToolDefinition[];
  workflowId: string;
  workflowInterfaceAppliedRef: RefObject<string>;
  setWorkflowGraph: Dispatch<SetStateAction<WorkflowGraphLite | null>>;
  setInputPorts: Dispatch<SetStateAction<ArtWizardPortDraft[]>>;
  setOutputPorts: Dispatch<SetStateAction<ArtWizardPortDraft[]>>;
  setParamPorts: Dispatch<SetStateAction<ArtWizardPortDraft[]>>;
  setWorkflowPreviewOutput: Dispatch<SetStateAction<WorkflowOutputBinding | undefined>>;
  setWorkflowPreviewRequiredNodes: Dispatch<SetStateAction<string[]>>;
}) {
  const [workflowParamCandidates, setWorkflowParamCandidates] = useState<WorkflowParamBindingCandidate[]>([]);
  const [workflowInterfaceLoading, setWorkflowInterfaceLoading] = useState(false);
  const [workflowInterfaceError, setWorkflowInterfaceError] = useState<string | null>(null);
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
          const templateMatches =
            initialRequest?.mode === "workflow" &&
            initialRequest.workflowId === workflowId &&
            Boolean(initialRequest.templateTool);
          if (!templateMatches) {
            const inferredInputs = inferred.inputs
              .filter((port) => port.bindingKind !== "param")
              .map((port) =>
                createPortDraft("input", {
                  name: port.name,
                  label: port.label,
                  type: port.type,
                  executionType: port.executionType,
                  defaultValue: port.default || "",
                  bindingNodeId: port.bindingNodeId || "",
                  bindingTarget: port.bindingTarget || "",
                  bindingKind: port.bindingKind || "",
                }),
              );
            const inferredOutputs = inferred.outputs.map((port) =>
              createPortDraft("output", {
                name: port.name,
                label: port.label,
                type: port.type,
                executionType: port.executionType,
                bindingNodeId: port.bindingNodeId || "",
                bindingTarget: port.bindingTarget || "",
                bindingKind: port.bindingKind || "",
              }),
            );
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
  return { workflowParamCandidates, workflowInterfaceLoading, workflowInterfaceError };
}
