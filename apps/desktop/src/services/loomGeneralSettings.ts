import { parsePersistedDisplayLanguage, setLanguage } from "../i18n/index.ts";
import type { LoomSettings } from "./loomApi.ts";

export type LoomGeneralSettings = Pick<
  LoomSettings["general"],
  "theme" | "language" | "minimize_to_tray"
>;

export const applyLoomGeneralSettings = (
  settings: LoomGeneralSettings,
  documentRef: Document = document,
): void => {
  const language = parsePersistedDisplayLanguage(settings.language);
  setLanguage(language);
  const root = documentRef.documentElement;
  root.lang = language;
  root.dataset.loomTheme = settings.theme;
  root.style.colorScheme = settings.theme === "system" ? "light dark" : settings.theme;
};
