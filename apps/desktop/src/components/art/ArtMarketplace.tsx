// Contains Art store cards, framework filters, and the publish dialog.
import {
  filterArtStoreEntries,
  frameworkFilterLabel,
  frameworkIdentity,
  isLocallyAuthoredTool,
  officialFrameworkDisplayName,
} from "../../services/artHubUi";
import {
  type ArtStoreEntry,
  fetchArtStoreCatalog,
  installArtFromStore,
  type LoomFramework,
  LoomToolDefinition,
  publishArt,
} from "../../services/loomApi";
import { artFrameworkIconKind, ArtIcon, StudioMessage } from "../app/appShell";
import { requestAppConfirmation } from "../feedback/AppFeedback";
import {
  type KeyboardEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

export function ArtStoreCard({
  baseUrl,
  active,
  frameworks,
  selectedFrameworkIds,
  searchText,
  officialOnly,
  refreshToken,
  onInstalled,
}: {
  baseUrl: string;
  active: boolean;
  frameworks: LoomFramework[];
  selectedFrameworkIds: ReadonlySet<string> | null;
  searchText: string;
  officialOnly: boolean;
  refreshToken: number;
  onInstalled: () => void | Promise<void>;
}) {
  const [catalog, setCatalog] = useState<ArtStoreEntry[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [installingId, setInstallingId] = useState<string | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const catalogLoadVersion = useRef(0);
  const visibleCatalog = useMemo(() => filterArtStoreEntries(
    catalog,
    frameworks,
    selectedFrameworkIds,
    searchText,
    officialOnly,
  ), [catalog, frameworks, officialOnly, searchText, selectedFrameworkIds]);

  const loadCatalog = useCallback(async () => {
    const version = ++catalogLoadVersion.current;
    setCatalogLoading(true);
    setMessage(null);
    try {
      const arts = await fetchArtStoreCatalog(baseUrl);
      if (version !== catalogLoadVersion.current) return;
      setCatalog(arts);
    } catch (err) {
      if (version === catalogLoadVersion.current) {
        setMessage({ ok: false, text: err instanceof Error ? err.message : "无法访问商店。" });
      }
    } finally {
      if (version === catalogLoadVersion.current) setCatalogLoading(false);
    }
  }, [baseUrl]);

  useEffect(() => {
    if (!active) return;
    void loadCatalog();
    return () => {
      catalogLoadVersion.current += 1;
    };
  }, [active, loadCatalog, refreshToken]);

  const install = async (art: ArtStoreEntry) => {
    if (art.official !== true && !await requestAppConfirmation({
      title: "安装未认证 Art",
      message: `“${art.name ?? art.id}”未经官方认证，可能包含恶意代码。仅在信任发布者和包来源时继续。`,
      confirmLabel: "继续安装",
      tone: "warning",
    })) return;
    setInstallingId(art.id);
    setMessage(null);
    try {
      await installArtFromStore(baseUrl, art.id);
      setMessage({ ok: true, text: `已安装 ${art.name ?? art.id}。` });
      await onInstalled();
    } catch (err) {
      setMessage({ ok: false, text: err instanceof Error ? err.message : "安装失败。" });
    } finally {
      setInstallingId(null);
    }
  };

  return (
    <section className="content-grid art-store">
      {message ? (
        <div className={message.ok ? "art-store__message success-text" : "art-store__message error-text"}>
          <span>{message.text}</span>
          {!message.ok ? (
            <button className="ghost-button" type="button" onClick={() => void loadCatalog()} disabled={catalogLoading}>
              重试
            </button>
          ) : null}
        </div>
      ) : null}
      {catalogLoading && catalog.length === 0 ? <p className="muted-line">加载中…</p> : null}
      {!catalogLoading && !message && visibleCatalog.length === 0 ? <p className="muted-line">没有匹配的 Art</p> : null}
      <div className="card-grid art-store-grid">
        {visibleCatalog.map((art) => {
          const frameworkReference = art.framework?.startsWith("neuro.official/")
            ? art.framework.slice("neuro.official/".length)
            : art.framework ?? null;
          const frameworkLabel = officialFrameworkDisplayName(art.framework) ?? art.framework ?? "Art";
          const installing = installingId === art.id;
          return (
            <article
              className={`glass-card art-store-card ${art.official === true ? "art-store-card--official" : "art-store-card--unverified"}`}
              key={art.qualifiedId || art.id}
            >
              <div className="art-store-card__head">
                <h3 title={art.name ?? art.id}>{art.name ?? art.id}</h3>
                <div className="art-store-card__badges">
                  <span className={art.official === true ? "art-store-card__trust art-store-card__trust--official" : "art-store-card__trust art-store-card__trust--unverified"}>
                    {art.official === true ? "官方" : "未认证"}
                  </span>
                  <span
                    className="art-registry-card__framework-icon"
                    role="img"
                    aria-label={`${frameworkLabel} Art`}
                    title={frameworkLabel}
                  >
                    <ArtIcon kind={artFrameworkIconKind(frameworkReference)} />
                  </span>
                </div>
              </div>
              <div className="art-store-card__body">
                {art.description ? (
                  <p className="art-store-card__description" title={art.description}>{art.description}</p>
                ) : null}
                <p className="art-store-card__identity mono-line" title={art.globalId || art.qualifiedId || art.id}>
                  {art.globalId || art.qualifiedId || art.id}
                  {art.latestVersion ? ` · ${art.latestVersion}` : ""}
                </p>
              </div>
              <div className="art-store-card__actions">
                <button
                  className="signal-button"
                  type="button"
                  onClick={() => void install(art)}
                  disabled={installingId !== null}
                >
                  {installing ? "安装中" : "安装"}
                </button>
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}

export function FrameworkFilter({
  frameworks,
  selectedFrameworkIds,
  onToggle,
  actions,
}: {
  frameworks: LoomFramework[];
  selectedFrameworkIds: ReadonlySet<string> | null;
  onToggle: (frameworkId: string) => void;
  actions: ReactNode;
}) {
  return (
    <div className="framework-filter" role="group" aria-label="按框架筛选 Art">
      <div className="framework-filter__options">
        {frameworks.map((framework) => {
          const identity = frameworkIdentity(framework);
          const checked = selectedFrameworkIds === null || selectedFrameworkIds.has(identity);
          return (
            <label
              className={checked ? "framework-filter__option framework-filter__option--checked" : "framework-filter__option"}
              key={identity}
            >
              <input
                type="checkbox"
                checked={checked}
                onChange={() => onToggle(identity)}
              />
              <span>{frameworkFilterLabel(framework)}</span>
            </label>
          );
        })}
        {frameworks.length === 0 ? <span className="muted-line">暂无框架</span> : null}
      </div>
      <div className="framework-filter__actions">
        {actions}
      </div>
    </div>
  );
}

export function authoredArtVersion(tool: LoomToolDefinition): string {
  const metadata = typeof tool.metadata === "object" && tool.metadata !== null && !Array.isArray(tool.metadata)
    ? tool.metadata as Record<string, unknown>
    : null;
  const packageSecurity = typeof metadata?.packageSecurity === "object"
    && metadata.packageSecurity !== null
    && !Array.isArray(metadata.packageSecurity)
    ? metadata.packageSecurity as Record<string, unknown>
    : null;
  return typeof packageSecurity?.version === "string" && packageSecurity.version.trim()
    ? packageSecurity.version.trim()
    : "0.1.0";
}

export function ArtPublishDialog({
  open,
  tools,
  baseUrl,
  onClose,
  onPublished,
}: {
  open: boolean;
  tools: LoomToolDefinition[];
  baseUrl: string;
  onClose: () => void;
  onPublished: () => Promise<void>;
}) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const closeButtonRef = useRef<HTMLButtonElement | null>(null);
  const [busyArtId, setBusyArtId] = useState<string | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const authoredTools = useMemo(() => tools.filter(isLocallyAuthoredTool), [tools]);

  useEffect(() => {
    if (!open) {
      setMessage(null);
      return;
    }
    closeButtonRef.current?.focus();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const handleKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        if (!busyArtId) onClose();
        return;
      }
      if (event.key !== "Tab" || !dialogRef.current) return;
      const focusable = [...dialogRef.current.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [tabindex]:not([tabindex="-1"])',
      )];
      if (!focusable.length) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [busyArtId, onClose, open]);

  if (!open) return null;

  const publish = async (tool: LoomToolDefinition) => {
    setBusyArtId(tool.id);
    setMessage(null);
    try {
      const result = await publishArt(baseUrl, tool.id);
      await onPublished();
      setMessage({ ok: true, text: `已发布 ${tool.name || tool.id} · ${result.globalId}` });
    } catch (error) {
      setMessage({ ok: false, text: error instanceof Error ? error.message : "发布失败。" });
    } finally {
      setBusyArtId(null);
    }
  };

  return (
    <div
      className="framework-dialog-backdrop"
      role="presentation"
      onMouseDown={(event) => {
        if (!busyArtId && event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="framework-dialog art-publish-dialog"
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="art-publish-dialog-title"
      >
        <header className="framework-dialog__header">
          <h2 id="art-publish-dialog-title">发布 Art</h2>
          <button className="ghost-button" type="button" ref={closeButtonRef} onClick={onClose} disabled={busyArtId !== null}>
            关闭
          </button>
        </header>
        {message ? <p className={message.ok ? "success-text" : "error-text"}>{message.text}</p> : null}
        <div className="art-publish-dialog__list">
          {authoredTools.map((tool) => {
            const busy = busyArtId === tool.id;
            return (
              <article className="art-publish-dialog__row" key={tool.id}>
                <div className="art-publish-dialog__identity">
                  <h3 title={tool.name || tool.id}>{tool.name || tool.id}</h3>
                  <p className="mono-line" title={tool.id}>{tool.id}</p>
                </div>
                <span className="art-publish-dialog__version">{authoredArtVersion(tool)}</span>
                <button className="signal-button" type="button" onClick={() => void publish(tool)} disabled={busyArtId !== null}>
                  {busy ? "发布中" : "发布"}
                </button>
              </article>
            );
          })}
          {authoredTools.length === 0 ? <p className="muted-line">暂无本地创建的 Art</p> : null}
        </div>
      </div>
    </div>
  );
}
