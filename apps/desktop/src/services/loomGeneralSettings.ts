import type { ArtLoomCompatSettings } from "./loomApi";

export type LoomGeneralSettings = Pick<
  ArtLoomCompatSettings["general"],
  "theme" | "language" | "minimize_to_tray"
>;

export const applyLoomGeneralSettings = (
  settings: LoomGeneralSettings,
  documentRef: Document = document,
): void => {
  const root = documentRef.documentElement;
  root.lang = settings.language === "en" ? "en" : "zh-Hans";
  root.dataset.loomTheme = settings.theme;
  root.style.colorScheme = settings.theme === "system" ? "light dark" : settings.theme;
};
