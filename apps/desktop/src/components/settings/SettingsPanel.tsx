// Coordinates settings UI; persistence and shortcut semantics remain in the controller.
import { Tab, TabList, TabPanel, TabsRoot } from "@neuro/ui";
import { LoomSnapshot } from "../../services/loomApi";
import { AboutPanel } from "./AboutPanel";
import { SettingsAccordionSection, type SettingsAppId } from "./settingsModel";
import { GeneralSettingsPanel, OtterCacheSettingsPanel, NetworkSettingsPanel } from "./SettingsPanels";
import { LoomSettingsSections } from "./LoomSettingsSections";
import { OtterShortcutSettings } from "./OtterShortcutSettings";
import { OtterLanguageSettings } from "./OtterLanguageSettings";
import { QuickBindingDialog, ShortcutEditDialog } from "./SettingsShortcutDialogs";
import { useSettingsPanelController } from "./useSettingsPanelController";

export function SettingsPanel({ snapshot }: { snapshot: LoomSnapshot }) {
  const controller = useSettingsPanelController({ snapshot });
  const { activeSettingsApp, appDiagnostics, checkApplicationUpdate, clearOtterCache, draft,
    otterCacheBusyKind, otterCacheLoading, otterCacheSnapshot, openApplicationLog, openRepository,
    openSettingsSection, selectSettingsApp, setDraft, toggleSettingsSection, updateOtterCacheDraft,
    updateOtterGeneralDraft, updateNetworkDraft } = controller;
  return <TabsRoot asChild value={activeSettingsApp} onValueChange={app => selectSettingsApp(app as SettingsAppId)}>
    <section className="settings-page" aria-labelledby="settings-page-title">
      <header className="settings-page__heading">
        <h1 id="settings-page-title">设置</h1>
        <TabList className="settings-app-tabs" label="应用设置">
          <Tab value="loom" onClick={() => selectSettingsApp("loom")}>Loom</Tab>
          <Tab value="hook" onClick={() => selectSettingsApp("hook")}>Otter</Tab>
        </TabList>
      </header>
      <TabPanel className="settings-app-panel" value="loom">
        <LoomSettingsSections baseUrl={snapshot.baseUrl} controller={controller} />
      </TabPanel>
      <TabPanel className="settings-app-panel" value="hook">
        <div className="settings-accordion">
          <SettingsAccordionSection id="general" label="常规" open={openSettingsSection === "general"} onToggle={() => toggleSettingsSection("general")}>
            <OtterLanguageSettings baseUrl={snapshot.baseUrl} online={snapshot.connectionState === "online"}
              active={activeSettingsApp === "hook" && openSettingsSection === "general"} />
            <GeneralSettingsPanel appName="hook" value={{ theme: draft.hook_general.theme, closeToTray: draft.hook_general.close_to_tray }} onChange={updateOtterGeneralDraft} />
          </SettingsAccordionSection>
          <SettingsAccordionSection id="shortcuts" label="快捷键" open={openSettingsSection === "shortcuts"} onToggle={() => toggleSettingsSection("shortcuts")}>
            <OtterShortcutSettings controller={controller} />
          </SettingsAccordionSection>
          <SettingsAccordionSection id="cache" label="缓存" open={openSettingsSection === "cache"} onToggle={() => toggleSettingsSection("cache")}>
            <OtterCacheSettingsPanel settings={draft.hook_cache} snapshot={otterCacheSnapshot} loading={otterCacheLoading} busyKind={otterCacheBusyKind}
              onSettingsChange={updateOtterCacheDraft} onClear={kind => void clearOtterCache(kind)} />
          </SettingsAccordionSection>
          <SettingsAccordionSection id="network" label="网络" open={openSettingsSection === "network"} onToggle={() => toggleSettingsSection("network")}>
            <NetworkSettingsPanel appName="Otter" value={draft.network.hook} onChange={patch => updateNetworkDraft("hook", patch)} />
          </SettingsAccordionSection>
          <SettingsAccordionSection id="about" label="关于" open={openSettingsSection === "about"} onToggle={() => toggleSettingsSection("about")}>
            <AboutPanel app="hook" diagnostics={appDiagnostics.hook} logLevel={draft.system.hook_log_level}
              onLogLevelChange={logLevel => setDraft(current => ({ ...current, system: { ...current.system, hook_log_level: logLevel } }))}
              onCheckUpdate={() => checkApplicationUpdate("hook")} onOpenLog={target => void openApplicationLog("hook", target)} onOpenRepository={url => void openRepository(url)} />
          </SettingsAccordionSection>
        </div>
      </TabPanel>
      <ShortcutEditDialog controller={controller} />
      <QuickBindingDialog controller={controller} />
    </section>
  </TabsRoot>;
}
