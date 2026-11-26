export { Catalogs, type MessageArguments } from "./catalog.ts";
export { type LocaleShard, mergeLocaleShards, parseMessageTemplate } from "./contract.ts";
export {
  DEFAULT_DISPLAY_LANGUAGE,
  DISPLAY_LANGUAGES,
  type DisplayLanguage,
  displayLanguageName,
  parseDisplayLanguage,
  parsePersistedDisplayLanguage,
  resolveCatalogLocale,
} from "./displayLanguage.ts";
export {
  currentLanguage,
  setLanguage,
  subscribeLanguage,
  uiFormat,
  uiFormatFor,
  uiText,
  uiTextFor,
} from "./uiText.ts";
export { type UiTextHandle, useUiText } from "./useUiText.ts";
