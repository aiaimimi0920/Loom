// Renders product identity and application diagnostics.
import { Button, Select } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
import { LoomMark } from "../app/appShell";
import { ApplicationDiagnosticsInfo, SettingsAppId, SettingsSectionIcon } from "./settingsModel";

export function ApplicationAboutMark({ app }: { app: SettingsAppId }) {
  if (app === "loom") {
    return <LoomMark />;
  }
  return (
    <svg className="about-panel__otter-mark" viewBox="0 0 1024 1024" aria-hidden="true">
      <defs>
        <filter id="about-otter-green-glow" x="-90%" y="-90%" width="280%" height="280%">
          <feGaussianBlur stdDeviation="18" result="blur" />
          <feColorMatrix in="blur" type="matrix" values="0 0 0 0 0.13  0 0 0 0 0.77  0 0 0 0 0.37  0 0 0 0.78 0" />
          <feMerge><feMergeNode /><feMergeNode in="SourceGraphic" /></feMerge>
        </filter>
        <filter id="about-otter-yellow-glow" x="-90%" y="-90%" width="280%" height="280%">
          <feGaussianBlur stdDeviation="18" result="blur" />
          <feColorMatrix in="blur" type="matrix" values="0 0 0 0 1  0 0 0 0 0.9  0 0 0 0 0  0 0 0 0.82 0" />
          <feMerge><feMergeNode /><feMergeNode in="SourceGraphic" /></feMerge>
        </filter>
      </defs>
      <g fill="none" strokeLinecap="round" strokeLinejoin="round">
        <path d="M250 394V250h144" stroke="var(--loom-brand-primary)" strokeWidth="72" filter="url(#about-otter-green-glow)" />
        <path d="M774 394V250H630" stroke="var(--loom-brand-primary)" strokeWidth="72" filter="url(#about-otter-green-glow)" />
        <path d="M250 630v144h144" stroke="var(--loom-brand-primary)" strokeWidth="72" filter="url(#about-otter-green-glow)" />
        <path d="M774 630v144H630" stroke="var(--loom-brand-secondary)" strokeWidth="72" filter="url(#about-otter-yellow-glow)" />
      </g>
    </svg>
  );
}

export function AboutPanel({
  app,
  diagnostics,
  logLevel,
  onLogLevelChange,
  onCheckUpdate,
  onOpenLog,
  onOpenRepository,
}: {
  app: SettingsAppId;
  diagnostics: ApplicationDiagnosticsInfo;
  logLevel: string;
  onLogLevelChange: (value: string) => void;
  onCheckUpdate: () => void;
  onOpenLog: (target: "directory" | "file") => void;
  onOpenRepository: (url: string) => void;
}) {
  const { t, tf } = useUiText();
  const titleId = `about-product-name-${app}`;
  return (
    <section className="about-panel" aria-labelledby={titleId}>
      <div className="about-panel__group">
        <div className="about-panel__identity">
          <span className={`about-panel__mark about-panel__mark--${app}`} aria-hidden="true">
            <ApplicationAboutMark app={app} />
          </span>
          <h2 id={titleId}>{diagnostics.appName}</h2>
        </div>
        <dl className="about-panel__rows">
          <div>
            <dt>{t("settings.about.appName")}</dt>
            <dd>{diagnostics.appName}</dd>
          </div>
          <div>
            <dt>{t("settings.about.version")}</dt>
            <dd>{diagnostics.version}</dd>
          </div>
          <div>
            <dt>{t("settings.about.checkUpdates")}</dt>
            <dd><Button compact onClick={onCheckUpdate}>{t("settings.about.checkNow")}</Button></dd>
          </div>
          {diagnostics.repositoryUrl ? (
            <div className="about-panel__repository-row">
              <dt>{t("settings.about.repository")}</dt>
              <dd>
                <Button compact variant="quiet"
                  className="about-panel__repository-link"
                  title={tf("settings.about.openRepositoryTitle", { url: diagnostics.repositoryUrl })}
                  onClick={() => onOpenRepository(diagnostics.repositoryUrl!)}
                >
                  <span>{diagnostics.repositoryUrl}</span>
                  <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M14 5h5v5M12 12l7-7M19 13v5a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h5" />
                  </svg>
                </Button>
                <span className="about-panel__commit" title={t("settings.about.commitTitle")}>
                  {diagnostics.commitShort?.slice(0, 6) || "------"}
                </span>
              </dd>
            </div>
          ) : null}
        </dl>
      </div>

      <div className="about-panel__group about-panel__group--diagnostics">
        <div className="about-panel__group-title">
          <SettingsSectionIcon kind="system" />
          <h3>{t("settings.about.diagnosticsHeader")}</h3>
        </div>
        <dl className="about-panel__rows">
          <div>
            <dt>{t("settings.about.logLevel")}</dt>
            <dd>
              <Select
                className="about-panel__log-level"
                aria-label={tf("settings.about.logLevelAria", { appName: diagnostics.appName })}
                value={logLevel}
                onChange={(event) => onLogLevelChange(event.target.value)}
              >
                <option value="error">Error</option>
                <option value="warn">Warn</option>
                <option value="info">{t("settings.about.logLevelInfoDefault")}</option>
                <option value="debug">Debug</option>
              </Select>
            </dd>
          </div>
          <div className="about-panel__path-row">
            <dt>{t("settings.about.logLocation")}</dt>
            <dd>
              <code title={diagnostics.logDir || undefined}>{diagnostics.logDir || t("settings.about.logNotLoaded")}</code>
              <Button compact onClick={() => onOpenLog("directory")}>{t("settings.about.openFolder")}</Button>
            </dd>
          </div>
          <div>
            <dt>{t("settings.about.viewLog")}</dt>
            <dd>
              <span className="about-panel__log-file">
                {diagnostics.logFile ? diagnostics.logFile.split(/[\\/]/).pop() : t("settings.about.noLogFile")}
              </span>
              <Button compact
                disabled={!diagnostics.logFileExists}
                onClick={() => onOpenLog("file")}
              >
                {t("settings.about.viewAction")}
              </Button>
            </dd>
          </div>
        </dl>
      </div>
    </section>
  );
}
