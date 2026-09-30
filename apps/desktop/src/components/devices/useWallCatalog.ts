import { useEffect, useRef, useState } from "react";
import { identifyWallEndpoint, readWallState, removeWallLayout, saveWallLayout, setWallPresentation } from "../../services/loomApi/walls.ts";
import { loadWallSourceInventory, type WallSourceInventory } from "../../services/loomApi/wallSourceInventory.ts";
import { errorMessage } from "../../services/loomApi/transport.ts";
import type { WallLayout, WallPresentationMode, WallState } from "../../services/loomApi/wallTypes.ts";
import { createWallDraft, draftAfterPresentation, type WallDraft } from "./wallDraft.ts";

// Draft CAS is pinned to the version the user opened; polling never rebases edits.
export function useWallCatalog(baseUrl: string, online: boolean) {
  const [state, setState] = useState<WallState | null>(null);
  const [draft, setDraft] = useState<WallDraft | null>(null);
  const [inventory, setInventory] = useState<WallSourceInventory>({ sources: [], errors: [] });
  const [sourcesLoading, setSourcesLoading] = useState(false);
  const [sourceRefreshKey, setSourceRefreshKey] = useState(0);
  const [error, setError] = useState("");
  const [writeError, setWriteError] = useState("");
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const generation = useRef(0), writing = useRef(false);
  const [refreshKey, setRefreshKey] = useState(0);
  useEffect(() => {
    setSourcesLoading(online);
    if (!online) return;
    return loadWallSourceInventory(baseUrl, (next, pending) => { setInventory(next); setSourcesLoading(pending); });
  }, [baseUrl, online, refreshKey, sourceRefreshKey]);
  useEffect(() => {
    setState(null); setDraft(null); setError(""); setWriteError("");
    setInventory({ sources: [], errors: [] });
  }, [baseUrl, refreshKey]);
  useEffect(() => {
    const id = ++generation.current;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setLoading(online);
    async function poll() {
      try {
        const next = await readWallState(baseUrl);
        if (id !== generation.current) return;
        if (!writing.current) {
          setState((current) => current && current.revision > next.revision ? current : next);
          setError("");
        }
      } catch (error) {
        if (id === generation.current) setError(errorMessage(error));
      } finally {
        if (id === generation.current) { setLoading(false); timer = setTimeout(() => void poll(), 4000); }
      }
    }
    if (online) {
      void poll();
    }
    return () => { generation.current += 1; clearTimeout(timer); };
  }, [baseUrl, online, refreshKey]);
  async function mutate(remove: boolean, activationDelayMs = 0) {
    if (!draft || writing.current || !online) return;
    const id = generation.current;
    writing.current = true; setBusy(true); setWriteError("");
    try {
      const next = remove ? await removeWallLayout(baseUrl, draft.baseRevision, draft.layout.wallId)
        : await saveWallLayout(baseUrl, draft.baseRevision, draft.layout, activationDelayMs);
      if (id !== generation.current) return;
      setState(next);
      setDraft(remove ? null : createWallDraft(next.revision, next.layouts.find((l) => l.wallId === draft.layout.wallId)));
    } catch (error) {
      if (id === generation.current) setWriteError(`${errorMessage(error)}。草稿已保留；请核对目录版本后重新载入。`);
    } finally { writing.current = false; setBusy(false); }
  }
  function edit(layout: WallLayout) {
    setDraft((current) => current ? { ...current, layout, dirty: true } : null);
  }
  async function present(wallId: string, mode: WallPresentationMode) {
    if (!state || writing.current || !online) return;
    const id = generation.current, baseRevision = state.revision;
    writing.current = true; setBusy(true); setWriteError("");
    try {
      const next = await setWallPresentation(baseUrl, baseRevision, wallId, mode);
      if (id !== generation.current) return;
      setState(next); setDraft((current) => draftAfterPresentation(current, baseRevision, next));
    } catch (error) {
      if (id === generation.current) setWriteError(`${errorMessage(error)}。显示操作未确认；草稿已保留，请核对目录状态。`);
    } finally { writing.current = false; setBusy(false); }
  }
  async function identify(endpointId: string) {
    if (writing.current || !online) return;
    const id = generation.current;
    writing.current = true; setBusy(true); setWriteError("");
    try {
      const next = await identifyWallEndpoint(baseUrl, endpointId);
      if (id === generation.current) setState(next);
    } catch (error) {
      if (id === generation.current) setWriteError(`${errorMessage(error)}。屏幕识别未确认，布局草稿已保留。`);
    } finally { writing.current = false; setBusy(false); }
  }
  return { state, draft, inventory, sourcesLoading, error: writeError || error, busy, loading, edit, present, identify,
    select: (layout?: WallLayout) => { if (state) { setDraft(createWallDraft(state.revision, layout)); setWriteError(""); } },
    close: () => setDraft(null), save: (activationDelayMs = 0) => mutate(false, activationDelayMs), remove: () => mutate(true),
    refresh: () => setRefreshKey((key) => key + 1),
    refreshSources: () => { if (online && !sourcesLoading) setSourceRefreshKey((key) => key + 1); },
  };
}
