// Owns script import state, ordered source discovery, and stale-read cleanup.
import { useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
import { useUiText } from "../../i18n/index.ts";
import {
  checkPythonArtJsonNearby,
  inferPythonArtPorts,
  readPythonArtJson,
  readPythonArtSource,
} from "../../services/loomApi";
import { inferPortsFromPythonCode, mapArtJsonPorts, type PythonArtPort } from "../../services/pythonArtSource";
import { basenameWithoutExtension, normalizePythonPort, normalizeToolId, type StudioMessage } from "../app/appShell";
import { createPortDraft, type ArtWizardPortDraft } from "./artWizardModel";

export function useArtScriptSource({
  baseUrl,
  repositoryName,
  name,
  setRepositoryName,
  setName,
  setDescription,
  setInputPorts,
  setOutputPorts,
  setWizardMessage,
}: {
  baseUrl: string;
  repositoryName: string;
  name: string;
  setRepositoryName: Dispatch<SetStateAction<string>>;
  setName: Dispatch<SetStateAction<string>>;
  setDescription: Dispatch<SetStateAction<string>>;
  setInputPorts: Dispatch<SetStateAction<ArtWizardPortDraft[]>>;
  setOutputPorts: Dispatch<SetStateAction<ArtWizardPortDraft[]>>;
  setWizardMessage: Dispatch<SetStateAction<StudioMessage | null>>;
}) {
  const { t } = useUiText();
  const [scriptSourcePath, setScriptSourcePath] = useState("");
  const [scriptSourceCode, setScriptSourceCode] = useState("");
  const [scriptArtJsonPath, setScriptArtJsonPath] = useState("");
  const [scriptSourceDirectory, setScriptSourceDirectory] = useState("");
  const [sourceBusyAction, setSourceBusyAction] = useState<"read" | "art-json" | "infer" | null>(null);
  const sourceRequestVersionRef = useRef(0);

  const applyPythonPorts = (ports: { inputs: PythonArtPort[]; outputs: PythonArtPort[] }) => {
    setInputPorts(
      ports.inputs.map((port) =>
        createPortDraft("input", {
          name: port.name,
          label: port.label,
          type: port.type,
          executionType: port.executionType || port.execution_type,
          defaultValue: typeof port.default === "string" ? port.default : JSON.stringify(port.default ?? ""),
        }),
      ),
    );
    setOutputPorts(
      ports.outputs.map((port) =>
        createPortDraft("output", {
          name: port.name,
          label: port.label,
          type: port.type,
          executionType: port.executionType || port.execution_type,
        }),
      ),
    );
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
      setWizardMessage({ kind: "error", text: t("art.wizard.script.error.enterPythonPath") });
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
      setWizardMessage({
        kind: "info",
        text: configured
          ? t("art.wizard.script.message.readSourceAndArtJson")
          : t("art.wizard.script.message.readSourceAndInferred"),
      });
    } catch (error) {
      if (requestVersion === sourceRequestVersionRef.current) {
        setWizardMessage({
          kind: "error",
          text: error instanceof Error ? error.message : t("art.wizard.script.error.readPythonFailed"),
        });
      }
    } finally {
      if (requestVersion === sourceRequestVersionRef.current) setSourceBusyAction(null);
    }
  };

  const readScriptArtJson = async () => {
    if (!scriptArtJsonPath.trim()) {
      setWizardMessage({ kind: "error", text: t("art.wizard.script.error.enterArtJsonPath") });
      return;
    }
    const requestVersion = ++sourceRequestVersionRef.current;
    setSourceBusyAction("art-json");
    try {
      const response = await readPythonArtJson(baseUrl, scriptArtJsonPath.trim());
      if (requestVersion !== sourceRequestVersionRef.current) return;
      setScriptArtJsonPath(response.artJsonPath || scriptArtJsonPath);
      applyArtJson(response.artJson, scriptSourcePath);
      setWizardMessage({ kind: "info", text: t("art.wizard.script.message.readArtJson") });
    } catch (error) {
      if (requestVersion === sourceRequestVersionRef.current) {
        setWizardMessage({
          kind: "error",
          text: error instanceof Error ? error.message : t("art.wizard.script.error.readArtJsonFailed"),
        });
      }
    } finally {
      if (requestVersion === sourceRequestVersionRef.current) setSourceBusyAction(null);
    }
  };

  const inferScriptPorts = async () => {
    if (!scriptSourcePath.trim() && !scriptSourceCode.trim()) {
      setWizardMessage({ kind: "error", text: t("art.wizard.script.error.enterSourceOrCode") });
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
      setWizardMessage({ kind: "info", text: t("art.wizard.script.message.updatedPorts") });
    } catch (error) {
      if (requestVersion !== sourceRequestVersionRef.current) return;
      const fallback = inferPortsFromPythonCode(scriptSourceCode);
      if (fallback.inputs.length || fallback.outputs.length) {
        applyPythonPorts(fallback);
        setWizardMessage({ kind: "info", text: t("art.wizard.script.message.updatedPortsFromCode") });
      } else {
        setWizardMessage({
          kind: "error",
          text: error instanceof Error ? error.message : t("art.wizard.script.error.inferPortsFailed"),
        });
      }
    } finally {
      if (requestVersion === sourceRequestVersionRef.current) setSourceBusyAction(null);
    }
  };
  useEffect(
    () => () => {
      sourceRequestVersionRef.current += 1;
    },
    [],
  );
  return {
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
  };
}
