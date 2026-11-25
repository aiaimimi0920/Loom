// Owns catalog paging, trust confirmation and installation of published Art packages.
import { Badge, Button, EmptyState, Notice, Panel } from "@neuro/ui";
import { filterArtStoreEntries, officialFrameworkDisplayName } from "../../services/artHubUi";
import type { LoomFramework } from "../../services/loomApi";
import { fetchCloudArtCatalog, type CloudArtEntry } from "../../services/cloudCatalog";
import { canInstallCloudPackage, installCloudPackage } from "../../services/cloudPackageInstall";
import { artFrameworkIconKind, ArtIcon } from "../app/appShell";
import { requestAppConfirmation } from "../feedback/AppFeedback";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

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
  const [catalog, setCatalog] = useState<CloudArtEntry[]>([]);
  const [nextAfter, setNextAfter] = useState<string | null>(null);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [installingId, setInstallingId] = useState<string | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const catalogLoadVersion = useRef(0);
  const visibleCatalog = useMemo(
    () => filterArtStoreEntries(catalog, frameworks, selectedFrameworkIds, searchText, officialOnly),
    [catalog, frameworks, officialOnly, searchText, selectedFrameworkIds],
  );

  const loadCatalog = useCallback(
    async (after = "") => {
      const version = ++catalogLoadVersion.current;
      setCatalogLoading(true);
      setMessage(null);
      try {
        const { arts, nextAfter } = await fetchCloudArtCatalog(baseUrl, after);
        if (version !== catalogLoadVersion.current) return;
        setCatalog(arts);
        setNextAfter(nextAfter);
      } catch (err) {
        if (version === catalogLoadVersion.current) {
          setMessage({ ok: false, text: err instanceof Error ? err.message : "无法访问商店。" });
        }
      } finally {
        if (version === catalogLoadVersion.current) setCatalogLoading(false);
      }
    },
    [baseUrl],
  );

  useEffect(() => {
    if (!active) return;
    void loadCatalog();
    return () => {
      catalogLoadVersion.current += 1;
    };
  }, [active, loadCatalog, refreshToken]);

  const install = async (art: CloudArtEntry) => {
    if (
      art.official !== true &&
      !(await requestAppConfirmation({
        title: "安装未认证 Art",
        message: `“${art.name ?? art.id}”未经官方认证，可能包含恶意代码。仅在信任发布者和包来源时继续。`,
        confirmLabel: "继续安装",
        tone: "warning",
      }))
    )
      return;
    setInstallingId(art.id);
    setMessage(null);
    try {
      await installCloudPackage(baseUrl, art.publication);
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
        <Notice
          className="art-store__message"
          tone={message.ok ? "success" : "danger"}
          role={message.ok ? "status" : "alert"}
        >
          <span>{message.text}</span>
          {!message.ok ? (
            <Button onClick={() => void loadCatalog()} disabled={catalogLoading} loading={catalogLoading}>
              重试
            </Button>
          ) : null}
        </Notice>
      ) : null}
      {catalogLoading && catalog.length === 0 ? <Notice role="status">加载中</Notice> : null}
      {!catalogLoading && !message && visibleCatalog.length === 0 ? <EmptyState title="没有匹配的 Art" /> : null}
      <div className="card-grid art-store-grid">
        {visibleCatalog.map((entry) => {
          const art = catalog.find((item) => item.id === entry.id)!;
          const frameworkReference = art.framework?.startsWith("neuro.official/")
            ? art.framework.slice("neuro.official/".length)
            : (art.framework ?? null);
          const frameworkLabel = officialFrameworkDisplayName(art.framework) ?? art.framework ?? "Art";
          const installing = installingId === art.id;
          return (
            <Panel className="art-market-entry" key={art.qualifiedId || art.id}>
              <div className="art-store-card__head">
                <h3 title={art.name ?? art.id}>{art.name ?? art.id}</h3>
                <div className="art-store-card__badges">
                  <Badge tone={art.official === true ? "success" : "warning"}>
                    {art.official === true ? "官方" : "未认证"}
                  </Badge>
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
                  <p className="art-store-card__description" title={art.description}>
                    {art.description}
                  </p>
                ) : null}
                <p className="art-store-card__identity mono-line" title={art.globalId || art.qualifiedId || art.id}>
                  {art.globalId || art.qualifiedId || art.id}
                  {art.latestVersion ? ` · ${art.latestVersion}` : ""}
                </p>
              </div>
              <div className="art-store-card__actions">
                <Button
                  variant="primary"
                  loading={installing}
                  onClick={() => void install(art)}
                  disabled={installingId !== null || !canInstallCloudPackage(art.publication)}
                >
                  {canInstallCloudPackage(art.publication) ? "安装" : "非原生 Art 包"}
                </Button>
              </div>
            </Panel>
          );
        })}
      </div>
      {nextAfter && (
        <Button
          className="art-store__next"
          loading={catalogLoading}
          disabled={catalogLoading || installingId !== null}
          onClick={() => void loadCatalog(nextAfter)}
        >
          下一页
        </Button>
      )}
    </section>
  );
}
