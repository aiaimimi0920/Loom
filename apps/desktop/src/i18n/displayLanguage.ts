// Supported Loom display languages modeled on Otter's DisplayLanguage contract.

export type DisplayLanguage = "en" | "zh-Hans";

export const DISPLAY_LANGUAGES: readonly DisplayLanguage[] = ["en", "zh-Hans"] as const;

export const DEFAULT_DISPLAY_LANGUAGE: DisplayLanguage = "en";

const NATIVE_LANGUAGE_NAMES: Readonly<Record<DisplayLanguage, string>> = {
  en: "English",
  "zh-Hans": "\u7b80\u4f53\u4e2d\u6587",
};

export function displayLanguageName(language: DisplayLanguage): string {
  return NATIVE_LANGUAGE_NAMES[language];
}

export function parseDisplayLanguage(value: string | null | undefined): DisplayLanguage | null {
  if (value === "en" || value === "zh-Hans") {
    return value;
  }
  return null;
}

/**
 * Normalizes persisted or browser-supplied language tags onto canonical DisplayLanguage values.
 * Legacy regional tags such as `zh-CN` map to `zh-Hans`; unrecognized values fall back to `en`.
 */
export function parsePersistedDisplayLanguage(value: string | null | undefined): DisplayLanguage {
  const trimmed = value?.trim();
  if (!trimmed) {
    return DEFAULT_DISPLAY_LANGUAGE;
  }
  const exact = parseDisplayLanguage(trimmed);
  if (exact) {
    return exact;
  }
  const normalized = trimmed.replace(/_/g, "-").toLowerCase();
  if (normalized === "zh" || normalized === "zh-cn" || normalized.startsWith("zh-hans")) {
    return "zh-Hans";
  }
  if (normalized === "en" || normalized.startsWith("en-")) {
    return "en";
  }
  return DEFAULT_DISPLAY_LANGUAGE;
}

export function resolveCatalogLocale(language: DisplayLanguage): "en-US" | "zh-CN" {
  return language === "zh-Hans" ? "zh-CN" : "en-US";
}
