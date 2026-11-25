// Owns script import state, ordered source discovery, and stale-read cleanup.
import { useEffect, useRef, useState, type Dispatch, type SetStateAction } from "react";
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
