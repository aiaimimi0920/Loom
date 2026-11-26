// Displays the application's shortcut catalog; key resolution and edits remain controller-owned.
import { Button, Disclosure, IconButton } from "@neuro/ui";
import { useUiText } from "../../i18n/index.ts";
import { HOOK_SHORTCUT_GROUPS, OtterShortcutGroupIcon, ShortcutKeySequence, splitShortcutAlternatives } from "./settingsModel";
import type { useSettingsPanelController } from "./useSettingsPanelController";

export function OtterShortcutSettings({ controller }: { controller: ReturnType<typeof useSettingsPanelController> }) {
  const { t, tf } = useUiText();
  const { availableArtToolById, availableArtTools, draft, openShortcutEditor, openQuickBindingEditor,
    openShortcutGroups, resolveShortcutKeys, setDraft, toggleShortcutGroup } = controller;
  return <div className="otter-shortcut-groups">
    {HOOK_SHORTCUT_GROUPS.map(group => {
      const open = openShortcutGroups.has(group.id);
      return <Disclosure key={group.id} className="otter-shortcut-group" open={open} onOpenChange={() => toggleShortcutGroup(group.id)}
        title={<h3 className="otter-shortcut-group__heading"><OtterShortcutGroupIcon kind={group.icon} /><span>{group.label}</span></h3>}>
        {open ? <div className="otter-shortcut-list">
          {group.items.map(item => <div className="otter-shortcut-row" key={item.id}>
            <span className="otter-shortcut-row__text"><strong>{item.label}</strong><small>{item.description}</small></span>
            {item.sourceId ? <Button compact variant="quiet" className="otter-shortcut-row__keys" aria-label={tf("settings.shortcuts.editShortcutAria", { label: item.label })}
              onClick={() => openShortcutEditor(item)}><ShortcutKeySequence shortcuts={resolveShortcutKeys(item)} /></Button>
              : <span className="otter-shortcut-row__keys" aria-label={tf("settings.shortcuts.gestureAria", { label: item.label })}><ShortcutKeySequence shortcuts={item.keys} /></span>}
          </div>)}
          {group.id === "panels-tools" ? <>
            {draft.quick_bindings.filter(binding => availableArtToolById.has(binding.art)).map(binding => {
              const tool = availableArtToolById.get(binding.art)!;
              const toolLabel = tool.name || tool.id;
              return <div className="otter-shortcut-row otter-shortcut-row--art-binding" key={binding.id}>
                <span className="otter-shortcut-row__text"><strong>{toolLabel}</strong><small>{t("settings.shortcuts.quickAddArtDescription")}</small></span>
                <span className="otter-quick-binding-actions">
                  <Button compact variant="quiet" className="otter-shortcut-row__keys" aria-label={tf("settings.shortcuts.editShortcutAria", { label: toolLabel })}
                    onClick={() => openQuickBindingEditor(binding)}><ShortcutKeySequence shortcuts={splitShortcutAlternatives(binding.key)} /></Button>
                  <IconButton compact variant="quiet" label={tf("settings.shortcuts.deleteShortcutAria", { label: toolLabel })} onClick={() => setDraft(current => ({
                    ...current, quick_bindings: current.quick_bindings.filter(item => item.id !== binding.id),
                  }))}><svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
                    <path d="M4 7h16M9 7V4h6v3M7 7l1 13h8l1-13M10 11v5M14 11v5" />
                  </svg></IconButton>
                </span>
              </div>;
            })}
            <Button compact className="otter-shortcut-add-art" disabled={!availableArtTools.length} onClick={() => openQuickBindingEditor()}>{t("settings.shortcuts.addArtShortcut")}</Button>
          </> : null}
        </div> : null}
      </Disclosure>;
    })}
  </div>;
}
