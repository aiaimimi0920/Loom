// Coordinates settings UI; persistence and shortcut semantics remain in the controller.
import { Tab, TabList, TabPanel, TabsRoot } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
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
  const { t } = useUiText();
  const controller = useSettingsPanelController({ snapshot });
  const { activeSettingsApp, appDiagnostics, checkApplicationUpdate, clearOtterCache, draft,
    otterCacheBusyKind, otterCacheLoading, otterCacheSnapshot, openApplicationLog, openRepository,
    openSettingsSection, selectSettingsApp, setDraft, toggleSettingsSection, updateOtterCacheDraft,
    updateOtterGeneralDraft, updateNetworkDraft } = controller;
  return <TabsRoot asChild value={activeSettingsApp} onValueChange={app => selectSettingsApp(app as SettingsAppId)}>
    <section className="settings-page" aria-labelledby="settings-page-title">
      <header className="settings-page__heading">
        <h1 id="settings-page-title">{t("settings.page.title")}</h1>
        <TabList className="settings-app-tabs" label={t("settings.page.appTabsAria")}>
          <Tab value="loom" onClick={() => selectSettingsApp("loom")}>Loom</Tab>
          <Tab value="hook" onClick={() => selectSettingsApp("hook")}>Otter</Tab>
        </TabList>
      </header>
      <TabPanel className="settings-app-panel" value="loom">
        <LoomSettingsSections baseUrl={snapshot.baseUrl} controller={controller} />
      </TabPanel>
      <TabPanel className="settings-app-panel" value="hook">
        <fieldset className="settings-accordion" disabled={controller.connectionSwitching || controller.settingsLoading}
          style={{ border: 0, padding: 0, margin: 0, minWidth: 0 }}>
          <SettingsAccordionSection id="general" label={t("settings.sections.general")} open={openSettingsSection === "general"} onToggle={() => toggleSettingsSection("general")}>
            <OtterLanguageSettings baseUrl={snapshot.baseUrl} online={snapshot.connectionState === "online"}
              active={activeSettingsApp === "hook" && openSettingsSection === "general"} />
            <GeneralSettingsPanel appName="hook" value={{ theme: draft.hook_general.theme, closeToTray: draft.hook_general.close_to_tray }} onChange={updateOtterGeneralDraft} />
          </SettingsAccordionSection>
          <SettingsAccordionSection id="shortcuts" label={t("settings.sections.shortcuts")} open={openSettingsSection === "shortcuts"} onToggle={() => toggleSettingsSection("shortcuts")}>
            <OtterShortcutSettings controller={controller} />
          </SettingsAccordionSection>
          <SettingsAccordionSection id="cache" label={t("settings.sections.cache")} open={openSettingsSection === "cache"} onToggle={() => toggleSettingsSection("cache")}>
            <OtterCacheSettingsPanel settings={draft.hook_cache} snapshot={otterCacheSnapshot} loading={otterCacheLoading} busyKind={otterCacheBusyKind}
              onSettingsChange={updateOtterCacheDraft} onClear={kind => void clearOtterCache(kind)} />
          </SettingsAccordionSection>
          <SettingsAccordionSection id="network" label={t("settings.sections.network")} open={openSettingsSection === "network"} onToggle={() => toggleSettingsSection("network")}>
            <NetworkSettingsPanel appName="Otter" value={draft.network.hook} onChange={patch => updateNetworkDraft("hook", patch)} />
          </SettingsAccordionSection>
          <SettingsAccordionSection id="about" label={t("settings.sections.about")} open={openSettingsSection === "about"} onToggle={() => toggleSettingsSection("about")}>
            <AboutPanel app="hook" diagnostics={appDiagnostics.hook} logLevel={draft.system.hook_log_level}
              onLogLevelChange={logLevel => setDraft(current => ({ ...current, system: { ...current.system, hook_log_level: logLevel } }))}
              onCheckUpdate={() => checkApplicationUpdate("hook")} onOpenLog={target => void openApplicationLog("hook", target)} onOpenRepository={url => void openRepository(url)} />
          </SettingsAccordionSection>
        </fieldset>
      </TabPanel>
      <ShortcutEditDialog controller={controller} />
      <QuickBindingDialog controller={controller} />
    </section>
  </TabsRoot>;
}
