// Coordinates settings persistence, diagnostics, caches, and shortcuts.
import { LoomSnapshot } from "../../services/loomApi";
import { AboutPanel } from "./AboutPanel";
import {
  gestureShortcutModifiers,
  HOOK_SHORTCUT_GROUPS,
  HookShortcutGroupIcon,
  removeShortcutSlot,
  SettingsAccordionSection,
  SHORTCUT_GESTURE_MODIFIERS,
  ShortcutKeySequence,
  ShortcutSlot,
  shortcutSlotCount,
  ShortcutSlots,
  splitShortcutAlternatives,
  toggleGestureShortcutModifier,
} from "./settingsModel";
import {
  ArtStoreSettingsPanel,
  GeneralSettingsPanel,
  HookCacheSettingsPanel,
  LoomCacheSettingsPanel,
  McpSettingsPanel,
  NetworkSettingsPanel,
} from "./SettingsPanels";
import { createPortal } from "react-dom";
import { useSettingsPanelController } from "./useSettingsPanelController";

export function SettingsPanel({ snapshot }: { snapshot: LoomSnapshot }) {
  const {
    activeSettingsApp,
    appDiagnostics,
    appPaths,
    applyQuickBindingEditor,
    applyShortcutEditor,
    artStoreTrustPolicy,
    artStoreTrustPolicyBusy,
    availableArtToolById,
    availableArtTools,
    checkApplicationUpdate,
    clearHookCache,
    clearLoomCache,
    draft,
    handleQuickBindingCapture,
    handleShortcutCapture,
    hookCacheBusyKind,
    hookCacheLoading,
    hookCacheSnapshot,
    loomCacheBusyKind,
    loomCacheLoading,
    loomCacheSnapshot,
    openApplicationLog,
    openQuickBindingEditor,
    openRepository,
    openSettingsSection,
    openShortcutEditor,
    openShortcutGroups,
    quickBindingConflict,
    quickBindingEditor,
    resolveShortcutKeys,
    selectSettingsApp,
    setDraft,
    setQuickBindingEditor,
    setShortcutEditor,
    shortcutEditor,
    shortcutEditorConflict,
    shortcuts,
    toggleMinimizeToTray,
    toggleSettingsSection,
    toggleShortcutGroup,
    updateArtStoreDraft,
    updateArtStoreTrustPolicy,
    updateHookCacheDraft,
    updateHookGeneralDraft,
    updateLoomCacheDraft,
    updateMcpDraft,
    updateNetworkDraft,
  } = useSettingsPanelController({ snapshot });

  return (
    <section className="settings-page" aria-labelledby="settings-page-title">
      <header className="settings-page__heading">
        <h1 id="settings-page-title">设置</h1>
        <nav className="settings-app-tabs" aria-label="应用设置" role="tablist">
          <button
            className={activeSettingsApp === "loom" ? "settings-app-tab settings-app-tab--active" : "settings-app-tab"}
            type="button"
            role="tab"
            aria-selected={activeSettingsApp === "loom"}
            aria-controls="settings-app-panel-loom"
            onClick={() => selectSettingsApp("loom")}
          >
            Loom
          </button>
          <button
            className={activeSettingsApp === "hook" ? "settings-app-tab settings-app-tab--active" : "settings-app-tab"}
            type="button"
            role="tab"
            aria-selected={activeSettingsApp === "hook"}
            aria-controls="settings-app-panel-hook"
            onClick={() => selectSettingsApp("hook")}
          >
            Hook
          </button>
        </nav>
      </header>

      <div
        className="settings-app-panel"
        id={`settings-app-panel-${activeSettingsApp}`}
        role="tabpanel"
        aria-label={`${activeSettingsApp === "loom" ? "Loom" : "Hook"} 设置`}
      >
      {activeSettingsApp === "loom" ? (
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
      ) : (
        <div className="settings-accordion">
          <SettingsAccordionSection id="general" label="常规" open={openSettingsSection === "general"} onToggle={() => toggleSettingsSection("general")}>
            <GeneralSettingsPanel
              appName="hook"
              value={{
                language: draft.hook_general.language,
                theme: draft.hook_general.theme,
                closeToTray: draft.hook_general.close_to_tray,
              }}
              onChange={updateHookGeneralDraft}
            />
          </SettingsAccordionSection>

          <SettingsAccordionSection id="shortcuts" label="快捷键" open={openSettingsSection === "shortcuts"} onToggle={() => toggleSettingsSection("shortcuts")}>
            <div className="hook-shortcut-groups">
              {HOOK_SHORTCUT_GROUPS.map((group) => {
                const groupOpen = openShortcutGroups.has(group.id);
                const contentId = `hook-shortcut-group-content-${group.id}`;
                return (
                <section className={groupOpen ? "hook-shortcut-group hook-shortcut-group--open" : "hook-shortcut-group"} key={group.id} aria-labelledby={`hook-shortcut-group-${group.id}`}>
                  <header className="hook-shortcut-group__header">
                    <h3 id={`hook-shortcut-group-${group.id}`}>
                      <button
                        className="hook-shortcut-group__trigger"
                        type="button"
                        aria-expanded={groupOpen}
                        aria-controls={contentId}
                        onClick={() => toggleShortcutGroup(group.id)}
                      >
                        <HookShortcutGroupIcon kind={group.icon} />
                        <span>{group.label}</span>
                        <svg className="hook-shortcut-group__chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
                          <path d="m7 9 5 5 5-5" />
                        </svg>
                      </button>
                    </h3>
                  </header>
                  {groupOpen ? <div className="hook-shortcut-list" id={contentId}>
                    {group.items.map((item) => (
                      <div className="hook-shortcut-row" key={item.id}>
                        <span className="hook-shortcut-row__text">
                          <strong>{item.label}</strong>
                          <small>{item.description}</small>
                        </span>
                        {item.sourceId ? (
                          <button
                            className="hook-shortcut-row__keys hook-shortcut-row__keys--editable"
                            type="button"
                            aria-label={`修改${item.label}快捷键`}
                            title="修改快捷键"
                            onClick={() => openShortcutEditor(item)}
                          >
                            <ShortcutKeySequence shortcuts={resolveShortcutKeys(item)} />
                            <svg className="hook-shortcut-row__edit-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
                              <path d="M4 20h4l11-11-4-4L4 16v4Z" /><path d="m13.5 6.5 4 4" />
                            </svg>
                          </button>
                        ) : (
                          <span className="hook-shortcut-row__keys" aria-label={`${item.label}操作手势`}>
                            <ShortcutKeySequence shortcuts={item.keys} />
                          </span>
                        )}
                      </div>
                    ))}
                    {group.id === "panels-tools" ? (
                      <>
                        {draft.quick_bindings
                          .filter((binding) => availableArtToolById.has(binding.art))
                          .map((binding) => {
                            const tool = availableArtToolById.get(binding.art)!;
                            return (
                              <div className="hook-shortcut-row hook-shortcut-row--art-binding" key={binding.id}>
                                <span className="hook-shortcut-row__text">
                                  <strong>{tool.name || tool.id}</strong>
                                  <small>快速添加 Art 节点</small>
                                </span>
                                <span className="hook-quick-binding-actions">
                                  <button
                                    className="hook-shortcut-row__keys hook-shortcut-row__keys--editable"
                                    type="button"
                                    aria-label={`修改${tool.name || tool.id}快捷键`}
                                    onClick={() => openQuickBindingEditor(binding)}
                                  >
                                    <ShortcutKeySequence shortcuts={splitShortcutAlternatives(binding.key)} />
                                    <svg className="hook-shortcut-row__edit-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
                                      <path d="M4 20h4l11-11-4-4L4 16v4Z" /><path d="m13.5 6.5 4 4" />
                                    </svg>
                                  </button>
                                  <button
                                    className="hook-quick-binding-remove"
                                    type="button"
                                    aria-label={`删除${tool.name || tool.id}快捷键`}
                                    title="删除"
                                    onClick={() => setDraft((current) => ({
                                      ...current,
                                      quick_bindings: current.quick_bindings.filter((item) => item.id !== binding.id),
                                    }))}
                                  >
                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
                                      <path d="M4 7h16M9 7V4h6v3M7 7l1 13h8l1-13M10 11v5M14 11v5" />
                                    </svg>
                                  </button>
                                </span>
                              </div>
                            );
                          })}
                        <button
                          className="hook-shortcut-add-art"
                          type="button"
                          disabled={!availableArtTools.length}
                          onClick={() => openQuickBindingEditor()}
                        >
                          <span aria-hidden="true">＋</span>
                          <strong>添加 Art 快捷键</strong>
                        </button>
                      </>
                    ) : null}
                  </div> : null}
                </section>
              )})}
            </div>
          </SettingsAccordionSection>

          <SettingsAccordionSection id="cache" label="缓存" open={openSettingsSection === "cache"} onToggle={() => toggleSettingsSection("cache")}>
            <HookCacheSettingsPanel
              settings={draft.hook_cache}
              snapshot={hookCacheSnapshot}
              loading={hookCacheLoading}
              busyKind={hookCacheBusyKind}
              onSettingsChange={updateHookCacheDraft}
              onClear={(kind) => void clearHookCache(kind)}
            />
          </SettingsAccordionSection>

          <SettingsAccordionSection id="network" label="网络" open={openSettingsSection === "network"} onToggle={() => toggleSettingsSection("network")}>
            <NetworkSettingsPanel
              appName="Hook"
              value={draft.network.hook}
              onChange={(patch) => updateNetworkDraft("hook", patch)}
            />
          </SettingsAccordionSection>

          <SettingsAccordionSection id="about" label="关于" open={openSettingsSection === "about"} onToggle={() => toggleSettingsSection("about")}>
            <AboutPanel
              app="hook"
              diagnostics={appDiagnostics.hook}
              logLevel={draft.system.hook_log_level}
              onLogLevelChange={(logLevel) => setDraft((current) => ({
                ...current,
                system: { ...current.system, hook_log_level: logLevel },
              }))}
              onCheckUpdate={() => checkApplicationUpdate("hook")}
              onOpenLog={(target) => void openApplicationLog("hook", target)}
              onOpenRepository={(url) => void openRepository(url)}
            />
          </SettingsAccordionSection>
        </div>
      )}
      </div>
      {shortcutEditor ? createPortal(
        <div className="framework-dialog-backdrop" role="presentation" onMouseDown={(event) => {
          if (event.target === event.currentTarget) setShortcutEditor(null);
        }}>
          <section
            className="framework-dialog shortcut-edit-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="shortcut-edit-dialog-title"
            onKeyDown={(event) => {
              if (event.key === "Escape") setShortcutEditor(null);
            }}
          >
            <header className="framework-dialog__header">
              <div>
                <h2 id="shortcut-edit-dialog-title">修改快捷键</h2>
                <p>{shortcutEditor.item.label}</p>
              </div>
              <button className="art-card-action" type="button" aria-label="关闭" onClick={() => setShortcutEditor(null)}>×</button>
            </header>
            <div className="shortcut-edit-dialog__body">
              {Array.from({ length: shortcutEditor.slotCount }, (_, index) => index as ShortcutSlot).map((slot) => {
                const keys = shortcutEditor.keys[slot];
                return (
                  <div className="shortcut-capture-slot" key={slot}>
                    <span>快捷键 {slot + 1}</span>
                    <div className="shortcut-capture-slot__controls">
                      {shortcutEditor.item.gestureAction ? (
                        <div className="shortcut-gesture-picker" role="group" aria-label={`${shortcutEditor.item.label}快捷键 ${slot + 1}`}>
                          <span className="shortcut-gesture-picker__action">{shortcutEditor.item.gestureAction}</span>
                          <span className="shortcut-gesture-picker__modifiers">
                            {SHORTCUT_GESTURE_MODIFIERS.map((modifier) => {
                              const selected = gestureShortcutModifiers(keys, shortcutEditor.item.gestureAction!).has(modifier);
                              return (
                                <button
                                  className={selected ? "shortcut-gesture-modifier shortcut-gesture-modifier--active" : "shortcut-gesture-modifier"}
                                  type="button"
                                  aria-pressed={selected}
                                  key={modifier}
                                  onClick={() => setShortcutEditor((current) => current?.item.gestureAction ? {
                                    ...current,
                                    activeSlot: slot,
                                    keys: current.keys.map((value, keyIndex) => keyIndex === slot
                                      ? toggleGestureShortcutModifier(value, current.item.gestureAction!, modifier)
                                      : value) as ShortcutSlots,
                                  } : current)}
                                >{modifier}</button>
                              );
                            })}
                          </span>
                        </div>
                      ) : (
                        <button
                          className={shortcutEditor.activeSlot === slot ? "shortcut-capture-field shortcut-capture-field--active" : "shortcut-capture-field"}
                          type="button"
                          autoFocus={slot === shortcutEditor.activeSlot}
                          onFocus={() => setShortcutEditor((current) => current ? { ...current, activeSlot: slot } : current)}
                          onKeyDown={(event) => handleShortcutCapture(event, slot)}
                        >
                          {keys ? <ShortcutKeySequence shortcuts={[keys]} /> : <strong>未设置</strong>}
                          <small>按下新的组合键</small>
                        </button>
                      )}
                      <button
                        className="shortcut-capture-clear"
                        type="button"
                        disabled={slot === 0 && !keys}
                        onClick={() => setShortcutEditor((current) => current ? {
                          ...current,
                          activeSlot: slot > 0 ? 0 : current.activeSlot,
                          slotCount: slot > 0 ? Math.max(1, current.slotCount - 1) as 1 | 2 | 3 : current.slotCount,
                          keys: removeShortcutSlot(current.keys, slot),
                        } : current)}
                      >{slot > 0 ? "删除" : "清除"}</button>
                    </div>
                  </div>
                );
              })}
              {shortcutEditor.slotCount < 3 ? (
                <button
                  className="shortcut-add-secondary"
                  type="button"
                  onClick={() => setShortcutEditor((current) => current ? {
                    ...current,
                    activeSlot: current.slotCount as ShortcutSlot,
                    slotCount: (current.slotCount + 1) as 2 | 3,
                  } : current)}
                >＋ 添加额外快捷键</button>
              ) : null}
              {shortcutEditorConflict ? (
                <p className="shortcut-edit-dialog__conflict">{shortcutEditorConflict}</p>
              ) : null}
            </div>
            <footer className="shortcut-edit-dialog__footer">
              <button className="ghost-button" type="button" onClick={() => setShortcutEditor((current) => current ? {
                ...current,
                keys: [current.item.keys[0] || "", current.item.keys[1] || "", current.item.keys[2] || ""],
                activeSlot: 0,
                slotCount: shortcutSlotCount(current.item.keys),
              } : current)}>恢复默认</button>
              <span />
              <button className="ghost-button" type="button" onClick={() => setShortcutEditor(null)}>取消</button>
              <button className="signal-button" type="button" disabled={!shortcutEditor.keys.some((keys) => keys.trim()) || Boolean(shortcutEditorConflict)} onClick={applyShortcutEditor}>应用</button>
            </footer>
          </section>
        </div>,
        document.body,
      ) : null}
      {quickBindingEditor ? createPortal(
        <div className="framework-dialog-backdrop" role="presentation" onMouseDown={(event) => {
          if (event.target === event.currentTarget) setQuickBindingEditor(null);
        }}>
          <section
            className="framework-dialog shortcut-edit-dialog quick-binding-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="quick-binding-dialog-title"
            onKeyDown={(event) => {
              if (event.key === "Escape") setQuickBindingEditor(null);
            }}
          >
            <header className="framework-dialog__header">
              <div>
                <h2 id="quick-binding-dialog-title">Art 快捷键</h2>
                <p>选择要快速添加的 Art</p>
              </div>
              <button className="art-card-action" type="button" aria-label="关闭" onClick={() => setQuickBindingEditor(null)}>×</button>
            </header>
            <div className="shortcut-edit-dialog__body">
              <label className="quick-binding-dialog__art">
                <span>Art</span>
                <select
                  className="studio-input"
                  value={quickBindingEditor.art}
                  onChange={(event) => setQuickBindingEditor((current) => current ? { ...current, art: event.target.value } : current)}
                >
                  {availableArtTools.map((tool) => <option value={tool.id} key={tool.id}>{tool.name || tool.id}</option>)}
                </select>
              </label>
              {Array.from({ length: quickBindingEditor.slotCount }, (_, index) => index as ShortcutSlot).map((slot) => {
                const keys = quickBindingEditor.keys[slot];
                return (
                  <div className="shortcut-capture-slot" key={slot}>
                    <span>快捷键 {slot + 1}</span>
                    <div className="shortcut-capture-slot__controls">
                      <button
                        className={quickBindingEditor.activeSlot === slot ? "shortcut-capture-field shortcut-capture-field--active" : "shortcut-capture-field"}
                        type="button"
                        autoFocus={slot === quickBindingEditor.activeSlot}
                        onFocus={() => setQuickBindingEditor((current) => current ? { ...current, activeSlot: slot } : current)}
                        onKeyDown={(event) => handleQuickBindingCapture(event, slot)}
                      >
                        {keys ? <ShortcutKeySequence shortcuts={[keys]} /> : <strong>未设置</strong>}
                        <small>按下新的组合键</small>
                      </button>
                      <button
                        className="shortcut-capture-clear"
                        type="button"
                        disabled={slot === 0 && !keys}
                        onClick={() => setQuickBindingEditor((current) => current ? {
                          ...current,
                          activeSlot: slot > 0 ? 0 : current.activeSlot,
                          slotCount: slot > 0 ? Math.max(1, current.slotCount - 1) as 1 | 2 | 3 : current.slotCount,
                          keys: removeShortcutSlot(current.keys, slot),
                        } : current)}
                      >{slot > 0 ? "删除" : "清除"}</button>
                    </div>
                  </div>
                );
              })}
              {quickBindingEditor.slotCount < 3 ? (
                <button
                  className="shortcut-add-secondary"
                  type="button"
                  onClick={() => setQuickBindingEditor((current) => current ? {
                    ...current,
                    activeSlot: current.slotCount as ShortcutSlot,
                    slotCount: (current.slotCount + 1) as 2 | 3,
                  } : current)}
                >＋ 添加额外快捷键</button>
              ) : null}
              {quickBindingConflict ? <p className="shortcut-edit-dialog__conflict">{quickBindingConflict}</p> : null}
            </div>
            <footer className="shortcut-edit-dialog__footer shortcut-edit-dialog__footer--simple">
              <span />
              <span />
              <button className="ghost-button" type="button" onClick={() => setQuickBindingEditor(null)}>取消</button>
              <button
                className="signal-button"
                type="button"
                disabled={!quickBindingEditor.art || !quickBindingEditor.keys.some((keys) => keys.trim()) || Boolean(quickBindingConflict)}
                onClick={applyQuickBindingEditor}
              >应用</button>
            </footer>
          </section>
        </div>,
        document.body,
      ) : null}
    </section>
  );
}
