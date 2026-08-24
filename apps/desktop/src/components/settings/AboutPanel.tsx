// Renders product identity and application diagnostics.
import { LoomMark } from "../app/appShell";
import { ApplicationDiagnosticsInfo, SettingsAppId, SettingsSectionIcon } from "./settingsModel";

export function ApplicationAboutMark({ app }: { app: SettingsAppId }) {
  if (app === "loom") {
    return <LoomMark />;
  }
  return (
    <svg className="about-panel__hook-mark" viewBox="0 0 1024 1024" aria-hidden="true">
      <defs>
        <filter id="about-hook-green-glow" x="-90%" y="-90%" width="280%" height="280%">
          <feGaussianBlur stdDeviation="18" result="blur" />
          <feColorMatrix in="blur" type="matrix" values="0 0 0 0 0.13  0 0 0 0 0.77  0 0 0 0 0.37  0 0 0 0.78 0" />
          <feMerge><feMergeNode /><feMergeNode in="SourceGraphic" /></feMerge>
        </filter>
        <filter id="about-hook-yellow-glow" x="-90%" y="-90%" width="280%" height="280%">
          <feGaussianBlur stdDeviation="18" result="blur" />
          <feColorMatrix in="blur" type="matrix" values="0 0 0 0 1  0 0 0 0 0.9  0 0 0 0 0  0 0 0 0.82 0" />
          <feMerge><feMergeNode /><feMergeNode in="SourceGraphic" /></feMerge>
        </filter>
      </defs>
      <g fill="none" strokeLinecap="round" strokeLinejoin="round">
        <path d="M250 394V250h144" stroke="var(--loom-brand-primary)" strokeWidth="72" filter="url(#about-hook-green-glow)" />
        <path d="M774 394V250H630" stroke="var(--loom-brand-primary)" strokeWidth="72" filter="url(#about-hook-green-glow)" />
        <path d="M250 630v144h144" stroke="var(--loom-brand-primary)" strokeWidth="72" filter="url(#about-hook-green-glow)" />
        <path d="M774 630v144H630" stroke="var(--loom-brand-secondary)" strokeWidth="72" filter="url(#about-hook-yellow-glow)" />
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
            <dt>应用名称</dt>
            <dd>{diagnostics.appName}</dd>
          </div>
          <div>
            <dt>版本号</dt>
            <dd>{diagnostics.version}</dd>
          </div>
          <div>
            <dt>检查更新</dt>
            <dd><button className="ghost-button" type="button" onClick={onCheckUpdate}>立即检查</button></dd>
          </div>
          {diagnostics.repositoryUrl ? (
            <div className="about-panel__repository-row">
              <dt>仓库</dt>
              <dd>
                <button
                  className="about-panel__repository-link"
                  type="button"
                  title={`打开 ${diagnostics.repositoryUrl}`}
                  onClick={() => onOpenRepository(diagnostics.repositoryUrl!)}
                >
                  <span>{diagnostics.repositoryUrl}</span>
                  <svg viewBox="0 0 24 24" aria-hidden="true">
                    <path d="M14 5h5v5M12 12l7-7M19 13v5a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1h5" />
                  </svg>
                </button>
                <span className="about-panel__commit" title="构建对应的 Git 提交">
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
          <h3>诊断日志</h3>
        </div>
        <dl className="about-panel__rows">
          <div>
            <dt>日志级别</dt>
            <dd>
              <select
                className="studio-input about-panel__log-level"
                aria-label={`${diagnostics.appName} 日志级别`}
                value={logLevel}
                onChange={(event) => onLogLevelChange(event.target.value)}
              >
                <option value="error">Error</option>
                <option value="warn">Warn</option>
                <option value="info">Info（默认）</option>
                <option value="debug">Debug</option>
              </select>
            </dd>
          </div>
          <div className="about-panel__path-row">
            <dt>日志位置</dt>
            <dd>
              <code title={diagnostics.logDir || undefined}>{diagnostics.logDir || "未加载"}</code>
              <button className="ghost-button" type="button" onClick={() => onOpenLog("directory")}>打开文件夹</button>
            </dd>
          </div>
          <div>
            <dt>查看日志</dt>
            <dd>
              <span className="about-panel__log-file">
                {diagnostics.logFile ? diagnostics.logFile.split(/[\\/]/).pop() : "暂无日志"}
              </span>
              <button
                className="ghost-button"
                type="button"
                disabled={!diagnostics.logFileExists}
                onClick={() => onOpenLog("file")}
              >
                查看
              </button>
            </dd>
          </div>
        </dl>
      </div>
    </section>
  );
}
