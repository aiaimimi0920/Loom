// Owns catalog paging, trust confirmation and installation of published Art packages.
import { Badge, Button, EmptyState, Notice, Panel } from "@neuro/ui";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { uiText, useUiText } from "../../i18n/index.ts";
import { filterArtStoreEntries, officialFrameworkDisplayName } from "../../services/artHubUi";
import { fetchCloudArtCatalog, type CloudArtEntry } from "../../services/cloudCatalog";
import { canInstallCloudPackage, installCloudPackage } from "../../services/cloudPackageInstall";
import type { LoomFramework } from "../../services/loomApi";
import { artFrameworkIconKind, ArtIcon } from "../app/appShell";
import { requestAppConfirmation } from "../feedback/AppFeedback";

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
  const { t, tf } = useUiText();
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
          setMessage({ ok: false, text: err instanceof Error ? err.message : uiText("art.store.loadFailed") });
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
        title: t("art.store.unverifiedConfirm.title"),
        message: tf("art.store.unverifiedConfirm.message", { name: art.name ?? art.id }),
        confirmLabel: t("art.store.unverifiedConfirm.confirm"),
        tone: "warning",
      }))
    )
      return;
    setInstallingId(art.id);
    setMessage(null);
    try {
      await installCloudPackage(baseUrl, art.publication);
      setMessage({ ok: true, text: tf("art.store.installedMessage", { name: art.name ?? art.id }) });
      await onInstalled();
    } catch (err) {
      setMessage({ ok: false, text: err instanceof Error ? err.message : t("art.store.installFailed") });
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
              {t("art.action.retry")}
            </Button>
          ) : null}
        </Notice>
      ) : null}
      {catalogLoading && catalog.length === 0 ? <Notice role="status">{t("art.common.loading")}</Notice> : null}
      {!catalogLoading && !message && visibleCatalog.length === 0 ? (
        <EmptyState title={t("art.store.empty")} />
      ) : null}
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
                    {art.official === true ? t("art.store.badge.official") : t("art.store.badge.unverified")}
                  </Badge>
                  <span
                    className="art-registry-card__framework-icon"
                    role="img"
                    aria-label={tf("art.card.frameworkIconAria", { framework: frameworkLabel })}
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
                  {canInstallCloudPackage(art.publication) ? t("art.action.install") : t("art.store.nonNativePackage")}
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
          {t("art.store.nextPage")}
        </Button>
      )}
    </section>
  );
}
