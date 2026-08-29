// Keeps Loom-owned settings sections cohesive and separate from Hook settings UI.
import { AboutPanel } from "./AboutPanel";
import { CapabilityExtensionsPanel } from "./capabilities/CapabilityExtensionsPanel";
import { SettingsAccordionSection } from "./settingsModel";
import {
  ArtStoreSettingsPanel,
  GeneralSettingsPanel,
  LoomCacheSettingsPanel,
  McpSettingsPanel,
  NetworkSettingsPanel,
} from "./SettingsPanels";
import type { useSettingsPanelController } from "./useSettingsPanelController";

export function LoomSettingsSections({
  baseUrl,
  controller,
}: {
  baseUrl: string;
  controller: ReturnType<typeof useSettingsPanelController>;
}) {
  const {
    appDiagnostics,
    appPaths,
    artStoreTrustPolicy,
    artStoreTrustPolicyBusy,
    checkApplicationUpdate,
    clearLoomCache,
    draft,
    loomCacheBusyKind,
    loomCacheLoading,
    loomCacheSnapshot,
    openApplicationLog,
    openRepository,
    openSettingsSection,
    setDraft,
    toggleMinimizeToTray,
    toggleSettingsSection,
    updateArtStoreDraft,
    updateArtStoreTrustPolicy,
    updateLoomCacheDraft,
    updateMcpDraft,
    updateNetworkDraft,
  } = controller;

  return (
    <div className="settings-accordion">
      <SettingsAccordionSection id="general" label="常规" open={openSettingsSection === "general"} onToggle={() => toggleSettingsSection("general")}>
        <GeneralSettingsPanel
          appName="loom"
          value={{
            language: draft.general.language,
            theme: draft.general.theme,
            closeToTray: draft.general.minimize_to_tray,
          }}
          onChange={(patch) => {
            if (patch.closeToTray !== undefined) {
              toggleMinimizeToTray(patch.closeToTray);
              return;
            }
            setDraft((current) => ({
              ...current,
              general: {
                ...current.general,
                ...(patch.language === undefined ? {} : { language: patch.language }),
                ...(patch.theme === undefined ? {} : { theme: patch.theme }),
              },
            }));
          }}
        />
      </SettingsAccordionSection>

      <SettingsAccordionSection id="mcp" label="MCP" open={openSettingsSection === "mcp"} onToggle={() => toggleSettingsSection("mcp")}>
        <McpSettingsPanel value={draft.mcp} onChange={updateMcpDraft} />
      </SettingsAccordionSection>

      <SettingsAccordionSection id="art-store" label="Art" open={openSettingsSection === "art-store"} onToggle={() => toggleSettingsSection("art-store")}>
        <ArtStoreSettingsPanel
          value={draft.art_store}
          trustPolicy={artStoreTrustPolicy}
          trustPolicyBusy={artStoreTrustPolicyBusy}
          onChange={updateArtStoreDraft}
          onTrustPolicyChange={(policy) => void updateArtStoreTrustPolicy(policy)}
        />
      </SettingsAccordionSection>

      <SettingsAccordionSection id="capabilities" label="能力扩展" open={openSettingsSection === "capabilities"} onToggle={() => toggleSettingsSection("capabilities")}>
        <CapabilityExtensionsPanel baseUrl={baseUrl} />
      </SettingsAccordionSection>

      <SettingsAccordionSection id="cache" label="缓存" open={openSettingsSection === "cache"} onToggle={() => toggleSettingsSection("cache")}>
        <LoomCacheSettingsPanel
          settings={draft.loom_cache}
          snapshot={loomCacheSnapshot}
          loading={loomCacheLoading}
          busyKind={loomCacheBusyKind}
          onSettingsChange={updateLoomCacheDraft}
          onClear={(kind) => void clearLoomCache(kind)}
        />
      </SettingsAccordionSection>

      <SettingsAccordionSection id="network" label="网络" open={openSettingsSection === "network"} onToggle={() => toggleSettingsSection("network")}>
        <NetworkSettingsPanel
          appName="Loom"
          value={draft.network.loom}
          onChange={(patch) => updateNetworkDraft("loom", patch)}
        />
      </SettingsAccordionSection>

      <SettingsAccordionSection id="about" label="关于" open={openSettingsSection === "about"} onToggle={() => toggleSettingsSection("about")}>
        <AboutPanel
          app="loom"
          diagnostics={{
            ...appDiagnostics.loom,
            logDir: appDiagnostics.loom.logDir || appPaths?.logDir || "",
          }}
          logLevel={draft.system.loom_log_level}
          onLogLevelChange={(logLevel) => setDraft((current) => ({
            ...current,
            system: { ...current.system, loom_log_level: logLevel },
          }))}
          onCheckUpdate={() => checkApplicationUpdate("loom")}
          onOpenLog={(target) => void openApplicationLog("loom", target)}
          onOpenRepository={(url) => void openRepository(url)}
        />
      </SettingsAccordionSection>
    </div>
  );
}
