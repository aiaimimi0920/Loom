// Module-level interface text store with English source contract and live subscription support.

import { Catalogs, type MessageArguments } from "./catalog.ts";
import { DEFAULT_DISPLAY_LANGUAGE, type DisplayLanguage } from "./displayLanguage.ts";

let activeLanguage: DisplayLanguage = DEFAULT_DISPLAY_LANGUAGE;
let embeddedCatalogs: Catalogs | null = null;
const listeners = new Set<() => void>();

function catalogs(): Catalogs {
  if (!embeddedCatalogs) {
    embeddedCatalogs = Catalogs.embedded();
  }
  return embeddedCatalogs;
}

export function currentLanguage(): DisplayLanguage {
  return activeLanguage;
}

export function setLanguage(language: DisplayLanguage): void {
  catalogs();
  if (activeLanguage === language) {
    return;
  }
  activeLanguage = language;
  for (const listener of listeners) {
    listener();
  }
}

export function subscribeLanguage(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function uiText(id: string): string {
  return catalogs().text(activeLanguage, id);
}

export function uiTextFor(language: DisplayLanguage, id: string): string {
  return catalogs().text(language, id);
}

export function uiFormat(id: string, args: MessageArguments): string {
  return catalogs().format(activeLanguage, id, args);
}

export function uiFormatFor(language: DisplayLanguage, id: string, args: MessageArguments): string {
  return catalogs().format(language, id, args);
}
