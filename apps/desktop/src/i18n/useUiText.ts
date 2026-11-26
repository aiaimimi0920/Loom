// React hook binding components to Loom's active DisplayLanguage without restarting.

import { useSyncExternalStore } from "react";
import { type MessageArguments } from "./catalog.ts";
import { type DisplayLanguage } from "./displayLanguage.ts";
import { currentLanguage, setLanguage, subscribeLanguage, uiFormatFor, uiTextFor } from "./uiText.ts";

export interface UiTextHandle {
  readonly language: DisplayLanguage;
  readonly setLanguage: (language: DisplayLanguage) => void;
  readonly t: (id: string) => string;
  readonly tf: (id: string, args: MessageArguments) => string;
}

export function useUiText(): UiTextHandle {
  const language = useSyncExternalStore(subscribeLanguage, currentLanguage, currentLanguage);
  return {
    language,
    setLanguage,
    t: (id: string) => uiTextFor(language, id),
    tf: (id: string, args: MessageArguments) => uiFormatFor(language, id, args),
  };
}
